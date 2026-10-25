// Offline JavaScript protocol regression. This is not database service evidence.
const assert = require("node:assert/strict");
const fs = require("node:fs");
const vm = require("node:vm");
const script = fs.readFileSync(`${__dirname}/../src/database_mongo.js`, "utf8");

async function run(requests) {
  const records = [
    ["v1","2026-07-06","sh","600000","a_share"],
    ["v1","2026-07-06","sh","600001","a_share"],
    ["v1","2026-07-06","sz","000001","a_share"],
    ["v1","2026-07-07","sh","600000","a_share"],
    ["v2","2026-07-06","sh","600000","a_share"],
  ].map(([contract_version,date,market,code,instrument_class]) => ({
    dataset_id:"d", release_id:"r", contract_version,date,market,code,instrument_class
  }));
  const matches = (row, filter) => Object.entries(filter).every(([field,value]) => {
    if (field === "$or") return value.some(term => matches(row, term));
    return typeof value === "object" ? row[field] > value.$gt : row[field] === value;
  });
  let connections = 0;
  const replies = [];
  const collection = {
    async getIndexes() { return [{
      unique:true,
      key:{dataset_id:1,release_id:1,contract_version:1,date:1,market:1,code:1,instrument_class:1}
    }]; },
    async countDocuments(filter, options) {
      assert.equal(options.collation.locale, "simple");
      return records.filter(row => matches(row, filter)).length;
    },
    async insertMany(rows, options) {
      assert.equal(options.ordered, true);
      assert.equal(options.writeConcern.w, "majority");
      return {acknowledged:true, insertedIds:Object.fromEntries(rows.map((_,i) => [i,i]))};
    },
    find(filter, projection) {
      assert.equal(projection._id, 0);
      let count;
      return {
        collation(value) { assert.equal(value.locale,"simple"); return this; },
        sort(value) {
          assert.equal(JSON.stringify(value),
            '{"contract_version":1,"date":1,"market":1,"code":1,"instrument_class":1}');
          return this;
        },
        limit(n) { count=n; return this; },
        maxTimeMS(n) { assert.equal(n,30000); return this; },
        async toArray() { return records.filter(row => matches(row, filter)).slice(0,count); }
      };
    }
  };
  await vm.runInNewContext(script, {
    process:{env:{RUSTDX_DB_URI:"mongodb://test-only"},stdin:{}},
    require(name) {
      assert.equal(name,"node:readline");
      return {createInterface:() => (async function*() {
        for (const request of requests) yield JSON.stringify(request);
      })()};
    },
    async connect(uri) {
      assert.equal(uri,"mongodb://test-only"); connections++;
      return {getSiblingDB:() => ({
        async getCollectionInfos() { return [{options:{collation:{locale:"simple"}}}]; },
        getCollection:() => collection
      })};
    },
    EJSON:{stringify:JSON.stringify},
    print:text => replies.push(JSON.parse(text))
  });
  assert.equal(connections, 1);
  return replies;
}

(async () => {
  let id = 0;
  const request = payload => ({
    query_id:`r-t-${++id}`,is_write:payload.operation==="insert",
    payload:{database:"test",collection:"bars",dataset:"d",release:"r",...payload}
  });
  const replies = await run([
    request({operation:"preflight"}),
    request({operation:"insert",rows:[{},{}]}),
    request({operation:"fetch",after:null,date:null,count:2}),
    request({operation:"fetch",after:{
      contract_version:"v1",date:"2026-07-06",market:"sh",code:"600001",instrument_class:"a_share"
    },date:null,count:512}),
    request({operation:"fetch",after:null,date:"2026-07-07",count:512}),
    request({operation:"count"}),
  ]);
  assert.equal(replies.length,6);
  replies.forEach((reply,index) => {
    assert.equal(reply.ok,true);
    assert.equal(reply.query_id,`r-t-${index+1}`);
  });
  assert.equal(replies[1].inserted,2);
  assert.deepEqual(replies[2].rows.map(row => row.code),["600000","600001"]);
  assert.deepEqual(replies[3].rows.map(row => [row.contract_version,row.date,row.market,row.code]),[
    ["v1","2026-07-06","sz","000001"],
    ["v1","2026-07-07","sh","600000"],
    ["v2","2026-07-06","sh","600000"],
  ]);
  assert.deepEqual(replies[4].rows.map(row => row.date),["2026-07-07"]);
  assert.equal(replies[5].count,5);
  await assert.rejects(run([request({operation:"fetch",after:null,date:null,count:513})]));
  console.log("MongoDB protocol: persistent connection, key order, date filter, identity and limits passed");
})().catch(error => { console.error(error); process.exitCode=1; });
