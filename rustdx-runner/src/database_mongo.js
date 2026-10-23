// Credentials stay in the process environment, never argv or archived requests.
const fs = require("fs");
const q = JSON.parse(fs.readFileSync(process.env.RUSTDX_DB_REQUEST, "utf8"));
const connection = connect(process.env.RUSTDX_DB_URI);
const d = connection.getSiblingDB(q.database);
const c = d.getCollection(q.collection);
const filter = {dataset_id:q.dataset, release_id:q.release};
if (q.operation === "insert") {
  const key = {dataset_id:1, release_id:1, contract_version:1, date:1, market:1, code:1, instrument_class:1};
  const info = d.getCollectionInfos({name:q.collection})[0];
  if (!info || (info.options?.collation && info.options.collation.locale !== "simple"))
    throw Error("provision a collection with simple collation");
  const valid = c.getIndexes().some(i => i.unique === true && !i.sparse &&
    !i.partialFilterExpression && !i.expireAfterSeconds &&
    (!i.collation || i.collation.locale === "simple") &&
    JSON.stringify(i.key) === JSON.stringify(key));
  if (!valid) throw Error("provision the full versioned unique index");
  const result = c.insertMany(q.rows, {ordered:true, writeConcern:{w:"majority", j:true, wtimeout:30000}});
  print(JSON.stringify({ok:result.acknowledged === true, inserted:Object.keys(result.insertedIds).length}));
} else if (q.operation === "count") {
  print(JSON.stringify({ok:true,count:c.countDocuments(filter, {maxTimeMS:30000})}));
} else if (q.operation === "fetch") {
  const rows = c.find(filter, {_id:0}).sort({date:1,market:1,code:1,contract_version:1,instrument_class:1})
    .skip(q.offset).limit(q.count).maxTimeMS(30000).toArray();
  print(JSON.stringify({ok:true,rows}));
} else {
  throw Error("unsupported operation");
}
