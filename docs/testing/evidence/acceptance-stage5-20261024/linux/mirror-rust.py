import concurrent.futures
import hashlib
import json
import pathlib
import subprocess
import tomllib

task = pathlib.Path("/tmp/rustdx-acceptance-20261024-TLDCn5")
out = task / "rust-mirror"
out.mkdir(exist_ok=True)
manifest = task / "rust-download/channel.toml"
expected = (task / "rust-download/channel.sha256").read_text().split()[0]
assert hashlib.sha256(manifest.read_bytes()).hexdigest() == expected
data = tomllib.loads(manifest.read_text())


def package(name):
    spec = data["pkg"][name]["target"]["x86_64-unknown-linux-gnu"]
    dest = out / (name + ".tar.xz")
    mirror = spec["xz_url"].replace(
        "https://static.rust-lang.org", "https://lf9-static.rsproxy.cn/obj/rsproxy"
    )
    if not dest.exists() or hashlib.sha256(dest.read_bytes()).hexdigest() != spec["xz_hash"]:
        if dest.exists():
            (out / (name + ".rejected.json")).write_text(json.dumps({
                "bytes": dest.stat().st_size,
                "actual_sha256": hashlib.sha256(dest.read_bytes()).hexdigest(),
                "expected_sha256": spec["xz_hash"],
                "reason": "overlapping abandoned downloader produced invalid partial file; never installed",
            }) + "\n")
            dest.unlink()
        with (out / (name + ".final.log")).open("w") as log:
            subprocess.run(
                ["curl", "--fail", "--location", "--max-time", "240", mirror, "-o", str(dest)],
                stdout=log, stderr=subprocess.STDOUT, check=True, timeout=250,
            )
    assert hashlib.sha256(dest.read_bytes()).hexdigest() == spec["xz_hash"]
    return {"package": name, "url": spec["xz_url"], "mirror_url": mirror,
            "sha256": spec["xz_hash"], "bytes": dest.stat().st_size}


with concurrent.futures.ThreadPoolExecutor(max_workers=5) as pool:
    records = list(pool.map(package, ["rustc", "rust-std", "cargo", "clippy-preview", "rustfmt-preview"]))
prefix = task / "toolchain"
prefix.mkdir(exist_ok=True)
for record in records:
    dest = out / (record["package"] + ".tar.xz")
    subprocess.run(["tar", "-xJf", str(dest), "-C", str(out)], check=True)
    name = subprocess.check_output(["tar", "-tJf", str(dest)]).decode().splitlines()[0].split("/")[0]
    with (out / (record["package"] + ".install.log")).open("w") as log:
        subprocess.run(
            [str(out / name / "install.sh"), "--prefix=" + str(prefix), "--disable-ldconfig"],
            stdout=log, stderr=subprocess.STDOUT, check=True,
        )
(task / "rust-installed.json").write_text(json.dumps({
    "version": "1.89.0", "official_distribution_digest_checked": True,
    "official_channel_sha256": expected, "packages": records,
    "transport": "rsproxy mirror; each artifact matched verified official distribution manifest",
}, indent=2) + "\n")
print("official-digest Rust1.89 installation complete", flush=True)
