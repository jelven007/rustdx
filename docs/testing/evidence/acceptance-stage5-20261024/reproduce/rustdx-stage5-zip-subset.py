import hashlib,json,pathlib,subprocess,zipfile
stage=pathlib.Path("/tmp/rustdx-stage5")
original=pathlib.Path("/private/tmp/rustdx-next-services/hsjday-20261022.zip")
hasher=hashlib.sha256()
with original.open("rb") as f:
 for b in iter(lambda:f.read(1048576),b""):hasher.update(b)
assert hasher.hexdigest()=="07aab2f1166a794cebfa380cda058417499c4f74049a5fe36fe029f5c803d400"
records=[]
with zipfile.ZipFile(original) as src,zipfile.ZipFile(stage/"official-subset.zip","w",compression=zipfile.ZIP_DEFLATED) as dst:
 names=sorted(n for n in src.namelist() if n.replace("\\","/").split("/")[-1].startswith("sh600") and n.endswith(".day"))[:128]
 for name in names:
  data=src.read(name);dst.writestr(name,data)
  records.append(dict(member=name,sha256=hashlib.sha256(data).hexdigest(),bytes=len(data),rows=len(data)//32))
manifest={"original_url":"https://data.tdx.com.cn/vipdoc/hsjday.zip",
 "original_capture_date":"2026-10-22","original_sha256":hasher.hexdigest(),
 "derivative":"ZIP container rebuilt with byte-identical first128 sorted sh600 members; original archive not modified",
 "subset_sha256":hashlib.sha256((stage/"official-subset.zip").read_bytes()).hexdigest(),"members":records}
(stage/"official-subset-provenance.json").write_text(json.dumps(manifest,indent=2)+"\n")
for name in ["official-subset.zip","official-subset-provenance.json"]:
 subprocess.run(["scp","-o","BatchMode=yes","-o","ConnectTimeout=8",str(stage/name),"trae-ecs:/tmp/rustdx-acceptance-20261024-TLDCn5/"+name],check=True)
print(sum(x["rows"] for x in records),flush=True)
