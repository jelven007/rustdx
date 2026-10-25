#!/usr/bin/env python3
"""Require current-SHA successful GitHub jobs and matching actual package bytes."""
import argparse
import datetime as dt
import hashlib
import io
import json
from pathlib import Path
import re
import subprocess
import zipfile

from source_scope import digest_file, inventory
from versioning import receipt_commit

ROOT = Path(__file__).resolve().parents[2]


def check_runs(sha, policy, workflows, version_event=None):
    issues, selected = [], {}
    for workflow, expected_jobs in policy["workflows"].items():
        event_sha = version_event["source_head"] if version_event else sha
        candidates = [run for run in workflows.get(workflow, [])
                      if run.get("head_sha") == event_sha and run.get("event") in policy["events"]
                      and (version_event is None or (
                          str(run.get("id")) == version_event["run_id"] and version_event["commit"] == sha))
                      and run.get("path", "").split("@")[0] == ".github/workflows/" + workflow]
        if not candidates:
            issues.append(f"{workflow}:missing_current_sha_run")
            continue
        latest = max(candidates, key=lambda run: (int(run["id"]), int(run.get("run_attempt", 1))))
        selected[workflow] = latest
        if latest.get("status") != "completed" or latest.get("conclusion") != "success":
            issues.append(f"{workflow}:not_successful")
        jobs = latest.get("jobs", [])
        for name in expected_jobs:
            matched = [job for job in jobs if job.get("name") == name]
            if len(matched) != 1 or matched[0].get("status") != "completed" or matched[0].get("conclusion") != "success":
                issues.append(f"{workflow}:{name}:missing_or_not_successful")
    return selected, issues


def verify_packages(data, sha, source_sha, policy):
    with zipfile.ZipFile(io.BytesIO(data)) as archive:
        members = archive.infolist()
        if len(members) > 10000 or sum(member.file_size for member in members) > 512 * 1024 * 1024:
            raise ValueError("package artifact budget")
        reports = [member for member in members if Path(member.filename).name == "preflight.json"]
        if len(reports) != 1 or reports[0].file_size > 32 * 1024 * 1024:
            raise ValueError("missing or ambiguous package report")
        report = json.loads(archive.read(reports[0]))
        if (report.get("schema_version") != 2 or report.get("head") != sha or report.get("dirty") is not False
                or report.get("source_sha256") != source_sha or report.get("result") != "passed"
                or not report.get("checks") or any(row.get("exit_code") != 0 for row in report["checks"])):
            raise ValueError("actual package report does not match clean current source and successful checks")
        packages = report.get("packages", [])
        if sorted(row["name"] for row in packages) != sorted(policy["package_names"]):
            raise ValueError("actual package names differ")
        for package in packages:
            matches = [member for member in members if Path(member.filename).name == Path(package["crate"]).name]
            if len(matches) != 1 or hashlib.sha256(archive.read(matches[0])).hexdigest() != package["sha256"]:
                raise ValueError("actual crate missing, ambiguous or changed")
        return {"packages": packages, "preflight_sha256": hashlib.sha256(archive.read(reports[0])).hexdigest(),
                "artifact_sha256": hashlib.sha256(data).hexdigest()}


def api(endpoint, raw=False):
    result = subprocess.check_output(["gh", "api", endpoint], timeout=45)
    return result if raw else json.loads(result)


def paged(endpoint, key):
    rows = []
    for page in range(1, 101):
        separator = "&" if "?" in endpoint else "?"
        result = api(endpoint + f"{separator}per_page=100&page={page}")
        rows.extend(result[key])
        if len(result[key]) < 100:
            return rows
    raise ValueError("GitHub response exceeds page budget")


def collect(repository, sha, policy, version_event=None):
    workflows = {}
    for workflow in policy["workflows"]:
        event_sha = version_event["source_head"] if version_event else sha
        runs = paged(f"repos/{repository}/actions/workflows/{workflow}/runs?head_sha={event_sha}", "workflow_runs")
        selected, _ = check_runs(sha, dict(policy, workflows={workflow: policy["workflows"][workflow]}),
                                 {workflow: runs}, version_event)
        if workflow in selected:
            run = selected[workflow]
            # The attempts endpoint avoids treating jobs from an older successful rerun as current.
            run["jobs"] = paged(
                f"repos/{repository}/actions/runs/{run['id']}/attempts/{run.get('run_attempt', 1)}/jobs", "jobs")
        workflows[workflow] = runs
    return workflows


