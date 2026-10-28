#!/usr/bin/env python3
"""Calendar-driven daily queue; exact inputs and target admission are rechecked per job."""
import argparse
import contextlib
import datetime as dt
import fcntl
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "validation"))
from audit_fact_maintenance import audit_job, load
from audit_point_in_time_facts import trading_days
import production_gate

ZONE = dt.timezone(dt.timedelta(hours=8))
CONTRACT = "a-share/day/raw/cny/shares/v1"
PATHS = ("runner", "template", "source_ready", "fact_registry", "calendar_reference",
         "evidence_root", "state_directory", "metrics_output", "business_metrics_output",
         "task_matrix", "dependency_lock", "deployment_identity", "admissions_directory",
         "admission_evidence_root")


def encoded(value):
    return (json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False) + "\n").encode()


def atomic(path, body, mode=0o600):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, temporary = tempfile.mkstemp(prefix="." + path.name + "-", dir=path.parent)
    try:
        with os.fdopen(fd, "wb") as stream:
            os.fchmod(stream.fileno(), mode)
            stream.write(body)
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temporary, path)
        directory = os.open(path.parent, os.O_RDONLY)
        try:
            os.fsync(directory)
        finally:
            os.close(directory)
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)


@contextlib.contextmanager
def lock(settings, name):
    root = Path(settings["state_directory"])
    root.mkdir(parents=True, exist_ok=True, mode=0o700)
    with (root / name).open("a+b") as stream:
        fcntl.flock(stream, fcntl.LOCK_EX | fcntl.LOCK_NB)
        yield


def settings_at(path):
    settings = load(path)
    if settings.get("schema_version") != 1:
        raise ValueError("unsupported daily settings")
    for name in PATHS:
        if not isinstance(settings.get(name), str) or not Path(settings[name]).is_absolute():
            raise ValueError("absolute settings path required:" + name)
    for name, low, high in (("publication_lag_days", 0, 7), ("max_jobs_per_run", 1, 64)):
        value = settings.get(name)
        if isinstance(value, bool) or not isinstance(value, int) or not low <= value <= high:
            raise ValueError("invalid daily budget:" + name)
    if (isinstance(settings.get("audit_max_bytes"), bool) or
            not isinstance(settings.get("audit_max_bytes"), int) or settings["audit_max_bytes"] <= 0):
        raise ValueError("positive audit quota required")
    dt.date.fromisoformat(settings["initial_from"])
    clock = dt.time.fromisoformat(settings["publish_time"])
    if clock.tzinfo is not None:
        raise ValueError("publish_time must be a local UTC+8 time")
    template = load(settings["template"])
    if settings.get("task_type") != "runner_day" or template.get("contract_version") != CONTRACT:
        raise ValueError("only the implemented runner_day contract is schedulable")
    if template.get("tcp_day") and settings["publication_lag_days"] < 1:
        raise ValueError("TCP daily publication requires an ended date")
    return settings


def deadline(settings, date):
    return dt.datetime.combine(date + dt.timedelta(days=settings["publication_lag_days"]),
                               dt.time.fromisoformat(settings["publish_time"]), ZONE)


def call(settings, action, *args):
    result = subprocess.run([settings["runner"], action, *map(str, args)],
                            capture_output=True, text=True, timeout=3700)
    if result.returncode:
        raise ValueError("runner " + action + " failed: " + result.stderr[-4000:])
    return result.stdout


def catalog(settings, template):
    if not (Path(template["root"]) / "ledger.sqlite").exists():
        return {}
    value = json.loads(call(settings, "catalog", "--root", template["root"],
                            "--dataset", template["dataset_id"], "--no-tcp-startup"))
    return value["partitions"] if value else {}


def expected(settings, template, now):
    start = dt.date.fromisoformat(settings["initial_from"])
    end = now.astimezone(ZONE).date() - dt.timedelta(days=settings["publication_lag_days"])
    if end < start or (end - start).days > 3660:
        raise ValueError("daily business range must be 0..3660 days")
    calendar = load(template["calendar"])
    reference = load(settings["calendar_reference"])
    if (calendar["source"] == reference["source"] or
            (calendar["start"], calendar["end"]) != (reference["start"], reference["end"])):
        raise ValueError("independent calendar source and identical coverage required")
    lower, upper = map(dt.date.fromisoformat, (calendar["start"], calendar["end"]))
    if trading_days(calendar, lower, upper) != trading_days(reference, lower, upper):
        raise ValueError("independent calendar dates disagree")
    return trading_days(reference, start, end)


