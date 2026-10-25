#!/usr/bin/env python3
"""Decimal versions, owned metadata and exactly-once main-push bookkeeping.

Read-only commands work locally. Only the main push workflow can prepare/tag.
No external dependencies; also runs on the project's Python 3.9 machines.
"""
import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import time

ROOT = Path(__file__).resolve().parents[2]
SHA = re.compile(r"[0-9a-f]{40}")
MARKER = re.compile(r"(<!-- version:([\w-]+) -->)([^<]+)(<!-- /version -->)")


def number(version):
    if not isinstance(version, str) or not re.fullmatch(r"[0-9]\.[0-9]\.[0-9]", version):
        raise ValueError("version must be a.b.c with one decimal digit per component")
    return int(version.replace(".", ""))


def increment(version):
    value = number(version)
    if value == 999:
        raise ValueError("version limit 9.9.9 reached")
    return ".".join(f"{value + 1:03d}")


def git(root, *args, check=True):
    return subprocess.run(["git", "-C", str(root), *args], check=check,
                          stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)


def read_at(root, ref, name):
    result = git(root, "show", f"{ref}:{name}", check=False)
    return result.stdout if result.returncode == 0 else None


def sections(text):
    """Keep TOML bytes intact; only select explicitly owned table entries."""
    return re.split(r"(?m)(?=^\[\[?[^]\n]+\]\]?\s*$)", text)


def field(text, name):
    match = re.search(r'(?m)^' + re.escape(name) + r'\s*=\s*"([^"]+)"', text)
    return match[1] if match else None


def set_version(text, value):
    result, count = re.subn(r'(?m)^(\s*version\s*=\s*")[^"]+(")', r"\g<1>" + value + r"\2", text)
    if count != 1:
        raise ValueError("owned version field missing or ambiguous")
    return result


def cargo_versions(text):
    table = next((part for part in sections(text) if part.startswith("[package]\n")), None)
    if table is None or field(table, "name") is None:
        raise ValueError("explicit Cargo package name required")
    return field(table, "name"), field(table, "version")


def transform_cargo(text, expected_name, version, owned):
    actual, _ = cargo_versions(text)
    if actual != expected_name:
        raise ValueError("Cargo registry name mismatch")
    parts = sections(text)
    for index, part in enumerate(parts):
        header = part.splitlines()[0] if part else ""
        if header == "[package]":
            parts[index] = set_version(part, version)
        elif "dependencies." in header and field(part, "package") in owned:
            if field(part, "version") is not None:
                parts[index] = set_version(part, version)
        else:
            # Inline tables with package= also cover renamed workspace dependencies.
            def dependency(match):
                body = match[0]
                if re.search(r'package\s*=\s*"(' + "|".join(map(re.escape, owned)) + r')"', body):
                    body = re.sub(r'(version\s*=\s*")[^"]+(")', r"\g<1>" + version + r"\2", body)
                return body
            parts[index] = re.sub(r"\{[^{}\n]*\}", dependency, part)
    return "".join(parts)


def transform_lock(text, versions):
    parts = sections(text)
    for index, part in enumerate(parts):
        if part.startswith("[[package]]\n") and field(part, "name") in versions and field(part, "source") is None:
            parts[index] = set_version(part, versions[field(part, "name")])
    # Cargo's disambiguated dependency IDs include a version only when needed.
    text = "".join(parts)
    for name, version in versions.items():
        text = re.sub(r'("' + re.escape(name) + r' )[0-9]+\.[0-9]+\.[0-9]+(")',
                      r"\g<1>" + version + r"\2", text)
    return text


