import hashlib
import io
import json
from pathlib import Path
import sys
import tarfile
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "release"))
from evidence_manifest import bundle, safe_path, verify
from source_scope import fingerprint


class EvidenceMigration(unittest.TestCase):
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
