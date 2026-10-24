#!/usr/bin/env python3
"""Sample simultaneous process-group/tree RSS, FD, threads and declared disk roots."""
import argparse
import json
import os
from pathlib import Path
import platform
import signal
import subprocess
import time


def processes():
    result = {}
    text = subprocess.check_output(
        ["ps", "-axo", "pid=,ppid=,pgid=,rss=,lstart="], text=True, timeout=5)
    for line in text.splitlines():
        fields = line.split()
        pid, parent, group, rss = map(int, fields[:4])
        result[pid] = (parent, group, rss * 1024, " ".join(fields[4:]))
    return result


def members(table, leader, known):
    selected = {pid for pid, (_, group, _, started) in table.items()
                if group == leader or known.get(pid) == started}
    selected.add(leader)
    while True:
        extra = {pid for pid, (parent, _, _, _) in table.items() if parent in selected}
        if extra <= selected:
            return selected & table.keys()
        selected |= extra


def descriptor_and_threads(pids):
    counts, threads = {}, {}
    if platform.system() == "Linux":
        for pid in pids:
            try:
                counts[pid] = len(list(Path(f"/proc/{pid}/fd").iterdir()))
                lines = Path(f"/proc/{pid}/status").read_text().splitlines()
                threads[pid] = int(next(line.split()[1] for line in lines if line.startswith("Threads:")))
            except (OSError, StopIteration):
                pass
    elif platform.system() == "Darwin" and pids:
        ids = ",".join(map(str, sorted(pids)))
        try:
            completed = subprocess.run(
                ["/usr/sbin/lsof", "-nP", "-a", "-p", ids, "-Fpf"],
                capture_output=True, text=True, timeout=5)
            current = None
            for line in completed.stdout.splitlines():
                if line.startswith("p") and line[1:].isdigit():
                    current = int(line[1:])
                    counts[current] = 0
                elif line.startswith("f") and line[1:].isdigit() and current is not None:
                    counts[current] += 1
            text = subprocess.check_output(["ps", "-M", "-p", ids], text=True, timeout=5)
            for line in text.splitlines()[1:]:
                fields = line.split()
                token = fields[0] if fields and fields[0].isdigit() else fields[1] if len(fields) > 1 else ""
                if token.isdigit() and int(token) in pids:
                    pid = int(token)
                    threads[pid] = threads.get(pid, 0) + 1
        except (OSError, subprocess.SubprocessError):
            pass
    return (sum(counts.values()) if pids <= counts.keys() else None,
            sum(threads.values()) if pids <= threads.keys() else None)


def disk_bytes(root):
    if not root.exists():
        return 0
    total = 0
    for directory, dirs, files in os.walk(root, followlinks=False):
        dirs[:] = [name for name in dirs if not Path(directory, name).is_symlink()]
        for name in files:
            path = Path(directory, name)
            try:
                if not path.is_symlink() and path.is_file():
                    total += path.stat().st_size
            except FileNotFoundError:
                pass
    return total


def measure(args):
    result = {"schema_version": 1, "status": "running", "platform": platform.platform(),
              "command": args.command, "interval_seconds": args.interval,
              "scope": "Simultaneous sampled group/tree sums; short-lived children and peaks between samples can be missed. Disk values are logical file bytes at declared roots, not allocated blocks or external database storage.",
              "samples": [], "peaks": {}, "sampling_errors": []}
    known = {}
    started = time.monotonic()
    log = args.output.with_suffix(".command.log")
    with log.open("xb") as stream:
        child = subprocess.Popen(args.command, stdout=stream, stderr=subprocess.STDOUT,
                                 start_new_session=True)
        result["leader_pid"] = child.pid
        try:
            while True:
                tick = time.monotonic()
                try:
                    table = processes()
                    pids = members(table, child.pid, known)
                    known = {pid: table[pid][3] for pid in pids}
                    fd, threads = descriptor_and_threads(pids)
                    sample = {"elapsed_seconds": time.monotonic() - started,
                              "processes": len(pids), "rss_bytes": sum(table[p][2] for p in pids),
                              "fds": fd, "threads": threads,
                              "disk_bytes": {str(p): disk_bytes(p) for p in args.disk_root}}
                    result["samples"].append(sample)
                    for name in ("processes", "rss_bytes", "fds", "threads"):
                        if sample[name] is not None:
                            result["peaks"][name] = max(result["peaks"].get(name, 0), sample[name])
                    peaks = result["peaks"].setdefault("disk_bytes", {})
                    for path, size in sample["disk_bytes"].items():
                        peaks[path] = max(peaks.get(path, 0), size)
                except (OSError, subprocess.SubprocessError) as error:
                    result["sampling_errors"].append(str(error))
                args.output.write_text(json.dumps(result, indent=2) + "\n")
                if child.poll() is not None:
                    break
                if time.monotonic() - started >= args.timeout:
                    result["timed_out"] = True
                    break
                time.sleep(max(0, args.interval - (time.monotonic() - tick)))
        finally:
            # Runner collectors can create their own process groups. Recheck
            # membership/start identity before terminating these owned children.
            try:
                table = processes()
                pids = members(table, child.pid, known)
                groups = {table[pid][1] for pid in pids}
                owned_groups = {group for group in groups if group == child.pid or group in pids}
                for group in owned_groups:
                    try:
                        os.killpg(group, signal.SIGKILL)
                    except ProcessLookupError:
                        pass
                for pid in pids:
                    if table[pid][1] not in owned_groups:
                        try:
                            os.kill(pid, signal.SIGKILL)
                        except ProcessLookupError:
                            pass
            except (OSError, subprocess.SubprocessError) as error:
                result["sampling_errors"].append(f"cleanup: {error}")
            try:
                os.killpg(child.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            child.wait(timeout=10)
    result.update(status="passed" if child.returncode == 0 and not result.get("timed_out") else "failed",
                  exit_code=child.returncode, elapsed_seconds=time.monotonic() - started,
                  missing_fd_samples=sum(s["fds"] is None for s in result["samples"]),
                  missing_thread_samples=sum(s["threads"] is None for s in result["samples"]))
    args.output.write_text(json.dumps(result, indent=2) + "\n")
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--disk-root", type=Path, action="append", default=[])
    parser.add_argument("--interval", type=float, default=1.0)
    parser.add_argument("--timeout", type=float, default=3600)
    parser.add_argument("command", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    if args.command[:1] == ["--"]:
        args.command.pop(0)
    if not args.command or not 0.1 <= args.interval <= 60 or not 0 < args.timeout <= 86400:
        parser.error("command and bounded interval/timeout required")
    if args.output.exists() or args.output.with_suffix(".command.log").exists():
        parser.error("use a new output and command log")
    result = measure(args)
    print(json.dumps({"status": result["status"], "peaks": result["peaks"]}))
    if result["status"] != "passed":
        raise SystemExit(1)


if __name__ == "__main__":
    main()