def registry(root):
    policy = json.loads((root / "config/release/versions.json").read_text())
    if policy["schema_version"] != 1:
        raise ValueError("unknown version registry")
    # Catch newly added metadata instead of silently failing to synchronize it.
    tracked = [name for name in git(root, "ls-files").stdout.splitlines()
               if not name.startswith(("docs/testing/evidence/", "docs/archive/", "project-review-20261022/"))]
    cargo = {name for name in tracked if name.endswith("Cargo.toml")
             and "[package]\n" in (root / name).read_text()}
    if cargo - set(policy["cargo"]):
        raise ValueError("unregistered Cargo package: " + ", ".join(sorted(cargo - set(policy["cargo"]))))
    json_paths = {row["path"] for row in policy["json_metadata"]}
    if {name for name in tracked if Path(name).name in ("package.json", "package-lock.json")} - json_paths:
        raise ValueError("register frontend package and lock version fields in versions.json")
    for name in [*policy["cargo"], *policy["locks"], *policy["documents"], *json_paths]:
        original = root / name
        path = original.resolve()
        path.relative_to(root.resolve())
        if not path.is_file() or original.is_symlink():
            raise ValueError("missing or unsafe managed path: " + name)
    return policy


def desired_files(root, version, policy, bootstrap=False):
    number(version)
    versions = (policy["bootstrap_packages"] if bootstrap
                else {name: version for name in policy["cargo"].values()})
    output = {"VERSION": version + "\n"}
    for path, name in policy["cargo"].items():
        output[path] = transform_cargo((root / path).read_text(), name, versions[name], versions)
    for path in policy["locks"]:
        output[path] = transform_lock((root / path).read_text(), versions)
    tokens = dict(versions, project=version, core=versions["jelven-rustdx-complete"],
                  cli=versions["jelven-rustdx-cli"], runner=versions["rustdx-runner"])
    for path in policy["documents"]:
        original = (root / path).read_text()
        if not MARKER.search(original):
            raise ValueError("document has no current version marker: " + path)
        def marker(match):
            if match[2] not in tokens:
                raise ValueError("unknown document version token: " + match[2])
            number(match[3])
            return match[1] + tokens[match[2]] + match[4]
        output[path] = MARKER.sub(marker, original)
    for row in policy["json_metadata"]:
        value = json.loads((root / row["path"]).read_text())
        for keys in row["fields"]:
            parent = value
            for key in keys[:-1]:
                parent = parent[key]
            parent[keys[-1]] = version
        output[row["path"]] = json.dumps(value, ensure_ascii=False, indent=2) + "\n"
    return output


def check(root, strict=False):
    policy = registry(root)
    version = (root / "VERSION").read_text().strip()
    events = list((root / ".github/version-events").glob("*.json"))
    bootstrap = not strict and not events and version == policy["bootstrap_version"]
    wanted = desired_files(root, version, policy, bootstrap)
    mismatched = [path for path, text in wanted.items() if (root / path).read_text() != text]
    if mismatched:
        raise ValueError("version metadata differs: " + ", ".join(mismatched))
    return version


def check_pr(root, base):
    """Merge-base permits versions inherited by merging the latest main."""
    baseline = git(root, "merge-base", "HEAD", base).stdout.strip()
    policy = registry(root)
    version = check(root)
    previous = read_at(root, baseline, "VERSION")
    if previous is None:
        # First policy PR introduces VERSION without changing existing versions.
        core = read_at(root, baseline, "crates/rustdx-core/Cargo.toml")
        previous = cargo_versions(core)[1] if core else None
    if previous is None or previous.strip() != version:
        raise ValueError("feature branches must inherit the main version without incrementing")
    for path, name in policy["cargo"].items():
        before = read_at(root, baseline, path)
        if before is not None and cargo_versions(before)[1] != cargo_versions((root / path).read_text())[1]:
            raise ValueError("local package version changed: " + name)
    # Event records are generated on main; PRs may inherit, but never mint them.
    names = git(root, "diff", "--name-only", baseline, "HEAD", "--", ".github/version-events").stdout
    if names.strip():
        raise ValueError("feature branches cannot create or modify main push receipts")
    return version


def event_environment(environ):
    if (environ.get("GITHUB_ACTIONS") != "true" or environ.get("GITHUB_EVENT_NAME") != "push"
            or environ.get("GITHUB_REF") != "refs/heads/main"):
        raise ValueError("version writes require the GitHub Actions main push workflow")
    run_id, source = environ.get("GITHUB_RUN_ID", ""), environ.get("GITHUB_SHA", "")
    if not re.fullmatch(r"[1-9][0-9]*", run_id) or not SHA.fullmatch(source):
        raise ValueError("invalid push event identity")
    event = json.loads(Path(environ["GITHUB_EVENT_PATH"]).read_text())
    before = event.get("before", "")
    if not SHA.fullmatch(before) or event.get("after") != source or event.get("ref") != "refs/heads/main":
        raise ValueError("push payload does not match workflow identity")
    return run_id, source, before