def source(settings, template, now):
    value = load(settings["source_ready"])
    today = now.astimezone(ZONE).date()
    observed, valid = (dt.datetime.fromisoformat(value[n]) for n in ("observed_at", "valid_until"))
    revision = value["source_revision"]
    as_of, complete = (dt.date.fromisoformat(value[n]) for n in ("source_as_of", "complete_through"))
    if (value.get("schema_version") != 1 or value.get("status") != "complete" or
            value.get("template_sha256") != production_gate.digest(settings["template"]) or
            isinstance(revision, bool) or not isinstance(revision, int) or revision < 1 or
            observed.tzinfo is None or valid.tzinfo is None or observed > now or valid <= observed or
            not complete <= as_of <= today or (template.get("tcp_day") and as_of != today)):
        raise ValueError("invalid or mismatched source-ready record")
    if valid <= now:
        raise ValueError("source-ready record expired")
    if value.get("revision_range"):
        first, last = map(dt.date.fromisoformat, value["revision_range"])
        if first > last or last > complete:
            raise ValueError("invalid explicit source revision range")
    return value


def queue(settings, template, dates, partitions, ready, now):
    pending = {str(date): "missing_partition" for date in dates if str(date) not in partitions}
    if ready and ready.get("revision_range"):
        first, last = map(dt.date.fromisoformat, ready["revision_range"])
        if not dates or first < dt.date.fromisoformat(settings["initial_from"]) or last > dates[-1]:
            raise ValueError("revision range outside approved business dates")
        for date in dates:
            if first <= date <= last:
                old = partitions.get(str(date))
                if old and (ready["source_revision"] < old["source_revision"] or
                            ready["source_as_of"] < old["source_as_of"]):
                    raise ValueError("revision or source cutoff regressed")
                if old and ready["source_revision"] == old["source_revision"]:
                    if ready["source_as_of"] != old["source_as_of"]:
                        raise ValueError("explicit revision must increase source_revision")
                    continue
                pending[str(date)] = "explicit_revision"
    overdue = [date for date in pending if now >= deadline(settings, dt.date.fromisoformat(date))]
    return pending, overdue


def job_at(template, date, ready, now):
    job = dict(template, **{"from": str(date), "through": str(date),
                           "planned_date": str(now.astimezone(ZONE).date()),
                           "source_revision": ready["source_revision"],
                           "source_as_of": ready["source_as_of"]})
    return job


def store_job(settings, job):
    body = encoded(job)
    digest = hashlib.sha256(body).hexdigest()
    path = Path(settings["state_directory"]) / "jobs" / (digest + ".json")
    if path.exists():
        if path.read_bytes() != body:
            raise ValueError("content-addressed job bytes differ")
    else:
        atomic(path, body)
    return path


def qualify(settings, job, now):
    reviewed = audit_job(load(settings["fact_registry"]), settings["evidence_root"],
                         now.astimezone(ZONE).date(), job, settings["calendar_reference"])
    if reviewed["job_status"] != "covered":
        raise ValueError("actual job facts lack independent review: " + ",".join(reviewed["job_issues"]))
    path = store_job(settings, job)
    plan = json.loads(call(settings, "plan", "--config", path))
    if not plan["can_start"]:
        raise ValueError("actual plan has start blockers")
    return path, plan, reviewed


def prepare(settings, now):
    template = load(settings["template"])
    dates = expected(settings, template, now)
    partitions = catalog(settings, template)
    ready, source_issue = None, None
    try:
        ready = source(settings, template, now)
    except (OSError, ValueError, KeyError, TypeError) as error:
        source_issue = str(error)
    pending, overdue = queue(settings, template, dates, partitions, ready, now)
    result = {"schema_version": 1, "prepared_at": now.isoformat(),
              "expected_dates": list(map(str, dates)), "pending": pending,
              "overdue": overdue, "source_issue": source_issue, "jobs": [], "blocked": []}
    if ready:
        eligible = [d for d in sorted(pending) if d <= ready["complete_through"]]
        for date in eligible[:settings["max_jobs_per_run"]]:
            job = job_at(template, date, ready, now)
            try:
                path, plan, review = qualify(settings, job, now)
                result["jobs"].append({"date": date, "path": str(path),
                                       "plan": plan, "maintenance_status": review["status"]})
            except (OSError, ValueError, KeyError, TypeError) as error:
                result["blocked"].append({"date": date, "issue": str(error)})
    with lock(settings, "schedule.lock"):
        atomic(Path(settings["state_directory"]) / "current.json", encoded(result))
    return result


