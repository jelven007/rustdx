#!/usr/bin/env python3
"""Linux-only native database recovery drill with owned containers and namespaces.

Uses two real runner releases and quiesced full backups, never production endpoints.
Keeps inputs, native backups, commands, snapshots and cleanup evidence in --output's
directory. Requires local Docker, mongosh, mongodump and mongorestore. Host networking
is restricted by database configuration to random loopback ports.
"""
import argparse
import csv
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import socket
import struct
import subprocess
import sys
import time
import urllib.error
import urllib.request
import uuid
import xml.etree.ElementTree as ET


CH_IMAGE = ("clickhouse/clickhouse-server:25.8.33.6-alpine@sha256:"
            "87e0a5b72f5465b18eacca7c76850e7ff551c9795c50e451f5646299e5e24146")
MONGO_IMAGE = ("mongo:8.0.16@sha256:"
               "4b58ebcb1dc7a7b4e84cd8ce9098d48764ae4478876898ff9551acf2ac4a6a6d")
CONTRACT = "a-share/day/raw/cny/shares/v1"
KEYS = ["dataset_id", "release_id", "contract_version", "date", "market",
        "code", "instrument_class"]
NUMBERS = ["open", "high", "low", "close", "amount_cny", "volume_shares"]


def digest(path):
    h = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            h.update(block)
    return h.hexdigest()


def inventory(root):
    return [{"path": str(p.relative_to(root)), "bytes": p.stat().st_size,
             "sha256": digest(p)}
            for p in sorted(root.rglob("*")) if p.is_file()]


def verify_inventory(root, expected):
    if inventory(root) != expected:
        raise ValueError("backup inventory mismatch; restore must not start")


def free_ports(count):
    sockets = []
    try:
        for _ in range(count):
            listener = socket.socket()
            listener.bind(("127.0.0.1", 0))
            sockets.append(listener)
        return [s.getsockname()[1] for s in sockets]
    finally:
        for listener in sockets:
            listener.close()