def receipt_commit(root, ref, run_id, source=None, before=None):
    path = f".github/version-events/{run_id}.json"
    text = read_at(root, ref, path)
    if text is None:
        return None
    record = json.loads(text)
    if (record["schema_version"] != 1 or str(record["run_id"]) != run_id
            or (source is not None and record["source_head"] != source)
            or (before is not None and record["before"] != before)
            or increment(record["old_version"]) != record["version"]):
        raise ValueError("push receipt identity mismatch")
    commits = git(root, "log", "--format=%H", "--diff-filter=A", ref, "--", path).stdout.splitlines()
    if len(commits) != 1:
        raise ValueError("ambiguous push receipt history")
    commit = commits[0]
    if read_at(root, commit, path) != text or read_at(root, commit, "VERSION") != record["version"] + "\n":
        raise ValueError("push receipt changed or has wrong version")
    if git(root, "rev-parse", commit + "^").stdout.strip() != record["base_commit"]:
        raise ValueError("push receipt parent mismatch")
    baseline = read_at(root, record["base_commit"], "VERSION")
    if baseline != record["old_version"] + "\n":
        raise ValueError("push receipt previous version mismatch")
    if git(root, "merge-base", "--is-ancestor", record["source_head"], record["base_commit"], check=False).returncode:
        raise ValueError("push receipt source is not included")
    return dict(record, commit=commit)


def prepare(root, run_id, source, before, retries=16):
    if git(root, "status", "--porcelain").stdout:
        raise ValueError("prepare requires a clean checkout")
    for attempt in range(retries):
        git(root, "fetch", "origin", "+refs/heads/main:refs/remotes/origin/main")
        base = git(root, "rev-parse", "refs/remotes/origin/main").stdout.strip()
        existing = receipt_commit(root, base, run_id, source, before)
        if existing:
            return existing
        if git(root, "merge-base", "--is-ancestor", source, base, check=False).returncode:
            raise ValueError("event source is no longer on main")
        git(root, "checkout", "--detach", base)
        old = check(root)
        version = increment(old)  # Check cap before any writes.
        policy = registry(root)
        files = desired_files(root, version, policy)
        for name, text in files.items():
            (root / name).write_text(text)
        path = f".github/version-events/{run_id}.json"
        record = dict(schema_version=1, run_id=run_id, source_head=source, before=before,
                      base_commit=base, old_version=old, version=version)
        (root / path).parent.mkdir(parents=True, exist_ok=True)
        (root / path).write_text(json.dumps(record, indent=2) + "\n")
        check(root, strict=True)
        git(root, "add", "--", *files, path)
        git(root, "-c", "user.name=github-actions[bot]",
            "-c", "user.email=41898282+github-actions[bot]@users.noreply.github.com",
            "commit", "-m", f"chore: main push {run_id} version {version}")
        result = git(root, "push", "origin", "HEAD:refs/heads/main", check=False)
        if result.returncode == 0:
            return receipt_commit(root, "HEAD", run_id, source, before)
        # Lost ACKs and competing pushes must both be resolved from remote history.
        git(root, "fetch", "origin", "+refs/heads/main:refs/remotes/origin/main")
        remote = git(root, "rev-parse", "refs/remotes/origin/main").stdout.strip()
        existing = receipt_commit(root, remote, run_id, source, before)
        if existing:
            return existing
        if remote == base:
            raise ValueError("main version push rejected: " + result.stderr.strip())
        time.sleep(min(attempt + 1, 5))
    raise ValueError("main kept advancing; rerun this event to finish its increment")


def history_refs(root):
    return {ref: oid for oid, ref in
            (line.split() for line in git(root, "ls-remote", "--refs", "origin",
                                          "refs/tags/history/*").stdout.splitlines())}


