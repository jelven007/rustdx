import json, os, pathlib, socket, subprocess, time, urllib.request
root=pathlib.Path("/Users/bytedance/Documents/rustdx")
task=pathlib.Path("/tmp/rustdx-stage5/mac");task.mkdir(exist_ok=True)
for n in ["ch-data","ch-tmp","ch-user","mongo-data"]:(task/n).mkdir(exist_ok=True)
original=pathlib.Path("/private/tmp/rustdx-next-services")
tools=original/"mongodb-database-tools-macos-arm64-100.12.2/bin"
shell=original/"mongosh-2.5.10-darwin-arm64/bin/mongosh"
config=(original/"clickhouse-config.xml").read_text().replace("/tmp/rustdx-next-services",str(task)).replace("18123","28123").replace("19000","29000")
(task/"clickhouse-config.xml").write_text(config)
for port in [28123,29000,28018]:
 s=socket.socket();s.settimeout(.2);r=s.connect_ex(("127.0.0.1",port));s.close()
 if r==0:raise RuntimeError("test port occupied")
processes=[]
def start(name,args):
 with (task/(name+"-startup.log")).open("w") as log:
  p=subprocess.Popen(args,stdout=log,stderr=subprocess.STDOUT,start_new_session=True)
 processes.append(p);return p
env=dict(os.environ,CARGO_HOME=str(root/".cargo"),RUSTUP_HOME=str(root/".cargo/rustup"),
 PATH=str(tools)+":"+str(shell.parent)+":"+os.environ["PATH"],
 RUSTDX_SKIP_INTEGRATION_TESTS="1",RUSTDX_DB_ACCEPTANCE="1",
 CLICKHOUSE_URL="http://127.0.0.1:28123",MONGODB_URI="mongodb://127.0.0.1:28018",
 RUSTDX_TEST_CLI=str(root/"target/debug/rustdx"),
 RUSTDX_TEST_CH_URL="http://127.0.0.1:28123",RUSTDX_TEST_CH_USER="default",RUSTDX_TEST_CH_PASSWORD="",
 RUSTDX_TEST_MONGO_URI="mongodb://127.0.0.1:28018",RUSTDX_TEST_MONGOSH=str(shell))
results=[]
def run(name,args,timeout=900):
 begin=time.monotonic()
 with (task/(name+".log")).open("w") as f:r=subprocess.run(args,cwd=root,env=env,stdout=f,stderr=subprocess.STDOUT,timeout=timeout)
 results.append(dict(name=name,command=args,exit_code=r.returncode,seconds=time.monotonic()-begin))
 (task/"results.json").write_text(json.dumps(results,indent=2)+"\n")
 if r.returncode:raise RuntimeError(name+" failed")
try:
 start("clickhouse",[str(original/"clickhouse"),"server","--config-file",str(task/"clickhouse-config.xml")])
 start("mongodb",[str(original/"mongodb-macos-aarch64--8.0.16/bin/mongod"),"--dbpath",str(task/"mongo-data"),"--bind_ip","127.0.0.1","--port","28018","--wiredTigerCacheSizeGB","0.25","--logpath",str(task/"mongod.log")])
 for _ in range(100):
  try:
   with urllib.request.urlopen(urllib.request.Request(env["CLICKHOUSE_URL"],b"SELECT 1"),timeout=1) as r:assert r.read().strip()==b"1"
   s=socket.create_connection(("127.0.0.1",28018),timeout=.2);s.close();break
  except Exception:time.sleep(.2)
 else:raise RuntimeError("local server readiness failed")
 sql=(root/"deploy/runner/clickhouse.sql").read_text()
 sql="\n".join(l for l in sql.splitlines() if not l.strip().startswith("--"))
 for statement in sql.split(";"):
  if not statement.strip():continue
  with urllib.request.urlopen(urllib.request.Request(env["CLICKHOUSE_URL"],statement.encode()),timeout=30) as r:
   text=r.read().decode()
   if text:raise RuntimeError(text)
 script="const db=connect(process.env.RUSTDX_TEST_MONGO_URI);load("+json.dumps(str(root/"deploy/runner/mongodb.js"))+");"
 run("provision-mongo",[str(shell),"--nodb","--quiet","--norc","--eval",script])
 c=[str(root/".cargo/bin/cargo"),"+1.89.0"]
 run("small-table",c+["test","-p","jelven-rustdx-cli","--test","database_acceptance","--offline","--locked","--","--nocapture"])
 run("real-runner",c+["test","-p","rustdx-runner","--test","control_plane","real_databases_isolate_versions","--offline","--locked","--","--include-ignored","--nocapture"],timeout=600)
finally:
 for p in processes:
  p.terminate()
 for p in processes:
  try:p.wait(timeout=20)
  except subprocess.TimeoutExpired:p.kill();p.wait()
 (task/"cleanup.json").write_text(json.dumps({"owned_services_stopped":all(p.poll() is not None for p in processes),"pids":[p.pid for p in processes]})+"\n")
