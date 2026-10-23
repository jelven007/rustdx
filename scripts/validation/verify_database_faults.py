#!/usr/bin/env python3
"""Real isolated database writes, TCP outage/lost acknowledgement, SIGKILL, recovery.

The proxy affects only this script's clients. No system network settings or existing
tables are changed. Mongo SIGKILL pauses an importer wrapper after the real tool
succeeds and before its exit status reaches rustdx, then terminates that wrapper.
"""
import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path
import platform
import signal
import socket
import struct
import subprocess
import sys
import tempfile
import threading
import time
import urllib.parse
import urllib.request
import uuid
import shutil


class FaultProxy:
    def __init__(self, host, port, mode):
        self.target = (host, port)
        self.mode = mode
        self.armed = threading.Event()
        self.triggered = threading.Event()
        self.stopped = threading.Event()
        self.connections = []
        self.lock = threading.Lock()
        self.listener = socket.socket()
        self.listener.bind(("127.0.0.1", 0))
        self.listener.listen(32)
        self.listener.settimeout(0.1)
        self.port = self.listener.getsockname()[1]
        threading.Thread(target=self.accept, daemon=True).start()

    def accept(self):
        while not self.stopped.is_set():
            try:
                client, _ = self.listener.accept()
            except socket.timeout:
                continue
            except OSError:
                return
            if self.triggered.is_set() and self.mode == "drop":
                client.close()
                continue
            try:
                server = socket.create_connection(self.target, timeout=5)
                server.settimeout(None)
            except OSError:
                client.close()
                continue
            with self.lock:
                self.connections += [client, server]
            state = {"uploaded": 0}
            threading.Thread(target=self.pipe, args=(client, server, state, False), daemon=True).start()
            threading.Thread(target=self.pipe, args=(server, client, state, True), daemon=True).start()

    def pipe(self, source, target, state, downstream):
        try:
            while not self.stopped.is_set():
                data = source.recv(65536)
                if not data:
                    break
                if (downstream and self.mode != "pass" and self.armed.is_set()
                        and state["uploaded"] >= 128 * 1024):
                    # A server response after a substantial write request. Drop
                    # the response, or hold it while the client is killed.
                    self.triggered.set()
                    if self.mode == "drop":
                        self.disconnect()
                        return
                    self.stopped.wait(45)
                    return
                if not downstream:
                    state["uploaded"] += len(data)
                target.sendall(data)
        except OSError:
            pass
        finally:
            for peer in (source, target):
                try:
                    peer.shutdown(socket.SHUT_RDWR)
                except OSError:
                    pass
                peer.close()
                with self.lock:
                    if peer in self.connections:
                        self.connections.remove(peer)

    def disconnect(self):
        with self.lock:
            peers = self.connections[:]
        for peer in peers:
            try:
                peer.shutdown(socket.SHUT_RDWR)
            except OSError:
                pass

    def close(self):
        self.stopped.set()
        self.listener.close()
        self.disconnect()


