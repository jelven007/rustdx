"""Private Git remotes exercise main-push races without touching real versions."""
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / "scripts/release"))
import versioning as v
from release_gate import current_version_event


def commit(root, message):
    v.git(root, "add", ".")
    v.git(root, "commit", "-qm", message)
    return v.git(root, "rev-parse", "HEAD").stdout.strip()


def fixture(root):
    policy = json.loads((ROOT / "config/release/versions.json").read_text())
    names = ["VERSION", "config/release/versions.json", *policy["cargo"], *policy["locks"], *policy["documents"]]
    for name in names:
        target = root / name
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(ROOT / name, target)
    # Historical Cargo files and version text must never be synchronized.
    historical = root / "docs/testing/evidence/sample/Cargo.toml"
    historical.parent.mkdir(parents=True)
    historical.write_text('[package]\nname="old"\nversion="1.12.2"\n')
    v.git(root, "init", "-q", "-b", "main")
    v.git(root, "config", "user.name", "Version tests")
    v.git(root, "config", "user.email", "tests@example.invalid")
    return commit(root, "fixture")


class VersionPolicy(unittest.TestCase):
    def test_all_decimal_carries_and_limit(self):
        for n in range(999):
            old, new = ".".join(f"{n:03d}"), ".".join(f"{n + 1:03d}")
            self.assertEqual(v.increment(old), new)
        for invalid in ["9.9.9", "1.12.2", "10.0.0", "01.0.0", "1.0", "v1.0.0", "1.0.0-dev", "-1.0.0", None]:
            with self.assertRaises(ValueError, msg=str(invalid)):
                v.increment(invalid)

    def test_no_local_or_feature_write_entry(self):
        for env in [{}, {"GITHUB_ACTIONS": "true", "GITHUB_EVENT_NAME": "pull_request"},
                    {"GITHUB_ACTIONS": "true", "GITHUB_EVENT_NAME": "push", "GITHUB_REF": "refs/heads/feature"}]:
            with self.assertRaises(ValueError):
                v.event_environment(env)

    def test_owned_metadata_locks_frontend_and_history(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            fixture(root)
            self.assertEqual(v.check(root), "2.0.0")
            policy = v.registry(root)
            frontend = root / "web/package.json"
            frontend.parent.mkdir()
            frontend.write_text('{"version":"2.0.0","packages":{"":{"version":"2.0.0"}},"third_party":"8.8.8"}\n')
            v.git(root, "add", "web")
            with self.assertRaisesRegex(ValueError, "frontend"):
                v.registry(root)
            policy["json_metadata"] = [dict(path="web/package.json",
                                           fields=[["version"], ["packages", "", "version"]])]
            (root / "config/release/versions.json").write_text(json.dumps(policy, indent=2) + "\n")
            old_lock = (root / "Cargo.lock").read_text()
            original_history = (root / "docs/testing/evidence/sample/Cargo.toml").read_bytes()
            for name, text in v.desired_files(root, "2.0.1", v.registry(root)).items():
                (root / name).write_text(text)
            self.assertEqual(v.check(root, strict=True), "2.0.1")
            self.assertIn('"third_party": "8.8.8"', frontend.read_text())
            self.assertEqual((root / "docs/testing/evidence/sample/Cargo.toml").read_bytes(), original_history)
            # Entire third-party lock blocks, including source/checksum, stay byte-identical.
            owned = set(policy["cargo"].values())
            external = lambda text: [p for p in v.sections(text)
                                     if p.startswith("[[package]]") and v.field(p, "name") not in owned]
            self.assertEqual(external(old_lock), external((root / "Cargo.lock").read_text()))
            (root / "apps/rustdx-cli/Cargo.toml").write_text(
                (root / "apps/rustdx-cli/Cargo.toml").read_text().replace('version  = "2.0.1"', 'version  = "2.0.0"'))
            with self.assertRaisesRegex(ValueError, "metadata differs"):
                v.check(root, strict=True)

    def test_pr_inheritance_and_illegal_local_increment(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            first = fixture(root)
            v.git(root, "checkout", "-qb", "feature")
            (root / "feature.txt").write_text("feature")
            commit(root, "feature")
            self.assertEqual(v.check_pr(root, first), "2.0.0")
            v.git(root, "checkout", "main")
            for name, text in v.desired_files(root, "2.0.1", v.registry(root)).items():
                (root / name).write_text(text)
            inherited = commit(root, "synthetic main sync")
            v.git(root, "checkout", "feature")
            v.git(root, "merge", "--no-edit", "main")
            self.assertEqual(v.check_pr(root, inherited), "2.0.1")
            for name, text in v.desired_files(root, "2.0.2", v.registry(root)).items():
                (root / name).write_text(text)
            commit(root, "illegal local sync")
            with self.assertRaisesRegex(ValueError, "inherit"):
                v.check_pr(root, inherited)

    def test_event_payload_and_main_cap(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            sha = fixture(root)
            event = root / "event.json"
            event.write_text(json.dumps(dict(before="0" * 40, after=sha, ref="refs/heads/main")))
            env = dict(GITHUB_ACTIONS="true", GITHUB_EVENT_NAME="push", GITHUB_REF="refs/heads/main",
                       GITHUB_RUN_ID="123", GITHUB_SHA=sha, GITHUB_EVENT_PATH=str(event))
            self.assertEqual(v.event_environment(env), ("123", sha, "0" * 40))
            event.write_text(json.dumps(dict(before="0" * 40, after="f" * 40, ref="refs/heads/main")))
            with self.assertRaisesRegex(ValueError, "payload"):
                v.event_environment(env)
            event.unlink()
            for name, text in v.desired_files(root, "9.9.9", v.registry(root)).items():
                (root / name).write_text(text)
            sha = commit(root, "cap")
            remote = root.parent / (root.name + "-remote.git")
            try:
                v.git(root, "clone", "--bare", str(root), str(remote))
                v.git(root, "remote", "add", "origin", str(remote))
                with self.assertRaisesRegex(ValueError, "limit"):
                    v.prepare(root, "1", sha, "0" * 40)
                self.assertFalse(v.git(root, "status", "--porcelain").stdout)
            finally:
                shutil.rmtree(remote, ignore_errors=True)

    def test_remote_replay_multi_commit_push_cas_lost_ack_and_tags(self):
        with tempfile.TemporaryDirectory() as directory:
            parent = Path(directory)
            root, remote, racer = parent / "worker", parent / "remote.git", parent / "racer"
            root.mkdir()
            first = fixture(root)
            (root / "one.txt").write_text("one")
            commit(root, "first commit in one push")
            (root / "two.txt").write_text("two")
            source = commit(root, "second commit in one push")
            v.git(root, "clone", "--bare", str(root), str(remote))
            v.git(root, "remote", "add", "origin", str(remote))
            event1 = v.prepare(root, "100", source, first)
            self.assertEqual(event1["version"], "2.0.1")
            self.assertEqual(v.prepare(root, "100", source, first), event1)
            self.assertEqual(current_version_event(root, event1["commit"]), event1)
            v.git(root, "clone", str(remote), str(racer))
            v.git(racer, "config", "user.name", "Race tests")
            v.git(racer, "config", "user.email", "tests@example.invalid")
            (racer / "three.txt").write_text("third")
            source2 = commit(racer, "second source push")
            v.git(racer, "push", "origin", "main")
            # Worker2 writes a new event between worker1's fetch and FF push.
            original_git = v.git
            injected = []
            def racing_git(where, *args, **kwargs):
                if where == root and args[:2] == ("push", "origin") and not injected:
                    injected.append(v.prepare(racer, "102", source2, event1["commit"]))
                return original_git(where, *args, **kwargs)
            with patch.object(v, "git", side_effect=racing_git), patch.object(v.time, "sleep"):
                event2 = v.prepare(root, "101", source2, event1["commit"])
            self.assertEqual(injected[0]["version"], "2.0.2")
            self.assertEqual(event2["version"], "2.0.3")
            # An older rerun always returns the original SHA without rolling main back.
            self.assertEqual(v.prepare(root, "100", source, first), event1)
            self.assertEqual(v.git(remote, "rev-parse", "main").stdout.strip(), event2["commit"])
            with self.assertRaisesRegex(ValueError, "identity"):
                v.prepare(root, "100", source2, first)
            lost = []
            def lost_ack(where, *args, **kwargs):
                result = original_git(where, *args, **kwargs)
                if where == root and args[:2] == ("push", "origin") and not lost:
                    lost.append(True)
                    result.returncode, result.stderr = 1, "synthetic acknowledgement lost"
                return result
            with patch.object(v, "git", side_effect=lost_ack):
                event3 = v.prepare(root, "103", source2, event1["commit"])
            self.assertEqual(event3["version"], "2.0.4")
            self.assertEqual(v.prepare(root, "103", source2, event1["commit"]), event3)
            # Tags may complete out of order, but each points to its checked snapshot.
            for event in [event3, event1, event2]:
                v.tag(root, event)
                v.tag(root, event)
                self.assertEqual(v.git(remote, "rev-parse", "v" + event["version"]).stdout.strip(), event["commit"])
            with self.assertRaisesRegex(ValueError, "different commit"):
                v.tag(root, dict(event1, commit=event3["commit"]))

    def test_rejected_write_fails_without_repeated_increments(self):
        with tempfile.TemporaryDirectory() as directory:
            parent = Path(directory)
            root, remote = parent / "worker", parent / "remote.git"
            root.mkdir()
            source = fixture(root)
            v.git(root, "clone", "--bare", str(root), str(remote))
            v.git(root, "remote", "add", "origin", str(remote))
            hook = remote / "hooks/pre-receive"
            hook.write_text("#!/bin/sh\nexit 1\n")
            hook.chmod(0o755)
            with self.assertRaisesRegex(ValueError, "rejected"):
                v.prepare(root, "100", source, "0" * 40)
            self.assertEqual(v.git(remote, "rev-parse", "main").stdout.strip(), source)

    def test_history_retains_three_by_main_order_and_old_replay_cannot_resurrect(self):
        with tempfile.TemporaryDirectory() as directory:
            parent = Path(directory)
            root, remote = parent / "worker", parent / "remote.git"
            root.mkdir()
            first = fixture(root)
            v.git(root, "tag", "-a", "history/old", "-m", "original", first)
            (root / "next").write_text("next")
            second = commit(root, "second")
            v.git(root, "tag", "history/newer", second)
            v.git(root, "clone", "--bare", str(root), str(remote))
            v.git(root, "remote", "add", "origin", str(remote))
            events = [v.prepare(root, str(n), second, first) for n in range(100, 104)]
            for event in reversed(events):
                v.tag(root, event)
            expected = {f"refs/tags/history/source-{e['version']}": e["commit"]
                        for e in events[-3:]}
            self.assertEqual(v.history_refs(root), expected)
            v.tag(root, events[0])
            self.assertEqual(v.history_refs(root), expected)
            for event in events:
                self.assertEqual(v.git(remote, "rev-parse", "v" + event["version"]).stdout.strip(),
                                 event["commit"])
            # Foreign history cannot be ranked safely and is never silently deleted.
            v.git(root, "checkout", "--orphan", "foreign")
            v.git(root, "rm", "-rf", ".")
            (root / "unrelated").write_text("unrelated")
            unrelated = commit(root, "foreign")
            v.git(root, "tag", "history/foreign", unrelated)
            v.git(root, "push", "origin", "refs/tags/history/foreign")
            with self.assertRaisesRegex(ValueError, "not on main"):
                v.retain_history(root, events[-1])
            self.assertEqual(v.history_refs(root)["refs/tags/history/foreign"], unrelated)

    def test_history_atomic_race_and_lost_ack_converge(self):
        with tempfile.TemporaryDirectory() as directory:
            parent = Path(directory)
            root, remote = parent / "worker", parent / "remote.git"
            root.mkdir()
            source = fixture(root)
            v.git(root, "clone", "--bare", str(root), str(remote))
            v.git(root, "remote", "add", "origin", str(remote))
            events = [v.prepare(root, str(n), source, "0" * 40) for n in range(200, 204)]
            for event in events[:3]:
                v.tag(root, event)
            original_git = v.git
            intercepted = []
            def lost_ack(where, *args, **kwargs):
                result = original_git(where, *args, **kwargs)
                if args[:2] == ("push", "--atomic") and not intercepted:
                    intercepted.append(True)
                    result.returncode, result.stderr = 1, "lost ACK after atomic update"
                return result
            with patch.object(v, "git", side_effect=lost_ack):
                v.tag(root, events[-1])
            self.assertTrue(intercepted)
            self.assertEqual(set(v.history_refs(root)),
                             {f"refs/tags/history/source-{e['version']}" for e in events[-3:]})
            newest = v.prepare(root, "204", source, "0" * 40)
            raced = []
            def concurrent_history(where, *args, **kwargs):
                if args[:2] == ("push", "--atomic") and not raced:
                    raced.append(True)
                    original_git(remote, "update-ref", "refs/tags/history/z-concurrent",
                                 newest["commit"])
                return original_git(where, *args, **kwargs)
            with patch.object(v, "git", side_effect=concurrent_history):
                v.tag(root, newest)
            refs = v.history_refs(root)
            self.assertTrue(raced)
            self.assertEqual(len(refs), 3)
            self.assertIn(newest["commit"], refs.values())
            self.assertNotIn(events[1]["commit"], refs.values())

    def test_workflow_order_all_checkout_revisions_and_permissions(self):
        main = (ROOT / ".github/workflows/main-version.yml").read_text()
        quality = (ROOT / ".github/workflows/ci.yml").read_text()
        database = (ROOT / ".github/workflows/database-acceptance.yml").read_text()
        self.assertIn("needs: [prepare, quality]", main)
        self.assertIn("revision: ${{ needs.prepare.outputs.commit }}", main)
        self.assertNotIn("cancel-in-progress:", main)
        self.assertNotIn("paths:", main)
        for workflow in [quality, database]:
            self.assertNotIn("\n  push:", workflow)
            self.assertEqual(workflow.count("uses: actions/checkout@"), workflow.count("persist-credentials: false"))
            self.assertEqual(workflow.count("uses: actions/checkout@"),
                             workflow.count("ref: ${{ inputs.revision || github.sha }}"))
            self.assertNotIn("contents: write", workflow)
        self.assertIn("uses: ./.github/workflows/database-acceptance.yml", quality)