def retain_history(root, record, retries=16):
    """Keep three source snapshots by main ancestry; release/data refs are untouched."""
    candidate = "refs/tags/history/source-" + record["version"]
    commit = record["commit"]
    for _ in range(retries):
        git(root, "fetch", "--no-tags", "origin",
            "+refs/heads/main:refs/remotes/origin/main")
        before = history_refs(root)
        if candidate in before and before[candidate] != commit:
            raise ValueError("history tag already points to a different commit")
        order = {sha: index for index, sha in enumerate(
            git(root, "rev-list", "--topo-order", "origin/main").stdout.splitlines())}
        positions = {}
        for ref, oid in before.items():
            # Fetch the exact object rather than mutable local tags. Annotated tags stay intact.
            git(root, "fetch", "--no-tags", "origin", oid)
            target = git(root, "rev-parse", oid + "^{commit}").stdout.strip()
            if target not in order:
                raise ValueError("history target is not on main: " + ref)
            positions[ref] = order[target]
        if commit not in order:
            raise ValueError("checked history commit is not on main")
        positions[candidate] = order[commit]
        all_refs = dict(before, **{candidate: commit})
        keep = sorted(all_refs, key=lambda ref: (positions[ref], ref))[:3]
        wanted = {ref: all_refs[ref] for ref in keep}
        additions = {ref: oid for ref, oid in wanted.items() if ref not in before}
        deletions = {ref: oid for ref, oid in before.items() if ref not in wanted}
        if not additions and not deletions:
            return dict(retained=wanted, count=len(wanted))
        leases = [f"--force-with-lease={ref}:{before.get(ref, '')}"
                  for ref in sorted(set(additions) | set(deletions))]
        refspecs = [f"{oid}:{ref}" for ref, oid in additions.items()]
        refspecs += [":" + ref for ref in deletions]
        result = git(root, "push", "--atomic", *leases, "origin", *refspecs, check=False)
        after = history_refs(root)
        if after == wanted:
            return dict(retained=after, count=len(after))
        if result.returncode and after == before:
            raise ValueError("history atomic push rejected: " + result.stderr.strip())
        # Competing completion or lost ACK: rank the reread refs, never replay old deletions.
    raise ValueError("history refs kept changing; rerun the checked event")


def tag(root, record):
    version, commit = record["version"], record["commit"]
    number(version)
    ref = "refs/tags/v" + version
    def remote_target():
        rows = git(root, "ls-remote", "origin", ref).stdout.splitlines()
        return rows[0].split()[0] if rows else None
    existing = remote_target()
    if existing:
        if existing != commit:
            raise ValueError("version tag already points to a different commit")
    else:
        git(root, "update-ref", ref, commit)
        result = git(root, "push", "origin", ref, check=False)
        if result.returncode and remote_target() != commit:
            raise ValueError("version tag push failed: " + result.stderr.strip())
    return retain_history(root, record)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=["check", "check-pr", "prepare", "tag"])
    parser.add_argument("--base")
    parser.add_argument("--strict", action="store_true")
    args = parser.parse_args()
    try:
        if args.command == "check":
            result = dict(version=check(ROOT, args.strict))
        elif args.command == "check-pr":
            if not args.base:
                parser.error("--base is required")
            result = dict(version=check_pr(ROOT, args.base))
        else:
            run_id, source, before = event_environment(os.environ)
            if args.command == "prepare":
                result = prepare(ROOT, run_id, source, before)
            else:
                git(ROOT, "fetch", "origin", "+refs/heads/main:refs/remotes/origin/main")
                result = receipt_commit(ROOT, "origin/main", run_id, source, before)
                if result is None or git(ROOT, "rev-parse", "HEAD").stdout.strip() != result["commit"]:
                    raise ValueError("tag requires this event's checked version commit")
                check(ROOT, strict=True)
                result["history"] = tag(ROOT, result)
            if os.environ.get("GITHUB_OUTPUT"):
                with open(os.environ["GITHUB_OUTPUT"], "a") as output:
                    output.write(f"version={result['version']}\ncommit={result['commit']}\n")
        print(json.dumps(result))
        return 0
    except (ValueError, KeyError, OSError, subprocess.SubprocessError) as error:
        parser.exit(1, f"version policy: {error}\n")


if __name__ == "__main__":
    raise SystemExit(main())
