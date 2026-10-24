import csv, hashlib, json, pathlib, struct, subprocess, time, zipfile
root=pathlib.Path("/Users/bytedance/Documents/rustdx")
stage=pathlib.Path("/tmp/rustdx-stage5")
snapshot=pathlib.Path("/private/tmp/rustdx-next-services/hsjday-20261022.zip")
codes=["sh600000","sh600519","sz000001"]
reference={}
with zipfile.ZipFile(snapshot) as archive:
 for code in codes:
  member=next(n for n in archive.namelist() if n.replace("\\","/").split("/")[-1]==code+".day")
  content=archive.read(member)
  (stage/(code+".day")).write_bytes(content)
  rows={}
  for date,o,h,l,c,a,v,reserved in struct.iter_unpack("<5If2I",content):
   if 20261019 <= date <= 20261022:
    rows[str(date)]={"date":str(date),"code":code,"open":o/100,"high":h/100,"low":l/100,"close":c/100,"amount":a,"volume_shares":v}
  reference[code]={"member":member,"sha256":hashlib.sha256(content).hexdigest(),"rows":rows}
material={"scope":"052d category9 lots comparison against preserved official local day shares, three A shares, 20261019..22; not authority for all instruments",
 "zip_source_url":"https://data.tdx.com.cn/vipdoc/hsjday.zip","zip_capture":"2026-10-22",
 "zip_sha256":"07aab2f1166a794cebfa380cda058417499c4f74049a5fe36fe029f5c803d400","references":reference}
basis=json.dumps(material,sort_keys=True,separators=(",",":")).encode()
hash_=hashlib.sha256(basis).hexdigest()
results=[]
for index,endpoint in enumerate(["115.238.56.198:7709","180.153.18.170:7709"]):
 candidate=stage/("tcp-live-"+str(index));candidate.mkdir()
 (candidate/"raw").mkdir();(candidate/"facts").mkdir()
 (candidate/"facts"/hash_).write_bytes(basis)
 request={"schema_version":1,"source_contract":"a-share/tcp-052d/day/raw/cny/lots-100-shares/v1",
 "source":{"endpoint":endpoint,"volume_evidence_sha256":hash_,"page_size":800,"max_pages":4,"max_raw_rows":3200,"max_payload_bytes":4*1024*1024,"max_wire_records":64},
 "from":"2026-10-19","through":"2026-10-22","acquired_on":"2026-10-24","codes":codes,
 "max_archive_bytes":8*1024*1024,"max_csv_bytes":1024*1024,"max_rows":100}
 (candidate/"tcp-request.json").write_text(json.dumps(request)+"\n")
 command=[str(root/"target/debug/rustdx"),"--no-tcp-startup","--network-dir",str(stage/"permits"),"--timeout","180","tcp-day","--directory",str(candidate)]
 start=time.monotonic()
 with (candidate/"cli.log").open("w") as log:
  result=subprocess.run(command,cwd=root,stdout=log,stderr=subprocess.STDOUT,timeout=200)
 results.append({"endpoint":endpoint,"exit_code":result.returncode,"seconds":time.monotonic()-start,"candidate":str(candidate)})
 (stage/"tcp-live-results.json").write_text(json.dumps(results,indent=2)+"\n")
 print(results[-1],flush=True)
