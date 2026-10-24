import importlib.util
from pathlib import Path
import hashlib
import json
import os
import shutil
import sqlite3
import tempfile
import unittest
from datetime import datetime, timedelta, timezone

MODULE = Path(__file__).resolve().parents[2] / "ops/retention_preview.py"
spec = importlib.util.spec_from_file_location("retention_preview", MODULE)
preview = importlib.util.module_from_spec(spec)
spec.loader.exec_module(preview)


def evidence(root, relative):
    data = (root / relative).read_bytes()
    return {"path": relative, "bytes": len(data), "sha256": hashlib.sha256(data).hexdigest()}


def snapshot(root):
    return {p.relative_to(root).as_posix(): hashlib.sha256(p.read_bytes()).hexdigest()
            for p in root.rglob("*") if p.is_file()}


class RetentionTests(unittest.TestCase):
    def fixture(self, root):
        (root / "controller.lock").touch()
        db = sqlite3.connect(root / "ledger.sqlite")
        db.executescript("""
            PRAGMA user_version=3;
            CREATE TABLE jobs(id TEXT);
            CREATE TABLE events(id TEXT);
            CREATE TABLE attempts(id TEXT,directory TEXT,state TEXT,pid INTEGER);
            CREATE TABLE releases(id TEXT,manifest TEXT,state TEXT);
            CREATE TABLE publications(dataset TEXT,version INTEGER,pointer TEXT);
            CREATE TABLE targets(release TEXT,id TEXT);
            CREATE TABLE target_io(release TEXT,target TEXT,request TEXT,response TEXT);
        """)
        for number in (1, 2):
            release = f"release{number}"
            directory = root / "releases" / release
            directory.mkdir(parents=True)
            (directory / "data.csv").write_text("date,close\n2026-07-07,10\n")
            (directory / "release.json").write_text(json.dumps({
                "release_id": release, "files": [evidence(directory, "data.csv")],
            }))
            db.execute("INSERT INTO releases VALUES(?,?,?)",
                       (release, json.dumps(evidence(root, f"releases/{release}/release.json")), "released"))
            catalog = f"catalogs/data/{number}.json"
            (root / catalog).parent.mkdir(parents=True, exist_ok=True)
            (root / catalog).write_text(json.dumps({
                "dataset_id": "data", "version": number,
                "partitions": {"2026-07-07": {"release_id": release}},
                "database_releases": {},
            }))
            db.execute("INSERT INTO publications VALUES(?,?,?)",
                       ("data", number, json.dumps({"manifest": evidence(root, catalog)})))
        attempt = root / "attempts/failed1"
        attempt.mkdir(parents=True)
        (attempt / "raw.json").write_text('{"partial":true}')
        db.execute("INSERT INTO attempts VALUES(?,?,?,NULL)", ("failed1", str(attempt), "failed"))
        (root / "database/release1").mkdir(parents=True)
        (root / "database/release1/receipt.json").write_text('{"verified":true}')
        db.execute("INSERT INTO targets VALUES('release1','db')")
        db.execute("INSERT INTO target_io VALUES('release1','db',?,NULL)",
                   (json.dumps(evidence(root, "database/release1/receipt.json")),))
        db.commit()
        db.close()
        orphan = root / "attempts/orphan"
        orphan.mkdir()
        (orphan / "partial").write_text("unregistered partial")
        os.utime(orphan / "partial", (1, 1))

    def test_historical_catalogs_receipts_failed_evidence_backups_and_trend(self):
        with tempfile.TemporaryDirectory() as temporary:
            base = Path(temporary)
            root = base / "live"
            root.mkdir()
            self.fixture(root)
            backup = base / "backup"
            shutil.copytree(root, backup)
            inventory = [evidence(backup, p.relative_to(backup).as_posix())
                         for p in backup.rglob("*") if p.is_file()]
            (backup / "backup.json").write_text(json.dumps({
                "schema_version": 1, "created_at": "2026-10-01T00:00:00Z", "files": inventory,
            }))
            original = snapshot(root)
            report = preview.preview(root)
            objects = {o["path"]: o for o in report["objects"]}
            self.assertEqual(objects["attempts/orphan"]["status"], "candidate_review")
            for key in ("releases/release1", "releases/release2", "catalogs/data/1.json", "catalogs/data/2.json",
                        "database/release1", "attempts/failed1", "ledger.sqlite"):
                self.assertEqual(objects[key]["status"], "protected", key)
            previous = dict(report, created_at=(datetime.now(timezone.utc) - timedelta(days=30)).isoformat(),
                            logical_bytes=report["logical_bytes"] - 300, allocated_bytes=report["allocated_bytes"] - 600)
            backed = preview.preview(root, [backup], previous=previous)
            self.assertEqual(backed["candidate_bytes"], 0)
            self.assertAlmostEqual(backed["trend"]["logical_bytes_per_day"], 10, places=3)
            self.assertEqual(snapshot(root), original)

    def test_changed_evidence_active_worker_and_scan_limits_refuse_preview(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            self.fixture(root)
            for kwargs in ({"max_files": 1}, {"max_bytes": 1}):
                with self.assertRaises(ValueError):
                    preview.preview(root, **kwargs)
            db = sqlite3.connect(root / "ledger.sqlite")
            db.execute("UPDATE attempts SET state='running'")
            db.commit()
            with self.assertRaises(ValueError):
                preview.preview(root)
            db.execute("UPDATE attempts SET state='failed'")
            db.commit()
            db.close()
            (root / "releases/release1/data.csv").write_text("tampered")
            with self.assertRaises(ValueError):
                preview.preview(root)

    def test_symlinks_escape_lock_conflict_and_previous_scope_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            self.fixture(root)
            for relative in ("../outside", "/absolute", "attempts/../outside", "attempts//outside"):
                with self.assertRaises(ValueError):
                    preview.safe(root, relative)
            with (root / "controller.lock").open("rb") as lock:
                preview.fcntl.flock(lock, preview.fcntl.LOCK_EX | preview.fcntl.LOCK_NB)
                with self.assertRaises(BlockingIOError):
                    preview.preview(root)
            previous = preview.preview(root)
            previous["root"] = "/different"
            with self.assertRaises(ValueError):
                preview.preview(root, previous=previous)
            (root / "attempts/link").symlink_to(root / "releases", target_is_directory=True)
            with self.assertRaises(ValueError):
                preview.preview(root)

    def test_committed_wal_is_included_without_creating_live_shm_or_writes(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            self.fixture(root)
            db = sqlite3.connect(root / "ledger.sqlite")
            db.execute("PRAGMA journal_mode=WAL")
            db.execute("INSERT INTO events VALUES('committed-in-wal')")
            db.commit()
            before = snapshot(root)
            result = preview.preview(root)
            self.assertEqual(result["ledger_counts"]["events"], 1)
            self.assertEqual(snapshot(root), before)
            db.close()


if __name__ == "__main__":
    unittest.main()
