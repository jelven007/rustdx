"""Bind original capture bytes and inspected third-party source without rewriting captures."""
import hashlib
import json
from pathlib import Path
import tarfile
import zipfile

root = Path("/private/tmp/rustdx-upstream-capabilities-20261027")
art = Path(__file__).resolve().parent
evidence = root / "docs/testing/evidence/upstream-capabilities-20261027"
sha = lambda data: hashlib.sha256(data).hexdigest()
archive_path = evidence / "raw-captures.zip"
with zipfile.ZipFile(archive_path) as archive:
    files = {name: sha(archive.read(name)) for name in sorted(archive.namelist())}
    for name, digest in files.items():
        assert sha((art / name).read_bytes()) == digest
sdist = evidence / "eltdx-3.3.0.tar.gz"
assert sha(sdist.read_bytes()) == "16fad43e518618728a2ac5c0a5d0abf0a3bcce3ccb12daccec55d8fa912d9a9f"
paths = [
    "eltdx-3.3.0/src/eltdx/api/quotes.py",
    "eltdx-3.3.0/src/eltdx/transport/socket.py",
    "eltdx-3.3.0/src/eltdx/transport/actor.py",
    "eltdx-3.3.0/crates/eltdx-python/src/transport.rs",
    "eltdx-3.3.0/crates/eltdx-runtime/src/engine.rs",
    "eltdx-3.3.0/crates/eltdx-runtime/src/slot.rs",
]
with tarfile.open(sdist) as archive:
    modules = {name: sha(archive.extractfile(name).read()) for name in paths}
value = dict(
    schema=1, captured_on="2026-10-27", source="live_tcp", synthetic=False,
    capture_archive_sha256=sha(archive_path.read_bytes()), capture_files=files,
    date_selector_schema=2,
    original_summary_schema=1, original_summary_has_date_schema_field=False,
    date_schema_basis="Independently check every raw request selector equals uint32(-YYYYMMDD); "
                      "original capture summaries and bytes remain unchanged.",
    capture_tool="probe-tool/main.rs", tool_cargo_manifest="probe-tool/Cargo.toml",
    tool_scope="Fixed endpoint, three symbols, bounded dates, 180s total, 4s socket, no heartbeat "
               "or automatic reconnect. Precision bytes inherited from first-stage capture.",
    precision_reference="../live-market-legacy-fix-20261027/window-hz-explicit/precision.json",
    sdist=dict(
        project="eltdx", version="3.3.0", artifact="eltdx-3.3.0.tar.gz",
        metadata_url="https://pypi.org/pypi/eltdx/3.3.0/json",
        sha256=sha(sdist.read_bytes()), inspected_modules=modules,
        finding="No additional 0547 activation step found in inspected API/native transport. "
                "slot::on_frame checks send_complete and receive boundary. "
                "Source inspection does not establish node passive-push capability.",
    ),
    official_tq_subscription_url="https://help.tdx.com.cn/quant/docs/markdown/ctx.stock.md/"
                                 "mindoc-1h1104d65vr68.html",
    official_tq_scope="Different TQ subscribe_hq API; notification needs get_report_data. "
                      "Not proof of 7709/0547 passive data.",
    official_daily_url="https://www.tdx.com.cn/article/vipdata.html",
    official_daily_provenance="../live-market-legacy-fix-20261027/official-current/provenance.json",
    official_daily_scope="Public daily history package does not promise complete minute retention.",
    cash_reference_url="https://static.cninfo.com.cn/finalpage/2026-10-24/1225621667.PDF",
    cash_expected="../../../../crates/rustdx-core/tests/fixtures/finance-cash-basis/expected.json",
    passive_push_status="not_verified",
    continuous_minute_history_status="not_verified",
    server_retention_ttl_status="not_verified",
)
destination = evidence / "source-info.json"
assert not destination.exists()
destination.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n")
print(json.dumps(dict(status="passed", capture_files=len(files),
                      archive_bytes=archive_path.stat().st_size, sdist_modules=len(modules))))