def monitor(settings, now):
    # Recompute expected dates and pinned catalog on every invocation, never trust old current.json.
    result = prepare(settings, now)
    text = (
        f"rustdx_daily_metrics_timestamp_seconds {now.timestamp()}\n"
        f"rustdx_daily_expected_dates {len(result['expected_dates'])}\n"
        f"rustdx_daily_pending_dates {len(result['pending'])}\n"
        f"rustdx_daily_overdue_dates {len(result['overdue'])}\n"
        f"rustdx_daily_source_ready {int(result['source_issue'] is None)}\n"
        f"rustdx_daily_fact_blocked_dates {len(result['blocked'])}\n"
    )
    try:
        from audit_fact_maintenance import audit as maintenance_audit
        registry = maintenance_audit(load(settings["fact_registry"]), settings["evidence_root"],
                                    now.astimezone(ZONE).date())
        text += f"rustdx_daily_registry_attention {int(registry['status'] != 'covered')}\n"
    except (OSError, ValueError, KeyError, TypeError):
        text += "rustdx_daily_registry_attention 1\n"
    atomic(settings["metrics_output"], text.encode(), 0o644)
    if result["expected_dates"]:
        template = load(settings["template"])
        date = result["expected_dates"][-1]
        part = catalog(settings, template).get(date)
        current = {"source_revision": part["source_revision"] if part else template["source_revision"],
                   "source_as_of": part["source_as_of"] if part else template["source_as_of"]}
        try:
            current = source(settings, template, now)
        except (OSError, ValueError, KeyError, TypeError):
            pass
        job = job_at(template, date, current, now)
        path = store_job(settings, job)
        business = call(settings, "business-metrics", "--config", path, "--publish-by",
                        deadline(settings, dt.date.fromisoformat(date)).isoformat(),
                        "--audit-max-bytes", settings["audit_max_bytes"])
        atomic(settings["business_metrics_output"], business.encode(), 0o644)
    return result


def run(settings):
    denied = []
    with lock(settings, "executor.lock"):
        now = dt.datetime.now(ZONE)
        result = prepare(settings, now)
        for item in result["jobs"]:
            now = dt.datetime.now(ZONE)
            refreshed = prepare(settings, now)
            current = next((j for j in refreshed["jobs"] if j["date"] == item["date"]), None)
            if current is None or current["path"] != item["path"]:
                raise ValueError("pending job or actual source changed before execution")
            job = load(current["path"])
            path, plan, _ = qualify(settings, job, now)
            try:
                admission = production_gate.check(settings, path, plan, now)
            except (OSError, ValueError, KeyError, TypeError) as error:
                admission = {"status": "admission_denied", "issues": [str(error)]}
            atomic(Path(settings["state_directory"]) / "last-admission.json", encoded(admission))
            if admission["status"] != "evidence_contract_complete":
                denied.append({"date": item["date"], "issues": admission["issues"]})
                continue
            call(settings, "recover", "--root", job["root"], "--no-tcp-startup")
            # Recovery can consume time; source, facts and admission must still qualify.
            ready = source(settings, load(settings["template"]), dt.datetime.now(ZONE))
            if (job["source_revision"] != ready["source_revision"] or
                    job["source_as_of"] != ready["source_as_of"] or job["through"] > ready["complete_through"]):
                raise ValueError("source changed during recovery")
            path, plan, _ = qualify(settings, job, dt.datetime.now(ZONE))
            if production_gate.check(settings, path, plan, dt.datetime.now(ZONE))["status"] != "evidence_contract_complete":
                raise ValueError("admission changed during recovery")
            call(settings, "run", "--config", path)
            part = catalog(settings, job).get(job["from"])
            if (part is None or part["source_revision"] < job["source_revision"] or
                    part["source_as_of"] < job["source_as_of"]):
                raise ValueError("completed run did not publish the requested complete date")
    result = monitor(settings, dt.datetime.now(ZONE))
    result["admission_denied"] = denied
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=("prepare", "run", "monitor"))
    parser.add_argument("--settings", type=Path, required=True)
    args = parser.parse_args()
    settings = settings_at(args.settings)
    result = run(settings) if args.action == "run" else globals()[args.action](settings, dt.datetime.now(ZONE))
    print(json.dumps(result, ensure_ascii=False))
    return 1 if result["blocked"] or result["source_issue"] or result.get("admission_denied") else 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, ValueError, KeyError, TypeError, subprocess.SubprocessError) as error:
        print(json.dumps({"status": "blocked", "error": str(error)}), file=sys.stderr)
        raise SystemExit(1)
