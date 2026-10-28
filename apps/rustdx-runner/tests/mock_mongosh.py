#!/usr/bin/python3
"""Local pipe oracle only; this does not emulate or qualify a real MongoDB deployment."""
import fcntl
import json
import os
from pathlib import Path
import sys

root = Path(os.environ["RUSTDX_DB_URI"])
config = json.loads((root / "oracle.json").read_bytes())
process_root = Path(config.get("process_root", str(root)))
rows = config["rows"]
suffix = ("contract_version", "date", "market", "code", "instrument_class")


def mark(delta):
    with (process_root / "processes.lock").open("a+b") as stream:
        fcntl.flock(stream, fcntl.LOCK_EX)
        path = process_root / "processes.json"
        state = json.loads(path.read_bytes()) if path.exists() else {"pids": [], "peak": 0}
        alive = []
        for pid in state["pids"]:
            try:
                os.kill(pid, 0)
                alive.append(pid)
            except ProcessLookupError:
                pass
        state["pids"] = alive + [os.getpid()] if delta > 0 else [p for p in alive if p != os.getpid()]
        state["peak"] = max(state["peak"], len(state["pids"]))
        path.write_text(json.dumps(state))


mark(1)
try:
    for line in sys.stdin:
        request = json.loads(line)
        q = request["payload"]
        assert request["is_write"] is False
        selected = [r for r in rows if r["release_id"] == q["release"]]
        reply = {"query_id": "wrong" if config.get("bad_query_id") else request["query_id"], "ok": True}
        if q["operation"] == "count":
            reply["count"] = len(selected) + int(config.get("extra_count", False))
        else:
            assert q["operation"] == "fetch"
            if q["date"]:
                selected = [r for r in selected if r["date"] == q["date"]]
            selected.sort(key=lambda r: tuple(r[n] for n in suffix))
            if q["after"]:
                after = tuple(q["after"][n] for n in suffix)
                selected = [r for r in selected if tuple(r[n] for n in suffix) > after]
            reply["rows"] = selected[:q["count"]]
        print(json.dumps(reply), flush=True)
finally:
    mark(-1)
