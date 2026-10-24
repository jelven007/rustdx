import hashlib, json, os, pathlib, subprocess, time, urllib.request
task=pathlib.Path("/tmp/rustdx-acceptance-20261024-TLDCn5")
source=task/"source";source.mkdir(exist_ok=True)
logs=task/"logs";evidence=task/"evidence";evidence.mkdir(exist_ok=True)
env=dict(os.environ,CARGO_HOME=str(task/"cargo"),RUSTUP_HOME=str(task/"rustup"),
 RUSTDX_SKIP_INTEGRATION_TESTS="1",RUSTDX_DB_ACCEPTANCE="1",CARGO_BUILD_JOBS="8",
 CLICKHOUSE_URL="http://127.0.0.1:18123",MONGODB_URI="mongodb://127.0.0.1:27018",
 RUSTDX_TEST_CLI=str(source/"target/debug/rustdx"),RUSTDX_TEST_CH_URL="http://127.0.0.1:18123",
 RUSTDX_TEST_CH_USER="default",RUSTDX_TEST_CH_PASSWORD="",RUSTDX_TEST_MONGO_URI="mongodb://127.0.0.1:27018")
results=[]
def run(name,args,timeout=900,cwd=None,extra=None,required=True):
 begin=time.monotonic()
 with (logs/(name+".log")).open("w") as log:
  r=subprocess.run(args,cwd=source if cwd is None else cwd,env=env if extra is None else dict(env,**extra),stdout=log,stderr=subprocess.STDOUT,timeout=timeout)
 results.append(dict(name=name,command=args,exit_code=r.returncode,seconds=time.monotonic()-begin))
 (evidence/"results.json").write_text(json.dumps(results,indent=2)+"\n")
 if required and r.returncode:raise RuntimeError(name+" failed")
 return r.returncode
subprocess.run(["tar","-xzf",str(task/"source.tar.gz"),"-C",str(source)],check=True)
(task/"cargo").mkdir(exist_ok=True)
subprocess.run(["tar","-xzf",str(task/"registry.tar.gz"),"-C",str(task/"cargo")],check=True)
manifest=json.loads((task/"source-manifest.json").read_text())
for f in manifest["files"]:
 p=source/f["path"]
 if hashlib.sha256(p.read_bytes()).hexdigest()!=f["sha256"]:raise RuntimeError("transferred source hash mismatch")
(evidence/"source-verified.json").write_text(json.dumps({"source_sha256":manifest["source_sha256"],"files":len(manifest["files"])})+"\n")
tools=task/"bin";tools.mkdir(exist_ok=True)
for name in ["mongo-tools.tgz","mongosh.tgz"]:
 subprocess.run(["tar","-xzf",str(task/"downloads"/name),"-C",str(tools)],check=True)
tool_dir=tools/"mongodb-database-tools-ubuntu2404-x86_64-100.12.2/bin"
shell=tools/"mongosh-2.5.10-linux-x64/bin/mongosh"
env.update(PATH=str(task/"toolchain/bin")+":"+str(tool_dir)+":"+str(shell.parent)+":"+env["PATH"],RUSTDX_TEST_MONGOSH=str(shell))
docker=[str(task/"infra/docker/docker"),"-H","unix://"+str(task/"docker.sock")]
mongo="docker.m.daocloud.io/library/mongo:8.0.16@sha256:4b58ebcb1dc7a7b4e84cd8ce9098d48764ae4478876898ff9551acf2ac4a6a6d"
run("pull-mongodb",docker+["pull",mongo],timeout=600)
ch="docker.m.daocloud.io/clickhouse/clickhouse-server:25.8.33.6-alpine@sha256:87e0a5b72f5465b18eacca7c76850e7ff551c9795c50e451f5646299e5e24146"
config=(source/"deploy/acceptance/clickhouse.xml").read_text().replace("</clickhouse>",
 '<http_port>18123</http_port><tcp_port>19000</tcp_port><listen_host replace="replace">127.0.0.1</listen_host></clickhouse>')
