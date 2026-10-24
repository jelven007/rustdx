db = connect(process.env.RUSTDX_RECOVERY_MONGO_URI).getSiblingDB("rustdx_recovery_c460890aa1f94602bc3495fcbd7c77e2");

const rows=db.bars.find().sort({dataset_id:1,release_id:1,contract_version:1,date:1,
  market:1,code:1,instrument_class:1}).toArray();
print(EJSON.stringify({rows,options:db.getCollectionInfos({name:"bars"})[0].options,
  indexes:db.bars.getIndexes()}, {relaxed:false}));

