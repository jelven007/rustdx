import hashlib
import io
import json
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "release"))
from evidence_manifest import bundle, restore, safe_path, verify
from source_scope import fingerprint


class EvidenceMigration(unittest.TestCase):
    def test_pinned_archives_still_reject_extra_duplicate_missing_link_and_changed_entries(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            data = b"original"
            row = dict(path="sample.bin", bytes=len(data), sha256=hashlib.sha256(data).hexdigest(),
                       availability="ordinary_git")
            for index, case in enumerate(["extra", "duplicate", "missing", "link", "changed"]):
                archive = root / (case + ".tar.gz")
                with tarfile.open(archive, "w:gz") as stream:
                    for name in ([] if case == "missing" else ["sample.bin"]):
                        info = tarfile.TarInfo(name)
                        info.size = len(data)
                        if case == "link":
                            info.type, info.linkname = tarfile.SYMTYPE, "../outside"
                            info.size = 0
                        stream.addfile(info, io.BytesIO(b"modified" if case == "changed" else data))
                    if case in ["extra", "duplicate"]:
                        info = tarfile.TarInfo("extra.bin" if case == "extra" else "sample.bin")
                        info.size = len(data)
                        stream.addfile(info, io.BytesIO(data))
                manifest = dict(schema_version=1, files=[row], files_sha256=fingerprint([row]),
                                archive=dict(bytes=archive.stat().st_size,
                                             sha256=hashlib.sha256(archive.read_bytes()).hexdigest()))
                target = root / ("restored-" + str(index))
                with self.assertRaises(ValueError, msg=case):
                    restore(target, manifest, archive)
                self.assertFalse(target.exists(), case)

    def test_restore_materializes_exact_bytes_in_new_root_and_rejects_corrupt_archive(self):
        with tempfile.TemporaryDirectory() as directory:
            parent = Path(directory)
            source = parent / "source"
            source.mkdir()
            data = b"frozen\r\n\x00bytes"
            (source / "sample.bin").write_bytes(data)
            row = dict(path="sample.bin", bytes=len(data),
                       sha256=hashlib.sha256(data).hexdigest(), availability="ordinary_git")
            manifest = dict(schema_version=1, files=[row], files_sha256=fingerprint([row]))
            archive = parent / "sample.tar.gz"
            bundled = bundle(source, manifest, archive, "sample")
            manifest_path = parent / "manifest.json"
            manifest_path.write_text(json.dumps(bundled))
            script = Path(__file__).resolve().parents[2] / "release/evidence_manifest.py"
            restored = parent / "restored"
            def run(root, output):
                return subprocess.run(
                    [sys.executable, str(script), "restore", "--root", str(root),
                     "--manifest", str(manifest_path), "--archive", str(archive),
                     "--output", str(output)], capture_output=True, text=True)
            result = run(restored, parent / "result.json")
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual((restored / "sample.bin").read_bytes(), data)
            self.assertEqual(verify(restored, manifest)["status"], "passed")
            self.assertNotEqual(run(restored, parent / "repeat.json").returncode, 0)
            archive.write_bytes(archive.read_bytes() + b"corrupt")
            rejected = parent / "rejected"
            self.assertNotEqual(run(rejected, parent / "corrupt.json").returncode, 0)
            self.assertFalse(rejected.exists())

    def test_old_manifest_verifies_and_bundles_original_names_without_changing_bytes(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            old = "project-review-20261022/log.txt"
            new = "docs/testing/evidence/project-review-20261022/log.txt"
            path = root / new
            path.parent.mkdir(parents=True)
            data = b"frozen\r\n\x00bytes"
            path.write_bytes(data)
            mapping = root / "docs/testing/evidence/production-lifecycle-20261025/migration.json"
            mapping.parent.mkdir()
            mapping.write_text(json.dumps(dict(files=[dict(old=old, new=new)])))
            row = dict(path=old, bytes=len(data), sha256=hashlib.sha256(data).hexdigest(),
                       availability="ordinary_git")
            manifest = dict(schema_version=1, files=[row], files_sha256=fingerprint([row]))
            original = json.dumps(manifest)
            self.assertEqual(verify(root, manifest)["status"], "passed")
            archive = root / "frozen.tar.gz"
            bundle(root, manifest, archive, "project-review-20261022/")
            with tarfile.open(archive) as stream:
                self.assertEqual(stream.getnames(), [old])
                self.assertEqual(stream.extractfile(old).read(), data)
            self.assertEqual(json.dumps(manifest), original)
            path.write_bytes(data + b"changed")
            self.assertEqual(verify(root, manifest)["status"], "incomplete")
            mapping.write_text(json.dumps(dict(files=[dict(old=old, new="../outside")])))
            with self.assertRaises(ValueError):
                safe_path(root, old)
