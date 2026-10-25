#!/usr/bin/env python3
"""Local source/package preflight. No registry upload, Git write or version change."""
import argparse
import gzip
import hashlib
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile
import time

from source_scope import EXCLUDED_PREFIXES, SCOPE_VERSION, inventory as engineering_inventory

ROOT = Path(__file__).resolve().parents[2]
PACKAGES = {
    "jelven-rustdx-complete": "crates/rustdx-core",
    "jelven-rustdx-cli": "apps/rustdx-cli",
}


def sha(data):
    return hashlib.sha256(data).hexdigest()


def git(*args):
    return subprocess.check_output(["git", "-C", str(ROOT), *args])


def inventory():
    return engineering_inventory(ROOT)


def source_archive(records, output):
    # Fixed order, ownership, modes, times and gzip header; covers ignored tracked files.
    with output.open("wb") as file, gzip.GzipFile(filename="", mode="wb", fileobj=file, mtime=0) as zipped:
        with tarfile.open(fileobj=zipped, mode="w|", format=tarfile.PAX_FORMAT) as archive:
            for record in records:
                path = ROOT / record["path"]
                info = tarfile.TarInfo(record["path"])
                info.mode = 0o755 if record["executable"] else 0o644
                if record["kind"] == "symlink":
                    info.type = tarfile.SYMTYPE
                    info.linkname = os.readlink(path)
                    archive.addfile(info)
                else:
                    data = path.read_bytes()
                    if sha(data) != record["sha256"]:
                        raise RuntimeError("source changed while creating archive")
                    info.size = len(data)
                    archive.addfile(info, io.BytesIO(data))


def run(command, output, label, results, env, cwd=ROOT):
    started = time.monotonic()
    log = output / f"{label}.log"
    with log.open("wb") as file:
        result = subprocess.run(command, cwd=cwd, env=env, stdout=file, stderr=subprocess.STDOUT)
    results.append({"command": command, "exit_code": result.returncode, "seconds": time.monotonic() - started,
                    "log": log.name, "log_sha256": sha(log.read_bytes())})
    if result.returncode:
        raise RuntimeError(f"{label} failed; see {log}")


