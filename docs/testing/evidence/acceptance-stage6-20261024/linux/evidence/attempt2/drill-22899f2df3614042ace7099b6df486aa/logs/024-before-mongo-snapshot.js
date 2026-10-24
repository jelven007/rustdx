db = connect(process.env.RUSTDX_RECOVERY_MONGO_URI).getSiblingDB("rustdx_recovery_3291b0df13ac4945bff839756d5e6b9a");

const rows=db.bars.find().sort({dataset_id:1,release_id:1,contract_version:1,date:1,
  market:1,code:1,instrument_class:1}).toArray();
print(EJSON.stringify({rows,options:db.getCollectionInfos({name:"bars"})[0].options,
  indexes:db.bars.getIndexes()}, {relaxed:false}));

