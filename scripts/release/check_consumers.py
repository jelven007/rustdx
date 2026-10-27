#!/usr/bin/env python3
"""Compile standalone current and intentionally incompatible 1.x consumers."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import time

ROOT = Path(__file__).resolve().parents[2]
CORE = ROOT / "crates/rustdx-core"
CASES = {
    "current": (
        True,
        """
use rustdx_complete::{
    cache::{Cache, CacheFetchError}, calendar::TradingCalendar,
    file::{day::Day, gbbq::Fq}, operation::OperationContext,
    tcp::{stock::{Client, FinanceSnapshot}, HeartbeatAck},
};
use std::{io, time::Duration};
fn finance(client: &mut Client) -> io::Result<FinanceSnapshot> { client.finance(0, "000001") }
fn heartbeat(client: &mut Client) -> io::Result<HeartbeatAck> { client.heartbeat() }
fn main() {
    let date = "2025-01-06".parse().unwrap();
    assert!(TradingCalendar::is_trading_day(&date).unwrap());
    assert!(TradingCalendar::is_trading_day(&"2027-01-01".parse().unwrap()).is_err());
    let cache = Cache::memory(Duration::from_secs(1));
    let task = OperationContext::new(Duration::from_secs(1), None).unwrap();
    let value: Result<_, CacheFetchError<io::Error>> =
        cache.get_or_fetch_checked("key", &task, || Ok(vec![1]));
    assert_eq!(value.unwrap(), vec![1]);
    let day = Day { code: "sz000001".into(), date: 20250106,
        open: 10., high: 10., low: 10., close: 10., amount: 100., vol: 100 };
    assert_eq!(Fq::new_checked(vec![day].into_iter(), &[]).unwrap()[0].factor, 1.);
    let _ = (finance, heartbeat);
}
""",
        None,
    ),
    "legacy-calendar": (
        False,
        """
use rustdx_complete::calendar::TradingCalendar;
fn main() {
    let date = "2025-01-06".parse().unwrap();
    let _: bool = TradingCalendar::is_trading_day(&date);
}
""",
        "Result<bool",
    ),
    "legacy-heartbeat": (
        False,
        """
use rustdx_complete::tcp::stock::Client;
fn legacy(client: &mut Client) -> std::io::Result<u16> { client.heartbeat() }
fn main() { let _ = legacy; }
""",
        "HeartbeatAck",
    ),
    "legacy-finance": (
        False,
        """
use rustdx_complete::tcp::stock::{Client, FinanceInfoData};
fn legacy(client: &mut Client) -> std::io::Result<FinanceInfoData> {
    client.finance(0, "000001")
}
fn main() { let _ = legacy; }
""",
        "FinanceSnapshot",
    ),
    "checked-owned": (
        True,
        """
