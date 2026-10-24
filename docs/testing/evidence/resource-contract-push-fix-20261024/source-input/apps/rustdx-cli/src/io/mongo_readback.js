// One request at a time, one bounded response. Rust closes stdin after the last
// verified batch and checks EOF plus exit status before accepting the import.
(async () => {
  const db = await connect(process.env.RUSTDX_MONGODB_URI);
  const lines = require("node:readline").createInterface({
    input: process.stdin,
    crlfDelay: Infinity
  });
  for await (const line of lines) {
    const request = JSON.parse(line);
    if (!Array.isArray(request.keys) || request.keys.length < 1 || request.keys.length > 16384)
      throw Error("invalid readback key count");
    const projection = {_id: 0};
    for (const field of request.fields) projection[field] = 1;
    const query = {$or: request.keys.map(([date, code]) => ({date, code}))};
    const rows = await db.getSiblingDB(request.database).getCollection(request.collection)
      .find(query, projection).collation({locale: "simple"})
      .limit(request.keys.length + 1).maxTimeMS(300000).toArray();
    print(EJSON.stringify({id: request.id, rows}, {relaxed: true}));
  }
})();
