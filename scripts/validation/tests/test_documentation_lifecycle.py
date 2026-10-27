import json
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from check_documentation_lifecycle import check


class DocumentationLifecycle(unittest.TestCase):
    def test_schema_anchor_rejects_source_document_drift(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "ledger.rs"
            document = root / "docs/ledger.md"
            document.parent.mkdir()
            source.write_text("pub const SCHEMA_VERSION: i64 = 4;")
            document.write_text("账本schema为4")
            policy = root / "config/documentation/registry.json"
            policy.parent.mkdir(parents=True)
            policy.write_text(json.dumps(dict(schema_version=1, critical_documents=[],
                source_anchors=[dict(id="ledger-schema", source="ledger.rs",
                    source_pattern=r"SCHEMA_VERSION: i64 = (\d+)",
                    documents=[dict(path="docs/ledger.md", pattern=r"账本schema为(\d+)")])])))
            self.assertEqual(check(root, "2026-10-27")["status"], "passed")
            source.write_text("pub const SCHEMA_VERSION: i64 = 5;")
            self.assertEqual(check(root, "2026-10-27")["status"], "failed")
            document.write_text("账本schema为5")
            self.assertEqual(check(root, "2026-10-27")["status"], "passed")
            document.write_text("账本schema为5；账本schema为4")
            self.assertEqual(check(root, "2026-10-27")["status"], "failed")

    def test_stale_missing_and_unregistered_operational_docs_fail(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            path = root / "docs/runbooks/RUNBOOK.md"
            path.parent.mkdir(parents=True)
            path.write_text("# Runbook\n<!-- lifecycle:registry -->")
            source = root / "src"
            source.mkdir()
            policy = root / "config/documentation/registry.json"
            policy.parent.mkdir(parents=True)
            row = dict(path="docs/runbooks/RUNBOOK.md", quadrant="how-to", owner="ops",
                       audience="operators", source_of_truth="src", verified_on="2026-10-25",
                       cadence_days=30, state="source-verified", trigger="incident",
                       stale_signal="time or code change", check="check_documentation_lifecycle.py")
            def save(rows):
                policy.write_text(json.dumps(dict(schema_version=1, critical_documents=rows)))
            save([row])
            self.assertEqual(check(root, "2026-11-24")["status"], "passed")
            self.assertEqual(check(root, "2026-11-25")["status"], "failed")
            self.assertEqual(check(root, "2026-10-24")["status"], "failed")
            save([dict(row, source_of_truth="../escape")])
            self.assertEqual(check(root, "2026-10-25")["status"], "failed")
            save([row, row])
            self.assertEqual(check(root, "2026-10-25")["status"], "failed")
            save([row])
            extra = path.parent / "NEW.md"
            extra.write_text("# Critical operation")
            self.assertEqual(check(root, "2026-10-25")["status"], "failed")