def wait_for(predicate, child, timeout=30):
    deadline = time.monotonic() + timeout
    while not predicate():
        if child.poll() is not None:
            raise AssertionError(f"rustdx exited before injection: {child.returncode}")
        if time.monotonic() > deadline:
            raise AssertionError("fault injection was not reached")
        time.sleep(0.005)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if os.name != "posix":
        parser.error("SIGKILL acceptance requires POSIX")
    endpoints = {}
    for backend, variable in [("clickhouse", "CLICKHOUSE_URL"), ("mongodb", "MONGODB_URI")]:
        value = os.environ.get(variable)
        if not value:
            parser.error(f"explicit {variable} required")
        url = urllib.parse.urlsplit(value)
        if url.hostname not in ["127.0.0.1", "localhost"] or url.username or url.password:
            parser.error("fault harness requires isolated, unauthenticated loopback services")
        endpoints[backend] = (url.hostname, url.port or (8123 if backend == "clickhouse" else 27017))
    importer = shutil.which("mongoimport")
    assert importer and shutil.which("mongosh"), "MongoDB tools required"
    binary = args.binary.resolve()
    scratch = Path(tempfile.mkdtemp(prefix="rustdx-db-faults-"))
    source = scratch / "source"
    source.mkdir()
    rows = 8192
    for index in range(16):
        date = datetime.date(2024, 1, 2)
        content = bytearray()
        for _ in range(rows // 16):
            while date.weekday() >= 5:
                date += datetime.timedelta(days=1)
            content += struct.pack("<5If2I", int(date.strftime("%Y%m%d")),
                                   1000, 1100, 900, 1050, 1234.5, 12345, 0)
            date += datetime.timedelta(days=1)
        (source / f"sh{600000 + index}.day").write_bytes(content)
    database = "rustdx_fault_acceptance_" + uuid.uuid4().hex
    result = {
        "platform": platform.platform(), "scratch": str(scratch), "database": database,
        "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
        "rows": rows, "cases": [],
        "scope": "owned_loopback_proxy_outage; SIGKILL_after_server_write_before_client_ack; recovery_exact_readback",
    }

    def save():
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(result, indent=2) + "\n")

    def ch(sql):
        url = os.environ["CLICKHOUSE_URL"]
        url += ("&" if "?" in url else "?") + "output_format_parallel_formatting=0"
        with urllib.request.urlopen(
            urllib.request.Request(url, sql.encode()), timeout=30
        ) as response:
            return response.read().decode().strip()

    def mongo(script):
        return subprocess.check_output(
            ["mongosh", os.environ["MONGODB_URI"], "--quiet", "--eval", script],
            text=True, timeout=30,
        ).strip()

    def count(backend, name):
        if backend == "clickhouse":
            return int(ch(f"SELECT count() FROM {database}.{name} FINAL"))
        return int(mongo(f'print(db.getSiblingDB("{database}").{name}.countDocuments())'))

    def command(backend, name):
        return [str(binary), "--timeout", "35", "day", str(source), "-o", backend,
                "-t", database + "." + name, "--workers", "4", "--report", "run.json"]

    result["versions"] = {"clickhouse": ch("SELECT version()"), "mongodb": mongo("print(db.version())")}
    save()
    try:
        for backend in ["clickhouse", "mongodb"]:
            for fault in ["network_outage", "sigkill"]:
                name = backend + "_" + fault
                work = scratch / name
                work.mkdir()
                # TCP proxy for both backends; Mongo kill uses the importer
                # exit-status boundary so its real server/tool behaviour is retained.
                mode = "drop" if fault == "network_outage" else ("hold" if backend == "clickhouse" else "pass")
                proxy = FaultProxy(*endpoints[backend], mode)
                env = os.environ.copy()
                if backend == "clickhouse":
                    env["CLICKHOUSE_URL"] = f"http://127.0.0.1:{proxy.port}"
                    proxy.armed.set()
                else:
                    env["MONGODB_URI"] = (
                        f"mongodb://127.0.0.1:{proxy.port}/?directConnection=true"
                        "&serverSelectionTimeoutMS=1500&connectTimeoutMS=1000"
                    )
                    tools = work / "tools"
                    tools.mkdir()
                    marker = work / "importer.marker"
                    wrapper = tools / "mongoimport"
                    wrapper.write_text(
                        "#!" + sys.executable + "\nimport os,pathlib,signal,subprocess,sys\n"
                        f"marker=pathlib.Path({str(marker)!r})\n"
                        "marker.write_text('started')\n"
                        f"status=subprocess.run([{importer!r},*sys.argv[1:]]).returncode\n"
                        + ("if status==0:\n marker.write_text(str(os.getpid()))\n os.kill(os.getpid(),signal.SIGSTOP)\n"
                           if fault == "sigkill" else "")
                        + "sys.exit(status)\n"
                    )
                    wrapper.chmod(0o755)
                    env["PATH"] = str(tools) + os.pathsep + env["PATH"]
                child = None
                wrapper_pid = None
                started = time.monotonic()
                try:
                    with (work / "fault.log").open("w") as log:
                        child = subprocess.Popen(command(backend, name), cwd=work, env=env,
                                                 stdout=log, stderr=log, start_new_session=True)
                        if backend == "mongodb":
                            wait_for(marker.exists, child)
                            if fault == "network_outage":
                                proxy.armed.set()
                            else:
                                wait_for(lambda: marker.read_text().isdigit(), child)
                                wrapper_pid = int(marker.read_text())
                        if mode != "pass":
                            wait_for(proxy.triggered.is_set, child)
                        if fault == "sigkill":
                            child.kill()
                        exit_code = child.wait(timeout=40)
                    assert exit_code != 0
                    report = json.loads((work / "run.json").read_text())
                    assert report["database_status"] == "unknown", report["database_status"]
                    assert report["status"] == ("running" if fault == "sigkill" else "failed")
                    assert report["artifact"]["state"] == "published"
                    assert (work / backend).exists()
                    written = count(backend, name)
                    assert 0 < written <= rows, (backend, fault, written)
                    entry = {
                        "backend": backend, "fault": fault, "exit": exit_code,
                        "seconds": time.monotonic() - started, "rows_after_fault": written,
                        "manifest_status_after_fault": report["status"],
                        "database_status_after_fault": report["database_status"],
                        "artifact_after_fault": report["artifact"],
                        "observations_after_fault": report["observations"],
                    }
                    result["cases"].append(entry)
                    save()
                finally:
                    proxy.close()
                    if child and child.poll() is None:
                        child.kill()
                        child.wait()
                    if wrapper_pid:
                        try:
                            os.kill(wrapper_pid, signal.SIGKILL)
                        except ProcessLookupError:
                            pass
                # Same output/table/run.json checks OS locks release after kill.
                with (work / "recovery.log").open("w") as log:
                    recovery = subprocess.run(command(backend, name) + ["--keep-csv"], cwd=work,
                                              stdout=log, stderr=log, timeout=60)
                assert recovery.returncode == 0, str(work / "recovery.log")
                report = json.loads((work / "run.json").read_text())
                assert report["database_status"] == "verified"
                assert report["database_verification"]["rows"] == rows
                assert report["artifact"]["sha256"] == entry["artifact_after_fault"]["sha256"]
                assert count(backend, name) == rows
                assert report["observations"]["temporary_current_bytes"] == 0
                entry["recovery_verified_rows"] = rows
                entry["recovery_verification"] = report["database_verification"]
                entry["recovery_observations"] = report["observations"]
                entry["run_archives"] = len(list(work.glob("run.json.*.json")))
                assert entry["run_archives"] == 2
                save()
                print(backend, fault, written, "rows after fault; recovered", rows, flush=True)
        result["completed"] = True
    finally:
        failed = sys.exc_info()[0] is not None
        cleanup_errors = []
        for backend, cleanup in [
            ("clickhouse", lambda: ch(f"DROP DATABASE IF EXISTS {database}")),
            ("mongodb", lambda: mongo(f'db.getSiblingDB("{database}").dropDatabase()')),
        ]:
            try:
                cleanup()
            except Exception as error:
                cleanup_errors.append(f"{backend}: {error}")
        result["cleanup_errors"] = cleanup_errors
        result["private_database_removed"] = not cleanup_errors
        save()
        if cleanup_errors and not failed:
            raise RuntimeError("; ".join(cleanup_errors))


if __name__ == "__main__":
    main()
