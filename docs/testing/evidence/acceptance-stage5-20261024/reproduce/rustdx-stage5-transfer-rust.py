import concurrent.futures
import hashlib
import json
import pathlib
import re
import subprocess

stage = pathlib.Path("/tmp/rustdx-stage5")
out = stage / "linux-rust"
out.mkdir(exist_ok=True)
manifest = (stage / "channel-linux.toml").read_text()
records = []
for name in ["rustc", "rust-std", "cargo", "clippy-preview", "rustfmt-preview"]:
    section = re.search(
        r"\[pkg\." + re.escape(name) + r"\.target\.x86_64-unknown-linux-gnu\]\n(.*?)(?=\n\[)",
        manifest,
        re.S,
    ).group(1)
    records.append({
        "package": name,
        "url": re.search(r'^xz_url = "(.*?)"', section, re.M).group(1),
        "sha256": re.search(r'^xz_hash = "(.*?)"', section, re.M).group(1),
    })


def transfer(record):
    name = record["package"]
    dest = out / (name + ".tar.xz")
    with (out / (name + ".log")).open("w") as log:
        subprocess.run(
            ["curl", "--fail", "--location", "--retry", "1", "--max-time", "240",
             record["url"], "-o", str(dest)],
            stdout=log, stderr=subprocess.STDOUT, check=True, timeout=500,
        )
    assert hashlib.sha256(dest.read_bytes()).hexdigest() == record["sha256"], name
    record["bytes"] = dest.stat().st_size
    subprocess.run(
        ["scp", "-o", "BatchMode=yes", "-o", "ConnectTimeout=8", str(dest),
         "trae-ecs:/tmp/rustdx-acceptance-20261024-TLDCn5/rust-local/" + dest.name],
        check=True, timeout=300,
    )
    print(name + " checked and transferred", flush=True)


subprocess.run(
    ["ssh", "trae-ecs", "mkdir -p /tmp/rustdx-acceptance-20261024-TLDCn5/rust-local"],
    check=True,
)
with concurrent.futures.ThreadPoolExecutor(max_workers=5) as pool:
    list(pool.map(transfer, records))
(out / "packages.json").write_text(json.dumps(records, indent=2) + "\n")
subprocess.run(
    ["scp", str(out / "packages.json"),
     "trae-ecs:/tmp/rustdx-acceptance-20261024-TLDCn5/rust-local/packages.json"],
    check=True,
)
remote = """
import hashlib,json,pathlib,subprocess
task=pathlib.Path('/tmp/rustdx-acceptance-20261024-TLDCn5')
out=task/'rust-local'; prefix=task/'toolchain';prefix.mkdir(exist_ok=True)
records=json.loads((out/'packages.json').read_text())
for record in records:
 p=out/(record['package']+'.tar.xz')
 assert hashlib.sha256(p.read_bytes()).hexdigest()==record['sha256']
 subprocess.run(['tar','-xJf',str(p),'-C',str(out)],check=True)
 name=subprocess.check_output(['tar','-tJf',str(p)]).decode().splitlines()[0].split('/')[0]
 with (out/(record['package']+'.install.log')).open('w') as log:
  subprocess.run([str(out/name/'install.sh'),'--prefix='+str(prefix),'--disable-ldconfig'],stdout=log,stderr=subprocess.STDOUT,check=True)
(task/'rust-installed.json').write_text(json.dumps({'version':'1.89.0','official_distribution_digest_checked':True,'packages':records,'transport':'local HTTPS then SSH; remote SHA rechecked'},indent=2)+'\\n')
"""
subprocess.run(["ssh", "trae-ecs", "python3 -"], input=remote, text=True, check=True, timeout=180)
print("official Rust1.89 installation complete", flush=True)
