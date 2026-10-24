#!/usr/bin/env python3
"""Back up and restore an explicit root, then compare every field of a pinned CSV."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import time


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runner", type=Path, required=True)
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--dataset", required=True)
    parser.add_argument("--version", type=int, required=True)
    parser.add_argument("--from", dest="start", required=True)
    parser.add_argument("--through", required=True)
    parser.add_argument("--output-directory", type=Path, required=True)
    parser.add_argument("--max-bytes", type=int, default=10 * 1024**3)
    parser.add_argument("--max-rows", type=int, default=10_000_000)
    args = parser.parse_args()
    root, out = args.root.resolve(), args.output_directory.resolve()
    if args.version < 1 or args.max_bytes < 1 or args.max_rows < 1:
        parser.error("positive fixed version and budgets required")
    if out.exists() or root == out or root in out.parents:
        parser.error("use a new output directory outside the live root")
    out.mkdir(mode=0o700)
    result = {"schema_version": 1, "status": "running", "root": str(root),
              "dataset": args.dataset, "catalog_version": args.version,
              "scope": "Consistent local control/archive backup only. No native remote database restore or target production RPO/RTO claim.",
              "commands": []}

    def run(action, *flags):
        started = time.monotonic()
        command = [str(args.runner.resolve()), "--no-tcp-startup", action, *map(str, flags)]
        completed = subprocess.run(command, capture_output=True, timeout=3600)
        index = len(result["commands"])
        (out / f"{index:02d}.stdout").write_bytes(completed.stdout)
        (out / f"{index:02d}.stderr").write_bytes(completed.stderr)
        result["commands"].append({"argv": command, "exit_code": completed.returncode,
                                   "seconds": time.monotonic() - started})
        if completed.returncode:
            raise ValueError(f"{action} failed; see {out / f'{index:02d}.stderr'}")
        return completed.stdout

    flags = ["--dataset", args.dataset, "--version", args.version, "--from", args.start,
             "--through", args.through, "--max-bytes", args.max_bytes, "--max-rows", args.max_rows]
    try:
        before = json.loads(run("export", "--root", root, *flags, "--output", out / "before.csv"))
        run("backup", "--root", root, "--destination", out / "backup", "--max-bytes", args.max_bytes)
        recovery_start = time.monotonic()
        run("restore", "--root", out / "backup", "--destination", out / "restored",
            "--max-bytes", args.max_bytes)
        after = json.loads(run("export", "--root", out / "restored", *flags, "--output", out / "after.csv"))
        if before != after or before["catalog_version"] != args.version:
            raise ValueError("pinned export version/count differs after restore")
        hashes = {}
        for name in ["before.csv", "after.csv"]:
            hasher = hashlib.sha256()
            with (out / name).open("rb") as stream:
                for chunk in iter(lambda: stream.read(65536), b""):
                    hasher.update(chunk)
            hashes[name] = hasher.hexdigest()
        if len(set(hashes.values())) != 1:
            raise ValueError("pinned CSV keys/fields differ after restore")
        result.update(status="passed", rows=before["rows"], csv_sha256=hashes,
                      recovery_to_verified_seconds=time.monotonic() - recovery_start)
    except Exception as error:
        result.update(status="failed", error=str(error))
        raise
    finally:
        (out / "result.json").write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps({"status": "passed", "catalog_version": args.version, "rows": result["rows"]}))


if __name__ == "__main__":
    main()