class Drill:
    def __init__(self, args):
        self.args = args
        self.repo = Path(__file__).resolve().parents[2]
        self.root = args.output.resolve().parent
        self.root.mkdir(parents=True, exist_ok=True, mode=0o700)
        self.work = self.root / ("drill-" + uuid.uuid4().hex)
        self.work.mkdir(mode=0o700)
        self.logs = self.work / "logs"
        self.logs.mkdir()
        self.database = "rustdx_recovery_" + uuid.uuid4().hex
        self.ports = free_ports(4)
        self.url = f"http://127.0.0.1:{self.ports[0]}"
        self.env = dict(os.environ, RUSTDX_RECOVERY_CH_USER="default",
                        RUSTDX_RECOVERY_CH_PASSWORD="",
                        RUSTDX_RECOVERY_MONGO_URI=f"mongodb://127.0.0.1:{self.ports[1]}",
                        RUSTDX_NETWORK_DIR=str(self.work / "permits"))
        self.containers = []
        self.result = {
            "schema_version": 1, "status": "running", "platform": platform.platform(),
            "work": str(self.work), "database": self.database, "ports": self.ports,
            "scope": "quiesced_full_native_backups; fresh_data_directories; two_pinned_releases",
            "source_is_synthetic": True, "securities": 600, "rows_per_release": 1200,
            "production_rpo_rto_approved": False, "commands": [], "checks": {},
            "binaries": {str(p): digest(p) for p in [args.cli, args.runner]},
        }

    def save(self):
        self.args.output.write_text(json.dumps(self.result, indent=2) + "\n")

    def run(self, label, command, *, stdin=None, expected=0, timeout=240):
        start = time.monotonic()
        completed = subprocess.run([str(x) for x in command], input=stdin,
                                   capture_output=True, env=self.env, timeout=timeout)
        n = len(self.result["commands"])
        stdout = self.logs / f"{n:03d}-{label}.stdout"
        stderr = self.logs / f"{n:03d}-{label}.stderr"
        stdout.write_bytes(completed.stdout)
        stderr.write_bytes(completed.stderr)
        self.result["commands"].append({
            "label": label, "argv": [str(x) for x in command],
            "exit_code": completed.returncode, "expected": expected,
            "seconds": round(time.monotonic() - start, 6),
            "stdout": str(stdout.relative_to(self.root)),
            "stderr": str(stderr.relative_to(self.root)),
        })
        self.save()
        if expected == "nonzero":
            if completed.returncode == 0:
                raise AssertionError(f"{label} unexpectedly succeeded")
        elif completed.returncode != expected:
            raise AssertionError(f"{label} exited {completed.returncode}; see {stderr}")
        return completed.stdout

    def docker(self, label, *arguments, **kwargs):
        return self.run(label, [self.args.docker, *arguments], **kwargs)

    def ch(self, label, sql, *, reject=False):
        start = time.monotonic()
        request = urllib.request.Request(self.url, data=sql.encode())
        try:
            with urllib.request.urlopen(request, timeout=90) as reply:
                status, body = reply.status, reply.read()
        except urllib.error.HTTPError as error:
            status, body = error.code, error.read()
        n = len(self.result["commands"])
        path = self.logs / f"{n:03d}-{label}.sql"
        path.write_text(sql + "\n")
        response = path.with_suffix(".response")
        response.write_bytes(body)
        self.result["commands"].append({
            "label": label, "request": str(path.relative_to(self.root)),
            "response": str(response.relative_to(self.root)),
            "http_status": status, "expected": "non-200" if reject else 200,
            "seconds": round(time.monotonic() - start, 6),
        })
        self.save()
        if (status == 200) == reject:
            raise AssertionError(f"{label}: unexpected HTTP {status}; see {response}")
        return body.decode()

    def mongo(self, label, source):
        path = self.logs / f"{len(self.result['commands']):03d}-{label}.js"
        path.write_text(
            'db = connect(process.env.RUSTDX_RECOVERY_MONGO_URI)'
            f'.getSiblingDB("{self.database}");\n' + source + "\n")
        return self.run(label, [self.args.mongosh, "--nodb", "--quiet", "--norc",
                               "--file", path])

    def start(self, generation):
        data = self.work / generation
        data.mkdir(mode=0o700)
        for name in ["ch-data", "ch-logs", "mongo-data", "mongo-config"]:
            (data / name).mkdir(mode=0o777)
            (data / name).chmod(0o777)
        if inventory(data):
            raise AssertionError("new server data directory is not empty")
        self.result.setdefault("fresh_directories", {})[generation] = str(data)
        ch_name = f"{self.database}-ch-{generation}"
        mo_name = f"{self.database}-mongo-{generation}"
        ch_args = [
            "run", "-d", "--name", ch_name, "--network", "host", "--cpus", "2",
            "--memory", "2g", "--ulimit", "nofile=262144:262144",
            "-e", "CLICKHOUSE_SKIP_USER_SETUP=1",
            "-v", f"{data / 'ch-data'}:/var/lib/clickhouse",
            "-v", f"{data / 'ch-logs'}:/var/log/clickhouse-server",
            "-v", f"{self.work / 'clickhouse.xml'}:/etc/clickhouse-server/config.d/recovery.xml:ro",
            "-v", f"{self.work / 'native-backups'}:/backups",
            self.args.clickhouse_image,
        ]
        # Register before launching so even a partially failed start is audited.
        self.containers.extend([ch_name, mo_name])
        self.docker(f"{generation}-ch-start", *ch_args)
        self.docker(
            f"{generation}-mongo-start", "run", "-d", "--name", mo_name,
            "--network", "host", "--cpus", "2", "--memory", "1g",
            "-v", f"{data / 'mongo-data'}:/data/db",
            "-v", f"{data / 'mongo-config'}:/data/configdb",
            self.args.mongodb_image, "mongod", "--bind_ip", "127.0.0.1",
            "--port", str(self.ports[1]), "--wiredTigerCacheSizeGB", "0.25")
        deadline = time.monotonic() + 60
        while True:
            try:
                with urllib.request.urlopen(self.url + "/ping", timeout=2) as response:
                    if response.read() != b"Ok.\n":
                        raise OSError("unexpected ClickHouse ping")
                with socket.create_connection(("127.0.0.1", self.ports[1]), timeout=2):
                    pass
                break
            except (OSError, urllib.error.URLError):
                if time.monotonic() >= deadline:
                    raise TimeoutError("owned servers failed to start")
                time.sleep(0.25)
        self.mongo(f"{generation}-mongo-ping",
                   'if(db.adminCommand({ping:1}).ok!==1) throw Error("ping failed");')
        return [ch_name, mo_name]

    def stop(self, names):
        for name in names:
            self.docker("container-stop", "stop", "--time", "30", name)
            self.docker("container-inspect", "inspect", name)
            self.docker("container-logs", "logs", name)
            self.docker("container-remove", "rm", "-v", name)
            self.containers.remove(name)

    def fixture(self):
        facts = self.work / "facts"
        facts.mkdir()
        announcement = b"SYNTHETIC RECOVERY DRILL; NOT MARKET FACT"
        fact_sha = hashlib.sha256(announcement).hexdigest()
        (facts / fact_sha).write_bytes(announcement)
        self.write_json("calendar.json", {
            "version": "synthetic-recovery", "source": "recovery-drill",
            "start": "2026-07-06", "end": "2026-07-07", "holidays": []})
        codes = [f"sh{600000 + i}" for i in range(600)]
        self.write_json("history.json", {
            "version": "synthetic-recovery", "checked_through": "2026-07-07",
            "intervals": [{"code": code, "start": "2026-07-06", "end": "2026-07-07",
                           "board": "Main", "st": False, "stage": "Seasoned",
                           "state": "Trading", "announcement": {
                               "url": "https://example.org/synthetic-recovery",
                               "sha256": fact_sha, "published": "2026-07-06",
                               "checked": "2026-07-07"}} for code in codes]})
        self.write_json("universe.json", {
            "version": "synthetic-recovery", "checked_through": "2026-07-07",
            "source_sha256": fact_sha, "members": [
                {"code": code, "from": "2026-07-06", "through": "2026-07-07"}
                for code in codes]})
        for revision in [1, 2]:
            source = self.work / f"source-{revision}"
            source.mkdir()
            for i, code in enumerate(codes):
                content = b"".join(
                    struct.pack("<5If2I", date, 1000, 1200, 900,
                                1000 + 50 * (revision - 1), 12345.5 + i,
                                101 + i, 0)
                    for date in [20260706, 20260707])
                (source / (code + ".day")).write_bytes(content)
            self.write_json(f"job-{revision}.json", {
                "schema_version": 1, "root": str(self.work / "live"),
                "cli": str(self.args.cli), "no_tcp_startup": True,
                "dataset_id": "synthetic-recovery", "contract_version": CONTRACT,
                "policy_revision": 1, "source_revision": revision,
                "source_as_of": "2026-07-07", "planned_date": "2026-07-07",
                "from": "2026-07-06", "through": "2026-07-07",
                "day_directories": [str(source)], "calendar": str(self.work / "calendar.json"),
                "calendar_source_sha256": fact_sha,
                "security_history": str(self.work / "history.json"),
                "universe": str(self.work / "universe.json"), "facts_directory": str(facts),
                "timeout_seconds": 180, "publish_reserve_seconds": 30,
                "cancel_grace_seconds": 1, "workers": 4,
                "max_source_bytes": 1024 * 1024, "max_workspace_bytes": 256 * 1024 * 1024,
                "max_rows": 10000, "min_free_bytes": 0,
                "database_targets": [
                    {"kind": "clickhouse", "id": "ch", "url": self.url,
                     "table": self.database + ".bars", "user_env": "RUSTDX_RECOVERY_CH_USER",
                     "password_env": "RUSTDX_RECOVERY_CH_PASSWORD"},
                    {"kind": "mongodb", "id": "mongo", "database": self.database,
                     "collection": "bars", "uri_env": "RUSTDX_RECOVERY_MONGO_URI",
                     "mongosh": str(self.args.mongosh)}]})

    def write_json(self, name, value):
        (self.work / name).write_text(json.dumps(value, indent=2) + "\n")

    def runner(self, label, action, *arguments):
        return self.run(label, [self.args.runner, "--no-tcp-startup", action, *arguments])

    def exports(self, phase, root):
        outputs = {}
        for version in [1, 2]:
            for target in ["file", "ch", "mongo"]:
                output = self.work / f"{phase}-v{version}-{target}.csv"
                arguments = [
                    "--root", root, "--dataset", "synthetic-recovery",
                    "--version", str(version), "--from", "2026-07-06",
                    "--through", "2026-07-07", "--output", output]
                action = "export"
                if target != "file":
                    action = "export-database"
                    arguments.extend(["--target", target])
                reply = json.loads(self.runner(f"{phase}-v{version}-{target}", action, *arguments))
                if reply != {"catalog_version": version, "rows": 1200}:
                    raise AssertionError("unexpected export version or count")
                with output.open(newline="") as handle:
                    rows = list(csv.DictReader(handle))
                if len(rows) != 1200 or len({(r["date"], r["market"], r["code"]) for r in rows}) != 1200:
                    raise AssertionError("independent export key check failed")
                for row in rows:
                    i = int(row["code"]) - 600000
                    expected = [10, 12, 9, 10 + 0.5 * (version - 1), 12345.5 + i, 101 + i]
                    if [float(row[field]) for field in NUMBERS] != expected:
                        raise AssertionError("independent synthetic full-field mismatch")
                outputs[f"v{version}-{target}"] = digest(output)
        for version in [1, 2]:
            if len({outputs[f"v{version}-{target}"] for target in ["file", "ch", "mongo"]}) != 1:
                raise AssertionError("file and database exports differ")
        if outputs["v1-file"] == outputs["v2-file"]:
            raise AssertionError("revision was not reflected")
        self.result["checks"][phase + "_exports"] = outputs
        return outputs

    def snapshot(self, phase):
        bits = [f"hex(reinterpretAsFixedString({field})) AS {field}_bits" for field in NUMBERS]
        rows = self.ch(phase + "-ch-rows", f"SELECT {','.join(KEYS + bits)} FROM "
                       f"{self.database}.bars ORDER BY {','.join(KEYS)} FORMAT JSONEachRow")
        ch_rows = [json.loads(line) for line in rows.splitlines()]
        if len(ch_rows) != 2400:
            raise AssertionError("wrong ClickHouse physical row count")
        schema = self.ch(phase + "-ch-schema", f"SHOW CREATE TABLE {self.database}.bars FORMAT TSVRaw")
        for marker in ["CONSTRAINT contract", "CONSTRAINT identity", "CONSTRAINT share_count",
                       "ENGINE = MergeTree", "ORDER BY"]:
            if marker not in schema:
                raise AssertionError("ClickHouse schema missing " + marker)
        mongo = json.loads(self.mongo(phase + "-mongo-snapshot", """
const rows=db.bars.find().sort({dataset_id:1,release_id:1,contract_version:1,date:1,
  market:1,code:1,instrument_class:1}).toArray();
print(EJSON.stringify({rows,options:db.getCollectionInfos({name:"bars"})[0].options,
  indexes:db.bars.getIndexes()}, {relaxed:false}));
"""))
        if len(mongo["rows"]) != 2400:
            raise AssertionError("wrong MongoDB physical row count")
        release_ids = {r["release_id"] for r in ch_rows}
        if len(release_ids) != 2 or any(sum(r["release_id"] == release for r in ch_rows) != 1200
                                      for release in release_ids):
            raise AssertionError("two complete releases required")
        result = {"clickhouse_rows": ch_rows, "clickhouse_schema": schema, "mongodb": mongo}
        self.write_json(phase + "-snapshot.json", result)
        return result

    def negative_constraints(self):
        table = self.database + ".bars"
        columns = ",".join(KEYS + NUMBERS)
        for name, expression, code in [
            ("contract", "'invalid' AS contract_version", "VIOLATED"),
            ("identity", "'index' AS instrument_class", "VIOLATED"),
            ("share-count", "1.5 AS volume_shares", "VIOLATED"),
        ]:
            body = self.ch("restored-ch-reject-" + name,
                           f"INSERT INTO {table} ({columns}) SELECT * REPLACE({expression}) "
                           f"FROM {table} LIMIT 1", reject=True)
            if code not in body:
                raise AssertionError("rejection did not come from constraint")
        self.mongo("restored-mongo-reject-validator-and-duplicate", """
const row=db.bars.findOne(); const before=db.bars.countDocuments({});
const invalid={...row,contract_version:"invalid"}; delete invalid._id;
try { db.bars.insertOne(invalid); throw Error("validator accepted invalid contract"); }
catch(e) { if(e.code!==121) throw e; }
const duplicate={...row}; delete duplicate._id;
try { db.bars.insertOne(duplicate); throw Error("unique index accepted duplicate"); }
catch(e) { if(e.code!==11000) throw e; }
if(db.bars.countDocuments({})!==before) throw Error("rejection changed row count");
print(JSON.stringify({validator_error:121,unique_error:11000,rows:before}));
""")
        self.result["checks"]["restored_constraints_reject_invalid_writes"] = True

    def execute(self):
        backups = self.work / "native-backups"
        backups.mkdir(mode=0o777)
        backups.chmod(0o777)
        config = ET.parse(self.repo / "deploy/acceptance/clickhouse.xml")
        node = config.getroot()
        for name, value in [("listen_host", "127.0.0.1"), ("http_port", self.ports[0]),
                            ("tcp_port", self.ports[2]), ("interserver_http_port", self.ports[3])]:
            child = ET.SubElement(node, name, {"replace": "replace"})
            child.text = str(value)
        disks = ET.SubElement(ET.SubElement(node, "storage_configuration"), "disks")
        disk = ET.SubElement(disks, "backups")
        ET.SubElement(disk, "type").text = "local"
        ET.SubElement(disk, "path").text = "/backups/"
        policy = ET.SubElement(node, "backups")
        ET.SubElement(policy, "allowed_disk").text = "backups"
        ET.SubElement(policy, "allowed_path").text = "/backups/"
        config.write(self.work / "clickhouse.xml")
        self.fixture()
        for label, tool in [("dump", self.args.mongodump), ("restore", self.args.mongorestore),
                            ("mongosh", self.args.mongosh), ("docker", self.args.docker)]:
            self.run(label + "-version", [tool, "--version"])
        old = self.start("original")
        for image in [self.args.clickhouse_image, self.args.mongodb_image]:
            self.docker("image-inspect", "image", "inspect", image)
        self.ch("ch-version", "SELECT version()")
        self.mongo("mongo-version", "print(db.version());")
        sql = (self.repo / "deploy/runner/clickhouse.sql").read_text().replace(
            "rustdx_versioned", self.database)
        for i, statement in enumerate(sql.split(";")[:2]):
            self.ch(f"provision-ch-{i}", statement)
        js = (self.repo / "deploy/runner/mongodb.js").read_text().replace(
            "rustdx_versioned", self.database)
        self.mongo("provision-mongo", js)
        for revision in [1, 2]:
            reply = self.runner(f"release-{revision}", "run",
                                "--config", self.work / f"job-{revision}.json")
            self.result.setdefault("release_replies", []).append(json.loads(reply))
        before = self.exports("before", self.work / "live")
        snapshot = self.snapshot("before")
        # All runner processes have exited; no other identity has these namespaces.
        self.result["backup_boundary"] = {
            "utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
            "catalog_version": 2, "quiesced": True, "rows_each_database": 2400,
            "release_ids": sorted({r["release_id"] for r in snapshot["clickhouse_rows"]})}
        self.runner("control-backup", "backup", "--root", self.work / "live",
                    "--destination", self.work / "control-backup")
        start = time.monotonic()
        response = self.ch("ch-native-backup",
                           f"BACKUP DATABASE {self.database} TO Disk('backups','full.zip')")
        if "BACKUP_CREATED" not in response:
            raise AssertionError("native ClickHouse backup did not complete")
        self.result["checks"]["clickhouse_backup_seconds"] = time.monotonic() - start
        connection = self.work / "mongo-tools.json"
        connection.write_text(json.dumps({"uri": self.env["RUSTDX_RECOVERY_MONGO_URI"]}))
        connection.chmod(0o600)
        try:
            self.run("mongo-native-dump", [self.args.mongodump, "--config", connection,
                                          "--db", self.database, "--out", backups / "mongo",
                                          "--numParallelCollections", "1"])
            expected = inventory(backups)
            if not any(p["path"].endswith(".bson") for p in expected):
                raise AssertionError("native BSON dump missing")
            self.write_json("native-backup-inventory.json", expected)
            verify_inventory(backups, expected)
            corrupt = self.work / "corrupt-backups"
            shutil.copytree(backups, corrupt)
            with (corrupt / "full.zip").open("r+b") as handle:
                handle.write(b"CORRUPTED")
            try:
                verify_inventory(corrupt, expected)
            except ValueError:
                self.result["checks"]["corrupt_inventory_rejected_before_restore"] = True
            else:
                raise AssertionError("corrupted backup accepted")
            self.stop(old)
            # A stopped old data directory is never mounted into the fresh servers.
            old_files = inventory(self.work / "original")
            self.write_json("old-server-stopped-inventory.json", old_files)
            recovery_start = time.monotonic()
            self.start("restored")
            if self.ch("fresh-ch-empty", f"EXISTS DATABASE {self.database}").strip() != "0":
                raise AssertionError("restore database already exists")
            self.mongo("fresh-mongo-empty", """
if(db.getCollectionInfos().length) throw Error("restore target is nonempty");
print(JSON.stringify({collections:0}));
""")
            verify_inventory(backups, expected)
            response = self.ch("ch-native-restore",
                               f"RESTORE DATABASE {self.database} FROM Disk('backups','full.zip')")
            if "RESTORED" not in response:
                raise AssertionError("native ClickHouse restore did not complete")
            self.run("mongo-native-restore", [
                self.args.mongorestore, "--config", connection, "--stopOnError",
                "--nsInclude", self.database + ".*", "--numParallelCollections", "1",
                backups / "mongo"])
            self.runner("control-restore", "restore", "--root", self.work / "control-backup",
                        "--destination", self.work / "restored-control")
            after = self.exports("after", self.work / "restored-control")
            if before != after or snapshot != self.snapshot("after"):
                raise AssertionError("restored full fields, metadata or fixed versions differ")
            self.result["checks"]["recovery_to_verified_seconds"] = time.monotonic() - recovery_start
            self.negative_constraints()
            rejection = self.ch(
                "ch-reject-nonempty-restore",
                f"RESTORE DATABASE {self.database} FROM Disk('backups','full.zip')", reject=True)
            if "non-empty" not in rejection.lower():
                raise AssertionError("wrong existing target rejection")
            if snapshot != self.snapshot("after-negative-checks"):
                raise AssertionError("negative checks changed data or metadata")
            verify_inventory(backups, expected)
            verify_inventory(self.work / "original", old_files)
            self.result["checks"].update({
                "fresh_native_restore": True, "two_pinned_versions_exact": True,
                "full_database_fields_and_metadata_identical": True,
                "old_data_directories_unchanged_after_restore": True,
                "nonempty_clickhouse_restore_rejected": True,
                "mongorestore_stop_on_error": True,
            })
        finally:
            connection.unlink(missing_ok=True)
        self.result["status"] = "passed"

    def cleanup(self):
        errors = []
        for name in self.containers[:]:
            try:
                self.stop([name])
            except Exception as error:
                errors.append(str(error))
        owned = self.docker("cleanup-owned-containers", "ps", "-a", "--filter",
                            "name=" + self.database, "--format", "{{.Names}}").decode().strip()
        self.result["cleanup"] = {"remaining_owned_containers": owned.splitlines(),
                                  "errors": errors, "native_backups_retained": True}
        if owned or errors:
            self.result["status"] = "failed"
        self.save()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cli", type=Path, required=True)
    parser.add_argument("--runner", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--docker", type=Path, default=shutil.which("docker"))
    parser.add_argument("--mongosh", type=Path, default=shutil.which("mongosh"))
    parser.add_argument("--mongodump", type=Path, default=shutil.which("mongodump"))
    parser.add_argument("--mongorestore", type=Path, default=shutil.which("mongorestore"))
    parser.add_argument("--clickhouse-image", default=CH_IMAGE)
    parser.add_argument("--mongodb-image", default=MONGO_IMAGE)
    args = parser.parse_args()
    if sys.platform != "linux":
        parser.error("requires Linux Docker host networking")
    if args.output.exists():
        parser.error("output already exists; use a new evidence directory")
    for name in ["cli", "runner", "docker", "mongosh", "mongodump", "mongorestore"]:
        value = getattr(args, name)
        if value is None or not value.is_file() or not os.access(value, os.X_OK):
            parser.error(f"missing executable --{name}")
        setattr(args, name, value.resolve())
    for image, required in [(args.clickhouse_image, CH_IMAGE), (args.mongodb_image, MONGO_IMAGE)]:
        if not image.endswith("@" + required.split("@")[1]):
            parser.error("image mirror may change, pinned digest must not change")
    drill = Drill(args)
    try:
        drill.execute()
    except Exception as error:
        drill.result.update(status="failed", error=str(error))
        raise
    finally:
        drill.cleanup()
    if drill.result["status"] != "passed":
        raise SystemExit(1)
    print(json.dumps({"status": "passed", "output": str(args.output)}))


if __name__ == "__main__":
    main()