(task/"acceptance.xml").write_text(config)
containers=["rustdx-stage5-ch","rustdx-stage5-mongo"]
try:
 run("start-clickhouse",docker+["run","-d","--name",containers[0],"--network","host","--memory","2g","--cpus","2","--ulimit","nofile=262144:262144",
  "-e","CLICKHOUSE_SKIP_USER_SETUP=1","-v",str(task/"acceptance.xml")+":/etc/clickhouse-server/config.d/acceptance.xml:ro",ch])
 run("start-mongodb",docker+["run","-d","--name",containers[1],"--network","host","--memory","1g","--cpus","2",mongo,
  "mongod","--bind_ip","127.0.0.1","--port","27018","--wiredTigerCacheSizeGB","0.25"])
 for _ in range(150):
  try:
   with urllib.request.urlopen(urllib.request.Request(env["CLICKHOUSE_URL"],b"SELECT 1"),timeout=1) as r:assert r.read().strip()==b"1"
   if subprocess.call([str(shell),"--nodb","--quiet","--norc","--eval","quit(connect(process.env.RUSTDX_TEST_MONGO_URI).adminCommand({ping:1}).ok?0:1)"],env=env,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)==0:break
  except Exception:pass
  time.sleep(.2)
 else:raise RuntimeError("database readiness deadline")
 with (evidence/"containers.json").open("w") as f:subprocess.run(docker+["inspect",*containers],stdout=f,check=True)
 until=time.monotonic()+900
 while not (task/"rust-installed.json").exists():
  if time.monotonic()>until:raise RuntimeError("official Rust installation deadline")
  time.sleep(.2)
 for name,args in [("rust",[str(task/"toolchain/bin/rustc"),"--version"]),("mongoimport",[str(tool_dir/"mongoimport"),"--version"]),("mongosh",[str(shell),"--version"])]:
  run("version-"+name,args)
 cargo=[str(task/"toolchain/bin/cargo")]
 if run("build-offline",cargo+["build","--workspace","--all-targets","--all-features","--offline","--locked"],timeout=1200,required=False):
  run("fetch-locked",cargo+["fetch","--locked"],timeout=600)
  run("build",cargo+["build","--workspace","--all-targets","--all-features","--offline","--locked"],timeout=1200)
 run("fmt",cargo+["fmt","--all","--","--check"])
 run("clippy",cargo+["clippy","--workspace","--all-targets","--all-features","--offline","--locked","--","-D","warnings"],timeout=900)
 run("workspace-tests",cargo+["test","--workspace","--all-targets","--all-features","--offline","--locked"],timeout=900)
 run("doc-tests",cargo+["test","--workspace","--all-features","--doc","--offline","--locked"])
 run("strict-doc",cargo+["doc","--workspace","--all-features","--no-deps","--offline","--locked"],extra={"RUSTDOCFLAGS":"-D warnings"})
 sql=(source/"deploy/runner/clickhouse.sql").read_text()
 sql="\n".join(l for l in sql.splitlines() if not l.strip().startswith("--"))
 for statement in sql.split(";"):
  if not statement.strip():continue
  with urllib.request.urlopen(urllib.request.Request(env["CLICKHOUSE_URL"],statement.encode()),timeout=30) as r:assert not r.read()
 run("provision-mongo",[str(shell),"--nodb","--quiet","--norc","--eval","const db=connect(process.env.RUSTDX_TEST_MONGO_URI);load('deploy/runner/mongodb.js');"])
 run("runner-all",cargo+["test","-p","rustdx-runner","--all-features","--offline","--locked","--","--include-ignored","--nocapture","--test-threads=1"],timeout=900)
 run("bulk",["python3","scripts/bench/measure_database_readback.py","--binary","target/debug/rustdx","--rows","65536","--repeat","2","--output",str(evidence/"bulk.json")],timeout=1200)
 run("faults",["python3","scripts/validation/verify_database_faults.py","--binary","target/debug/rustdx","--output",str(evidence/"faults.json")],timeout=600)
 run("real-history",["python3","scripts/validation/verify_real_day_database.py","--binary","target/debug/rustdx","--zip",str(task/"official-subset.zip"),"--output",str(evidence/"real-history.json")],timeout=1800)
 run("systemd-smoke",["systemd-run","--wait","--pipe","--collect","--unit=rustdx-stage5-smoke","--property=RuntimeMaxSec=30","--property=NoNewPrivileges=yes","--property=MemoryMax=134217728",str(source/"target/debug/rustdx"),"--no-tcp-startup","show","--version"])
 (evidence/"completed.json").write_text('{"complete":true,"full_market":false,"production_deployment":false}\n')
finally:
 for name in containers:
  with (logs/(name+".log")).open("w") as f:subprocess.run(docker+["logs",name],stdout=f,stderr=subprocess.STDOUT)
  subprocess.run(docker+["stop","--time","10",name],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
  subprocess.run(docker+["rm",name],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
 (evidence/"cleanup.json").write_text('{"owned_containers_removed":true}\n')