use rustdx_complete::{
    IncrementalIndicatorError, LegacyIndicatorError,
    data::{CanonicalBar, Market, Period, Symbol, VolumeUnit},
    tcp::stock::KlineRequest,
};
fn main() {
    let incremental: rustdx_complete::indicators::IndicatorError =
        IncrementalIndicatorError::ZeroPeriod;
    assert!(matches!(rustdx_complete::indicators::try_ema(&[1.], 0),
        Err(error) if error == incremental));
    let legacy: rustdx_complete::IndicatorError =
        LegacyIndicatorError::insufficient_data("SMA".into(), 20, 1);
    assert!(matches!(rustdx_complete::Error::from(legacy),
        rustdx_complete::Error::Indicator(_)));
    let raw: Vec<u8> = vec![REAL_FRAME_BYTES];
    let owned: Vec<CanonicalBar> = {
        let symbol = Symbol::new(Market::Shenzhen, "000001").unwrap();
        let request = KlineRequest::for_symbol(&symbol, Period::Day, 0, 3).unwrap();
        request.decode_canonical_stock(&raw, VolumeUnit::Lots).unwrap()
    };
    assert_eq!(owned.len(), 3);
    let encoded = serde_json::to_vec(&owned).unwrap();
    let restored: serde_json::Value = serde_json::from_slice(&encoded).unwrap();
    for bar in restored.as_array().unwrap() {
        assert_eq!(bar["bar"]["symbol"]["market"], "shenzhen");
        assert_eq!(bar["bar"]["symbol"]["code"], "000001");
        assert_eq!(bar["period"], "Day");
        assert_eq!(bar["price_basis"], "Raw");
        assert_eq!(bar["source"], "TdxTcp");
        assert_eq!(bar["provenance"]["source_volume_unit"], "Lots");
        assert_eq!(bar["provenance"]["volume_unit_basis"], "CallerSpecified");
        assert_eq!(bar["provenance"]["request"],
            serde_json::json!({"category":9,"command":1325,"market":0,"start":0,"count":3}));
        assert_eq!(bar["provenance"]["acquisition"], serde_json::Value::Null);
    }
}
""",
        None,
    ),
}


def sha(data):
    return hashlib.sha256(data).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--cargo", default="cargo")
    parser.add_argument("--toolchain", default="1.89.0")
    parser.add_argument("--offline", action="store_true")
    parser.add_argument("--core", type=Path, default=CORE, help="source or unpacked core directory")
    args = parser.parse_args()
    core_directory = args.core.resolve()
    output = args.output.resolve()
    if output == ROOT or ROOT in output.parents:
        parser.error("output must be outside the repository")
    output.mkdir(parents=True, exist_ok=False)
    cargo_name = args.cargo
    # Do not resolve rustup's cargo symlink: argv[0] selects the cargo proxy.
    if Path(cargo_name).exists():
        cargo_name = os.path.abspath(cargo_name)
    cargo = [cargo_name, f"+{args.toolchain}"]
    metadata = json.loads(subprocess.check_output(
        cargo + ["metadata", "--no-deps", "--format-version", "1", "--locked"], cwd=ROOT
    ))
    core = next(package for package in metadata["packages"] if package["name"] == "jelven-rustdx-complete")
    cli = next(package for package in metadata["packages"] if package["name"] == "jelven-rustdx-cli")
    core_version = core["version"]
    core_requirement = next(dependency["req"] for dependency in cli["dependencies"]
                            if dependency["name"] == "jelven-rustdx-complete")
    report = {
        "schema_version": 1,
        "scope": "selected breaking signatures, not a complete API diff",
        "core_version": core_version,
        "core_manifest": str(core_directory / "Cargo.toml"),
        "core_manifest_sha256": sha((core_directory / "Cargo.toml").read_bytes()),
        "cli_version": cli["version"],
        "lock_sha256": sha((ROOT / "Cargo.lock").read_bytes()),
        "cases": [],
        "registry_verification": "not performed",
    }
    env = dict(os.environ, CARGO_TERM_COLOR="never")
    # Reuse an explicitly provided external target; otherwise isolate all artifacts.
    env.setdefault("CARGO_TARGET_DIR", str(output / "target"))
    try:
        if (not re.fullmatch(r"[0-9]\.[0-9]\.[0-9]", core_version)
                or cli["version"] != core_version or core_requirement != "^" + core_version):
            raise RuntimeError("consumer contract requires matching decimal core/CLI versions")
        report["toolchain"] = subprocess.check_output(cargo + ["--version"], env=env).decode().strip()
        for name, (success, source, diagnostic) in CASES.items():
            fixture_sha256 = None
            if name == "checked-owned":
                fixture = ROOT / "crates/rustdx-core/tests/fixtures/kline/stock-day.response.hex"
                raw = bytes.fromhex(fixture.read_text().strip())
                fixture_sha256 = sha(raw)
                source = source.replace("REAL_FRAME_BYTES", ",".join(str(byte) for byte in raw))
            directory = output / name
            (directory / "src").mkdir(parents=True)
            (directory / "Cargo.toml").write_text(
                '[package]\nname="rustdx-consumer-' + name + '"\nversion="0.0.0"\nedition="2024"\n'
                '[workspace]\n[dependencies]\nchrono="0.4"\nserde_json="1"\n'
                'rustdx_complete={package="jelven-rustdx-complete",path=' + json.dumps(str(core_directory)) + '}\n'
            )
            (directory / "src/main.rs").write_text(source.strip() + "\n")
            command = cargo + ["run" if success else "check", "--manifest-path", str(directory / "Cargo.toml")]
            if args.offline:
                command.append("--offline")
            if not success:
                command.append("--message-format=json")
            started = time.monotonic()
            result = subprocess.run(command, env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            log = output / f"{name}.log"
            log.write_bytes(result.stdout + result.stderr)
            diagnostics = []
            if not success:
                for line in result.stdout.splitlines():
                    try:
                        message = json.loads(line)
                    except (ValueError, UnicodeError):
                        continue
                    if message.get("reason") == "compiler-message" and message["message"]["level"] == "error":
                        diagnostics.append(message["message"])
            matched = (result.returncode == 0) if success else (
                result.returncode != 0 and any(
                    item.get("code", {}).get("code") == "E0308"
                    and diagnostic in item.get("rendered", "") for item in diagnostics
                )
            )
            report["cases"].append({
                "name": name, "command": command, "expected_compile_success": success,
                "exit_code": result.returncode, "expectation_met": matched,
                "elapsed_seconds": time.monotonic() - started,
                "source_sha256": sha(source.strip().encode()), "log": log.name,
                "log_sha256": sha(log.read_bytes()),
                "fixture_payload_sha256": fixture_sha256,
            })
            if not matched:
                raise RuntimeError(f"consumer expectation failed: {name}; see {log}")
        report["result"] = "passed"
    except (RuntimeError, OSError, subprocess.CalledProcessError) as error:
        report["result"] = "failed"
        report["error"] = str(error)
        raise
    finally:
        (output / "consumers.json").write_text(json.dumps(report, indent=2) + "\n")
    print(output / "consumers.json")


if __name__ == "__main__":
    main()
