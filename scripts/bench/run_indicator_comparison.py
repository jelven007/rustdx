#!/usr/bin/env python3
"""Snapshot two indicator implementations; build and sample alternately on one host."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import statistics
import struct
import subprocess
import sys
import time
import uuid

ROOT = Path(__file__).resolve().parents[2]
HARNESS = Path(__file__).with_name("indicator_harness.rs")
BUILD = {"profile": "release", "opt_level": 3, "lto": False,
         "codegen_units": 1, "incremental": False, "edition": "2024"}


def sha(data):
    return hashlib.sha256(data).hexdigest()


def git(*args):
    return subprocess.check_output(["git", "-C", str(ROOT), *args])


def hardware():
    if sys.platform == "darwin":
        def sysctl(key):
            return subprocess.check_output(["sysctl", "-n", key]).decode().strip()
        return {"cpu": sysctl("machdep.cpu.brand_string"), "memory_bytes": sysctl("hw.memsize"),
                "logical_cpus": sysctl("hw.logicalcpu"), "model": sysctl("hw.model")}
    if sys.platform.startswith("linux"):
        cpu = Path("/proc/cpuinfo").read_text()
        models = sorted({line.split(":", 1)[1].strip() for line in cpu.splitlines()
                         if line.startswith(("model name", "Hardware"))})
        memory = next(line for line in Path("/proc/meminfo").read_text().splitlines()
                      if line.startswith("MemTotal:"))
        return {"cpu": models, "memory": memory, "logical_cpus": os.cpu_count()}
    raise ValueError("same-host performance gate supports macOS/Linux only")


def sources(reference=None):
    if reference is None:
        base = ROOT / "crates/rustdx-core/src"
        files = [base / "indicators.rs", *sorted((base / "indicators").rglob("*.rs"))]
        return {p.relative_to(base).as_posix(): p.read_bytes() for p in files}
    names = git("ls-tree", "-r", "--name-only", reference).decode().splitlines()
    base = next((p for p in ["crates/rustdx-core/src/", "src/"]
                 if p + "indicators.rs" in names), None)
    if base is None:
        raise ValueError("baseline has no supported indicator source layout")
    return {p[len(base):]: git("show", reference + ":" + p) for p in names
            if p == base + "indicators.rs" or (p.startswith(base + "indicators/") and p.endswith(".rs"))}


def source_identity(files):
    return [{"path": p, "sha256": sha(data)} for p, data in sorted(files.items())]


def run(command, cwd, env, log):
    started = time.monotonic()
    result = subprocess.run(command, cwd=cwd, env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    log.write_bytes(result.stdout + result.stderr)
    if result.returncode:
        raise RuntimeError(f"command failed ({result.returncode}); see {log}")
    return result.stdout, time.monotonic() - started


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline-ref", required=True)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--cargo", default="cargo")
    parser.add_argument("--toolchain", default="1.89.0")
    parser.add_argument("--samples", type=int, default=7)
    parser.add_argument("--max-ratio", type=float, default=1.5)
    parser.add_argument("--offline", action="store_true")
    args = parser.parse_args()
    if not 5 <= args.samples <= 31 or args.samples % 2 == 0:
        parser.error("samples must be odd and in [5,31]")
    output = args.output.resolve()
    if output == ROOT or ROOT in output.parents:
        parser.error("output must be a new directory outside the source workspace")
    output.mkdir(parents=True, exist_ok=False)
    ref = git("rev-parse", "--verify", args.baseline_ref + "^{commit}").decode().strip()
    snapshots = {"baseline": sources(ref), "current": sources()}
    harness = HARNESS.read_bytes()
    # Integer arithmetic and exact binary fractions; archive actual input bytes.
    input_path = output / "input.f64le"
    input_path.write_bytes(b"".join(
        struct.pack("<ddd", (10240 + (i * 37 % 2048)) / 1024,
                    (11264 + (i * 37 % 2048)) / 1024,
                    (9216 + (i * 37 % 2048)) / 1024) for i in range(100_000)))
    cargo_name = os.path.abspath(args.cargo) if Path(args.cargo).exists() else args.cargo
    cargo = [cargo_name, "+" + args.toolchain]
    env = dict(os.environ, CARGO_INCREMENTAL="0", CARGO_TERM_COLOR="never")
    # User flags would defeat the declared fixed build profile.
    for key in ["RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS", "RUSTC_WRAPPER", "RUSTC_WORKSPACE_WRAPPER"]:
        env.pop(key, None)
    for key in list(env):
        if key.startswith("CARGO_PROFILE_RELEASE_"):
            env.pop(key)
    rustc = str(Path(cargo_name).with_name("rustc")) if Path(cargo_name).exists() else "rustc"
    toolchain = subprocess.check_output([rustc, "+" + args.toolchain, "-vV"], env=env).decode()
    identity = {"os": platform.system(), "arch": platform.machine(), "os_version": platform.release(),
                "hardware": hardware(), "rustc": toolchain,
                "cargo": subprocess.check_output(cargo + ["--version"], env=env).decode().strip(),
                "build": BUILD, "input_sha256": sha(input_path.read_bytes()),
                "harness_sha256": sha(harness), "samples": args.samples,
                "session": str(uuid.uuid4()), "sampling": "alternating AB/BA; warmup each measurement"}
    reports = {}
    unlocked = ["--offline"] if args.offline else []
    for name, files in snapshots.items():
        directory = output / name
        (directory / "src").mkdir(parents=True)
        (directory / "src/main.rs").write_bytes(harness)
        for relative, data in files.items():
            path = directory / "src" / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(data)
        (directory / "Cargo.toml").write_text(
            '[package]\nname="rustdx-indicator-comparison"\nversion="0.0.0"\nedition="2024"\n'
            '[workspace]\n[dependencies]\nthiserror="2"\nserde_json="1"\n'
            '[profile.release]\nopt-level=3\nlto=false\ncodegen-units=1\nincremental=false\n')
        if name == "baseline":
            run(cargo + ["generate-lockfile"] + unlocked, directory, env, output / "lock.log")
        else:
            (directory / "Cargo.lock").write_bytes((output / "baseline/Cargo.lock").read_bytes())
        build_env = dict(env, CARGO_TARGET_DIR=str(directory / "target"))
        _, elapsed = run(cargo + ["build", "--release", "--locked"] + unlocked, directory,
                         build_env, output / f"{name}-build.log")
        binary = directory / "target/release/rustdx-indicator-comparison"
        reports[name] = {"schema_version": 2, "statistic": "median", "synthetic": True,
                         "environment": identity, "source_ref": ref if name == "baseline" else "working-tree",
                         "sources": source_identity(files), "binary_sha256": sha(binary.read_bytes()),
                         "lock_sha256": sha((directory / "Cargo.lock").read_bytes()),
                         "build_seconds": elapsed, "measurements": [], "runs": []}
    timings = {"baseline": {}, "current": {}}
    for index in range(args.samples):
        for name in (["baseline", "current"] if index % 2 == 0 else ["current", "baseline"]):
            binary = output / name / "target/release/rustdx-indicator-comparison"
            raw, elapsed = run([str(binary), str(input_path)], output, env, output / f"{index}-{name}.log")
            entries = json.loads(raw)
            reports[name]["runs"].append({"round": index, "elapsed_seconds": elapsed,
                                         "log": f"{index}-{name}.log", "sha256": sha(raw)})
            for entry in entries:
                key = (entry["name"], entry["rows"], entry["period"])
                record = timings[name].setdefault(key, {"samples_ns": [], "result_hash": entry["result_hash"]})
                if record["result_hash"] != entry["result_hash"]:
                    raise ValueError("indicator output changed between samples")
                record["samples_ns"].append(entry["elapsed_ns"])
    for name in reports:
        reports[name]["measurements"] = [
            {"name": key[0], "rows": key[1], "period": key[2], **record,
             "median_ns": statistics.median(record["samples_ns"])}
            for key, record in sorted(timings[name].items())]
        (output / f"{name}.json").write_text(json.dumps(reports[name], indent=2) + "\n")
    if source_identity(sources()) != source_identity(snapshots["current"]) or HARNESS.read_bytes() != harness:
        raise RuntimeError("current source or harness changed while sampling")
    command = [sys.executable, str(Path(__file__).with_name("compare_indicators.py")),
               "--baseline", str(output / "baseline.json"), "--current", str(output / "current.json"),
               "--output", str(output / "comparison.json"), "--max-ratio", str(args.max_ratio)]
    result = subprocess.run(command, env=env)
    print(output / "comparison.json")
    raise SystemExit(result.returncode)


if __name__ == "__main__":
    main()
