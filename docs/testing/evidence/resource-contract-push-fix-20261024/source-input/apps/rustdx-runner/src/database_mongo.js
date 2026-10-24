// Credentials stay in the process environment, never argv or archived requests.
// One connection, one request at a time; Rust verifies EOF and process exit.
(async () => {
  const lines = require("node:readline").createInterface({
    input: process.stdin, crlfDelay: Infinity
  });
  let connection;
  const fields = ["contract_version", "date", "market", "code", "instrument_class"];
  for await (const line of lines) {
    const request = JSON.parse(line);
    if (typeof request.query_id !== "string" || request.query_id.length > 300)
      throw Error("invalid request identity");
    const q = request.payload;
    connection ??= await connect(process.env.RUSTDX_DB_URI);
    const d = connection.getSiblingDB(q.database);
    const c = d.getCollection(q.collection);
    const filter = {dataset_id:q.dataset, release_id:q.release};
    let reply;
    if (q.operation === "preflight") {
      const key = {dataset_id:1, release_id:1, contract_version:1, date:1, market:1, code:1, instrument_class:1};
      const info = (await d.getCollectionInfos({name:q.collection}))[0];
      if (!info || (info.options?.collation && info.options.collation.locale !== "simple"))
        throw Error("provision a collection with simple collation");
      const valid = (await c.getIndexes()).some(i => i.unique === true && !i.sparse &&
        !i.partialFilterExpression && i.expireAfterSeconds === undefined &&
        (!i.collation || i.collation.locale === "simple") &&
        JSON.stringify(i.key) === JSON.stringify(key));
      if (!valid) throw Error("provision the full versioned unique index");
      reply = {ok:true};
    } else if (q.operation === "insert") {
      const result = await c.insertMany(q.rows, {
        ordered:true, writeConcern:{w:"majority", j:true, wtimeout:30000}
      });
      reply = {ok:result.acknowledged === true, inserted:Object.keys(result.insertedIds).length};
    } else if (q.operation === "count") {
      reply = {ok:true,count:await c.countDocuments(filter, {
        maxTimeMS:30000, collation:{locale:"simple"}
      })};
    } else if (q.operation === "fetch") {
      if (!Number.isInteger(q.count) || q.count < 1 || q.count > 512)
        throw Error("invalid batch limit");
      if (q.date !== null) filter.date = q.date;
      if (q.after !== null) {
        // Lexicographic comparison follows the complete unique index suffix.
        filter.$or = fields.map((field, index) => {
          const term = {};
          for (const prefix of fields.slice(0, index)) term[prefix] = q.after[prefix];
          term[field] = {$gt:q.after[field]};
          return term;
        });
      }
      const rows = await c.find(filter, {_id:0}).collation({locale:"simple"})
        .sort({contract_version:1,date:1,market:1,code:1,instrument_class:1})
        .limit(q.count).maxTimeMS(30000).toArray();
      reply = {ok:true,rows};
    } else {
      throw Error("unsupported operation");
    }
    print(EJSON.stringify({query_id:request.query_id, ...reply}, {relaxed:true}));
  }
})();
