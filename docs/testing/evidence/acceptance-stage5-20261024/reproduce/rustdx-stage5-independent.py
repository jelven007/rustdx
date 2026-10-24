import csv, datetime, hashlib, json, pathlib, struct, zlib
base=pathlib.Path("/tmp/rustdx-stage5")
def require(x,msg):
 if not x:raise ValueError(msg)
def decode(payload,count):
 n=struct.unpack_from("<H",payload)[0];require(n<=count,"page too large")
 p=2;price=0;rows=[]
 def vi():
  nonlocal p
  first=byte=payload[p];p+=1;v=first&63;shift=6
  while byte&128:
   byte=payload[p];p+=1;v+=(byte&127)<<shift;shift+=7
   require(shift<=34,"overlong price")
  return -v if first&64 else v
 for _ in range(n):
  day=struct.unpack_from("<I",payload,p)[0];p+=4
  a,b,c,d=[vi() for _ in range(4)];price+=a
  rawvol,rawamount=struct.unpack_from("<II",payload,p);p+=8
  # Independently expand the vendor volume bit fields for the actual observed
  # high-exponent normal-volume profile; reject other encodings.
  exponent=rawvol>>24;hi=(rawvol>>16)&255;mid=(rawvol>>8)&255;lo=rawvol&255
  require(exponent>=67,"unvalidated low-exponent volume profile")
  unit=2.0**(exponent*2-127)
  lots=unit + (unit*(64+(hi&127))/64 if hi>128 else unit*hi/128)
  lots+=unit*mid/32768*(2 if hi&128 else 1)+unit*lo/8388608*(2 if hi&128 else 1)
  amount=float(struct.unpack("<f",struct.pack("<I",rawamount))[0])
  rows.append(dict(date=str(day),open=price/1000,close=(price+b)/1000,
   high=(price+c)/1000,low=(price+d)/1000,volume_shares=lots*100,amount=amount,
   raw_volume_u32=rawvol))
  price+=b
 require(p==len(payload),"trailing body")
 return sorted(rows,key=lambda r:r["date"])
results=[]
for index in [0,1]:
 candidate=base/("tcp-live-"+str(index));request=json.loads((candidate/"tcp-request.json").read_text())
 csvrows={(r["code"],r["date"].replace("-","")):r for r in csv.DictReader((candidate/"collected.csv").open())}
 endpoints=[]
 for code in request["codes"]:
  root=candidate/"raw"/code;inventory=json.loads((root/"wire/inventory.json").read_text())
  receipt=json.loads((root/"receipt.json").read_text());pending=None;responses=0;decoded=[];files=[]
  for entry in inventory["entries"]:
   data=(root/"wire"/entry["file"]).read_bytes()
   require(len(data)==entry["bytes"] and hashlib.sha256(data).hexdigest()==entry["sha256"],"inventory hash")
   record=json.loads(data)
   require(record["endpoint"]==request["source"]["endpoint"],"endpoint")
   if record["phase"]=="request":
    require(pending is None,"unpaired request");pending=bytes(record["request"]);continue
   require(record["phase"]=="response" and pending==bytes(record["request"]),"response pair")
   header=bytes(record["header"]);body=bytes(record["body"]);req=pending;pending=None;responses+=1
   require(header[:4]==bytes.fromhex("b1cb7400") and header[5:9]==req[1:5] and header[10:12]==req[10:12],"response identity")
   compressed,expanded=struct.unpack_from("<HH",header,12);require(len(body)==compressed,"wire length")
   payload=body if compressed==expanded else zlib.decompress(body);require(len(payload)==expanded,"inflate length")
   if req[10:12]==b"\x0d\x00":continue
   require(req[10:12]==b"\x2d\x05" and req[14:20].decode()==code[2:] and struct.unpack_from("<H",req,20)[0]==9,"daily request")
   decoded.extend(decode(payload,struct.unpack_from("<H",req,26)[0]))
  require(pending is None and responses==2,"complete single page")
  selected=[r for r in decoded if "20261019"<=r["date"]<="20261022"];require(len(selected)==4,"scope dates")
  dayrows={str(r[0]):r for r in struct.iter_unpack("<5If2I",(base/(code+".day")).read_bytes())}
  comparison=[]
  for r in selected:
   csvrow=csvrows[(code,r["date"])]
   for field in ["open","high","low","close","amount"]:
    require(float(csvrow[field])==r[field],"CSV mismatch "+field)
   require(float(csvrow["vol"])==r["volume_shares"],"CSV share mismatch")
   raw=dayrows[r["date"]]
   exact=all(r[field]==raw[j]/100 for field,j in [("open",1),("high",2),("low",3),("close",4)]) and r["amount"]==raw[5]
   require(exact,"official day price or amount differs")
   diff=r["volume_shares"]-raw[6]
   comparison.append({"date":r["date"],"ohlc_amount_exact":True,"tcp_shares":r["volume_shares"],"day_shares":raw[6],"share_difference":diff,"shares_exact":diff==0})
  endpoints.append({"code":code,"frames":responses,"raw_rows":len(decoded),"selected_rows":len(selected),"comparisons":comparison,"receipt_rows_exact":len(receipt["rows"])==len(selected)})
 results.append({"endpoint":request["source"]["endpoint"],"securities":endpoints})
require((base/"tcp-live-0/collected.csv").read_bytes()==(base/"tcp-live-1/collected.csv").read_bytes(),"two nodes disagree")
report={"results":results,"two_nodes_csv_exact":True,"native_csv_fields_match_independent_wire_decoder":True,
 "official_zip_ohlc_amount_exact":True,"official_zip_volume_exact":False,
 "limitation":"Observed category9 lots lose integer-share precision versus u32 day files (up to 94 shares); no exact-volume promotion or all-instrument unit claim."}
(base/"tcp-independent.json").write_text(json.dumps(report,indent=2)+"\n")
print(json.dumps({k:v for k,v in report.items() if k!="results"},indent=2))