def current_version_event(root, sha):
    names = subprocess.check_output(
        ["git", "-C", str(root), "diff-tree", "--no-commit-id", "--name-only", "-r", sha]
    ).decode().splitlines()
    receipts = [name for name in names if re.fullmatch(r"\.github/version-events/[1-9][0-9]*\.json", name)]
    if len(receipts) != 1:
        raise ValueError("release HEAD must be a CI version commit with one push receipt")
    run_id = Path(receipts[0]).stem
    event = receipt_commit(root, sha, run_id)
    if event is None or event["commit"] != sha:
        raise ValueError("release HEAD does not match the version receipt")
    return event


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--preflight", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--repository", default="jelven007/rustdx")
    parser.add_argument("--snapshot-runs", type=Path, help="offline evaluation only; never authorizes publication")
    args = parser.parse_args()
    if args.output.exists():
        parser.error("use a new release manifest")
    if not re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", args.repository):
        parser.error("invalid repository")
    report = {"schema_version": 1, "publication_qualified": False,
              "checked_at": dt.datetime.now(dt.timezone.utc).isoformat(), "issues": [],
              "registry_verification": "pending core upload, index visibility and CLI registry dry-run"}
    try:
        local = json.loads(args.preflight.read_text())
        policy = json.loads((ROOT / "config/release/gates.json").read_text())
        sha = subprocess.check_output(["git", "-C", str(ROOT), "rev-parse", "HEAD"]).decode().strip()
        status = subprocess.check_output(["git", "-C", str(ROOT), "status", "--porcelain=v1", "-z"])
        _, source_sha = inventory(ROOT)
        report.update(head=sha, source_sha256=source_sha, repository=args.repository,
                      preflight_sha256=digest_file(args.preflight))
        if (status or local.get("schema_version") != 2 or local.get("source_scope_version") != 1
                or local.get("head") != sha or local.get("source_sha256") != source_sha
                or local.get("local_gate_passed") is not True or local.get("dirty") is not False
                or local.get("result") != "passed"):
            raise ValueError("local full preflight and clean current commit are required")
        for package in local["packages"]:
            path = (args.preflight.parent / package["crate"]).resolve()
            path.relative_to(args.preflight.parent.resolve())
            if digest_file(path) != package["sha256"]:
                raise ValueError("local reviewed package bytes changed")
        if sorted(row["name"] for row in local.get("binaries", [])) != ["rustdx", "rustdx-runner"]:
            raise ValueError("both reviewed release binaries are required")
        for binary in local["binaries"]:
            path = (args.preflight.parent / binary["path"]).resolve()
            path.relative_to(args.preflight.parent.resolve())
            if digest_file(path) != binary["sha256"]:
                raise ValueError("reviewed release binary changed")
        version_event = current_version_event(ROOT, sha)
        workflows = (json.loads(args.snapshot_runs.read_text()) if args.snapshot_runs
                     else collect(args.repository, sha, policy, version_event))
        selected, issues = check_runs(sha, policy, workflows, version_event)
        report.update(workflows=selected, issues=issues, binaries=local.get("binaries", []),
                      version_event=version_event)
        report["inputs"] = {name: digest_file(ROOT / name) for name in [
            "VERSION", "config/release/versions.json",
            "Cargo.lock", "config/release/gates.json", "config/release/action-pins.json",
            "deploy/acceptance/compose.yml", "deploy/acceptance/tool-archives.sha256"]}
        if not issues and not args.snapshot_runs:
            run_id = selected[policy["artifact_workflow"]]["id"]
            artifacts = paged(f"repos/{args.repository}/actions/runs/{run_id}/artifacts", "artifacts")
            matches = [row for row in artifacts if row["name"] == policy["artifact"].format(sha=sha)
                       and not row.get("expired")
                       and row.get("workflow_run", {}).get("head_sha") == version_event["source_head"]]
            if len(matches) != 1:
                raise ValueError("current-SHA package artifact missing or expired")
            artifact = matches[0]
            if artifact.get("size_in_bytes", 0) > 512 * 1024 * 1024:
                raise ValueError("artifact download budget")
            package_audit = verify_packages(api(f"repos/{args.repository}/actions/artifacts/{artifact['id']}/zip", True),
                                            sha, source_sha, policy)
            # Compare local reviewed package source listings; gzip bytes can vary by lock/VCS staging.
            if [{key: row[key] for key in ["name", "files"]} for row in package_audit["packages"]] != [
                    {key: row[key] for key in ["name", "files"]} for row in local["packages"]]:
                raise ValueError("remote and local package listing differs")
            report["package_artifact"] = dict(package_audit, id=artifact["id"])
            if source_sha != inventory(ROOT)[1] or status != subprocess.check_output(
                    ["git", "-C", str(ROOT), "status", "--porcelain=v1", "-z"]):
                raise ValueError("source changed during remote review")
            report["publication_qualified"] = True
        report["result"] = "passed" if report["publication_qualified"] else "not_qualified"
    except (ValueError, KeyError, OSError, subprocess.SubprocessError, zipfile.BadZipFile) as error:
        report["result"] = "not_qualified"
        report["issues"].append(str(error))
    args.output.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps({"result": report["result"], "issues": report["issues"]}))
    return 0 if report["publication_qualified"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