def extract_package(crate, destination):
    # Only ordinary files in the single package prefix are needed for verification.
    with tarfile.open(crate) as archive:
        members = archive.getmembers()
        prefixes = {member.name.split("/")[0] for member in members}
        if len(prefixes) != 1:
            raise RuntimeError("invalid package prefix")
        prefix = next(iter(prefixes))
        for member in members:
            relative = Path(member.name)
            if relative.is_absolute() or ".." in relative.parts or not member.isfile():
                raise RuntimeError(f"unexpected package entry: {member.name}")
            target = destination / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(archive.extractfile(member).read())
    return destination / prefix


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", required=True, type=Path, help="new directory outside the repository")
    parser.add_argument("--snapshot", action="store_true", help="allow dirty source for review; never qualifies publication")
    parser.add_argument("--offline", action="store_true")
    parser.add_argument("--cargo", default="cargo")
    parser.add_argument("--toolchain", default="1.89.0")
    parser.add_argument("--build-cache", type=Path, help="optional external Cargo target cache; reviewed packages are copied into output")
    parser.add_argument("--skip-workspace-checks", action="store_true", help="package-only audit; not a release gate")
    args = parser.parse_args()
    output = args.output.resolve()
    if output == ROOT or ROOT in output.parents:
        parser.error("output must be outside the repository")
    output.mkdir(parents=True, exist_ok=False)
    status = git("status", "--porcelain=v1", "-z")
    if status and not args.snapshot:
        parser.error("formal release requires a clean working tree; use --snapshot for a review artifact")
    records, fingerprint = inventory()
    report = {
        "schema_version": 2, "head": git("rev-parse", "HEAD").decode().strip(),
        "source_scope_version": SCOPE_VERSION, "excluded_prefixes": EXCLUDED_PREFIXES,
        "historical_evidence_verification": "separate evidence_manifest.py; not performed by package preflight",
        "dirty": bool(status), "git_status_hex": status.hex(), "source_sha256": fingerprint,
        "lock_sha256": sha((ROOT / "Cargo.lock").read_bytes()), "files": records, "checks": [],
        "publication_qualified": False, "registry_verification": "not performed",
        "binary_reproducibility": "not claimed",
    }
    build_target = args.build_cache.resolve() if args.build_cache else output / "target"
    if build_target == ROOT or ROOT in build_target.parents:
        parser.error("build cache must be outside the repository")
    report["build_target"] = str(build_target)
    env = dict(os.environ, RUSTDX_SKIP_INTEGRATION_TESTS="1", CARGO_TARGET_DIR=str(build_target))
    cargo = [args.cargo, f"+{args.toolchain}"]
    locked = ["--locked"] + (["--offline"] if args.offline else [])
    try:
        report["cargo"] = subprocess.check_output(cargo + ["--version"], env=env).decode().strip()
        source_archive(records, output / "source.tar.gz")
        report["source_archive_sha256"] = sha((output / "source.tar.gz").read_bytes())
        if not args.skip_workspace_checks:
            for label, command in [
                ("fmt", cargo + ["fmt", "--all", "--", "--check"]),
                ("clippy", cargo + ["clippy", "--workspace", "--all-targets", "--all-features"] + locked + ["--", "-D", "warnings"]),
                ("tests", cargo + ["test", "--workspace", "--all-targets", "--all-features"] + locked),
                ("doctests", cargo + ["test", "--workspace", "--doc", "--all-features"] + locked),
                ("strict-docs", cargo + ["doc", "--workspace", "--all-features", "--no-deps"] + locked),
                ("python", [sys.executable, "-m", "unittest", "discover", "-s", "scripts/validation/tests", "-v"]),
                ("documentation-navigation", [sys.executable, "scripts/validation/check_documentation.py"]),
                ("release-binaries", cargo + ["build", "--release", "-p", "jelven-rustdx-cli", "-p", "rustdx-runner"] + locked),
            ]:
                check_env = dict(env, RUSTDOCFLAGS="-D warnings") if label == "strict-docs" else env
                run(command, output, label, report["checks"], check_env)
            report["binaries"] = [
                {"name": name, "path": f"target/release/{name}",
                 "sha256": sha((build_target / "release" / name).read_bytes()),
                 "bytes": (build_target / "release" / name).stat().st_size,
                 "platform": sys.platform}
                for name in ["rustdx", "rustdx-runner"]
            ]
            if args.build_cache:
                (output / "target/release").mkdir(parents=True)
                for name in ["rustdx", "rustdx-runner"]:
                    shutil.copy2(build_target / "release" / name, output / "target/release" / name)
        extracted = {}
        package_reports = []
        for package, relative in PACKAGES.items():
            local_core = extracted.get("jelven-rustdx-complete")
            package_patch = [] if local_core is None else [
                "--config", f"patch.crates-io.jelven-rustdx-complete.path={json.dumps(str(local_core))}",
            ]
            package_root = ROOT
            if local_core is not None:
                # Cargo's normalized registry dependency needs a patch in offline
                # audits. Regenerate only a staging workspace lock, never source.
                package_root = output / "package-workspace"
                for record in records:
                    name = record["path"]
                    if name in {"Cargo.toml", "Cargo.lock"} or name.startswith(("crates/", "apps/", "tests/integration/")):
                        if record["kind"] != "file":
                            raise RuntimeError(f"package staging refuses symlink: {name}")
                        destination = package_root / name
                        destination.parent.mkdir(parents=True, exist_ok=True)
                        destination.write_bytes((ROOT / name).read_bytes())
                unlocked = ["--offline"] if args.offline else []
                run(cargo + ["generate-lockfile"] + unlocked + package_patch, output,
                    "package-staging-lock", report["checks"], env, cwd=package_root)
                report["staging_lock_sha256"] = sha((package_root / "Cargo.lock").read_bytes())
            run(cargo + ["package", "-p", package, "--allow-dirty", "--no-verify"] + locked + package_patch,
                output, f"package-{package}", report["checks"], env, cwd=package_root)
            crates = list((build_target / "package").glob(f"{package}-*.crate"))
            if len(crates) != 1:
                raise RuntimeError(f"expected one package for {package}")
            crate = crates[0]
            if args.build_cache:
                destination = output / "target/package" / crate.name
                destination.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(crate, destination)
                crate = destination
            directory = extract_package(crate, output / "unpacked")
            # Cargo.toml is normalized; every production source and embedded resource must match.
            listing = sorted(p.relative_to(directory).as_posix() for p in directory.rglob("*") if p.is_file())
            source_paths = [ROOT / record["path"] for record in records
                            if record["path"].startswith(relative + "/src/")]
            for source in source_paths:
                packaged = directory / source.relative_to(ROOT / relative)
                if not packaged.is_file() or sha(packaged.read_bytes()) != sha(source.read_bytes()):
                    raise RuntimeError(f"package missing or changed source: {source}")
            license_file = "LICENSE" if package == "jelven-rustdx-complete" else "LICENSE-MIT"
            for required in ["Cargo.toml", "Cargo.toml.orig", "README.md", license_file]:
                if required not in listing:
                    raise RuntimeError(f"package missing {required}")
                original = ROOT / relative / required
                if required != "Cargo.toml" and required != "Cargo.toml.orig" and original.read_bytes() != (directory / required).read_bytes():
                    raise RuntimeError(f"package changed {required}")
            extracted[package] = directory
            package_reports.append({"name": package, "crate": str(crate.relative_to(output)),
                                    "sha256": sha(crate.read_bytes()), "files": listing})
        report["packages"] = package_reports
        # Verify a downstream package with no source workspace inheritance.
        downstream = output / "downstream"
        (downstream / "src").mkdir(parents=True)
        core = extracted["jelven-rustdx-complete"]
        cli = extracted["jelven-rustdx-cli"]
        quoted_core = json.dumps(str(core))
        (downstream / "Cargo.toml").write_text(
            '[package]\nname="rustdx-package-consumer"\nversion="0.0.0"\nedition="2024"\n'
            '[workspace]\n[dependencies]\nrustdx_complete={package="jelven-rustdx-complete",path=' + quoted_core + '}\n')
        (downstream / "src/main.rs").write_text(
            'use rustdx_complete::{indicators::{EmaState,MacdState,MacdParameters},'
            'tcp::{ip::ProbePolicy,stock::{BarRangeOptions,BatchStreamOptions}}};\n'
            'fn main(){let mut e=EmaState::new(3).unwrap(); assert_eq!(e.update(Some(10.0)),Some(10.0));'
            'let _=MacdState::new(MacdParameters::default()).unwrap();'
            'let _=(BarRangeOptions::default(),BatchStreamOptions::default(),ProbePolicy::default());}\n')
        # Standalone lockfiles are generated in the review artifact, never in source.
        unlocked = ["--offline"] if args.offline else []
        for label, manifest in [("downstream", downstream / "Cargo.toml"), ("unpacked-cli", cli / "Cargo.toml")]:
            patch = [] if label == "downstream" else ["--config", f"patch.crates-io.jelven-rustdx-complete.path={quoted_core}"]
            run(cargo + ["generate-lockfile", "--manifest-path", str(manifest)] + unlocked + patch,
                output, f"{label}-lock", report["checks"], env)
            command = "run" if label == "downstream" else "check"
            run(cargo + [command, "--manifest-path", str(manifest)] + locked + patch,
                output, label, report["checks"], env)
        consumer_command = [
            sys.executable, str(ROOT / "scripts/release/check_consumers.py"),
            "--output", str(output / "signature-consumers"), "--core", str(core),
            "--cargo", args.cargo, "--toolchain", args.toolchain,
        ] + (["--offline"] if args.offline else [])
        run(consumer_command, output, "signature-consumers", report["checks"], env)
        _, final_fingerprint = inventory()
        if fingerprint != final_fingerprint or status != git("status", "--porcelain=v1", "-z"):
            raise RuntimeError("source or Git status changed during preflight")
        report["local_package_verification"] = "passed; CLI uses extracted core patch, not registry core"
        report["local_gate_passed"] = not status and not args.skip_workspace_checks
        # Remote current-SHA checks are verified separately immediately before upload.
        report["publication_qualified"] = False
        report["result"] = "passed"
    except (RuntimeError, OSError, subprocess.CalledProcessError) as error:
        report["result"] = "failed"
        report["error"] = str(error)
        raise
    finally:
        (output / "preflight.json").write_text(json.dumps(report, indent=2, ensure_ascii=False) + "\n")
    print(output / "preflight.json")


if __name__ == "__main__":
    main()
