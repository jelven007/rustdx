db = connect(process.env.RUSTDX_RECOVERY_MONGO_URI).getSiblingDB("rustdx_recovery_3291b0df13ac4945bff839756d5e6b9a");
// Run using a DBA account; configure URI outside argv in production.
const d = db.getSiblingDB("rustdx_recovery_3291b0df13ac4945bff839756d5e6b9a");
if (!d.getCollectionInfos({name:"bars"}).length) {
  d.createCollection("bars", {
    collation:{locale:"simple"},
    validator:{$jsonSchema:{
      bsonType:"object",
      required:["dataset_id","release_id","contract_version","date","market","code","instrument_class",
        "open","high","low","close","amount_cny","volume_shares"],
      additionalProperties:false,
      properties:{
        _id:{bsonType:"objectId"},
        dataset_id:{bsonType:"string"},release_id:{bsonType:"string"},
        contract_version:{enum:["a-share/day/raw/cny/shares/v1"]},
        date:{bsonType:"string",pattern:"^[0-9]{4}-[0-9]{2}-[0-9]{2}$"},
        market:{enum:["sh","sz"]},code:{bsonType:"string",pattern:"^[0-9]{6}$"},
        instrument_class:{enum:["a_share"]},
        open:{bsonType:["double","int","long"]},high:{bsonType:["double","int","long"]},
        low:{bsonType:["double","int","long"]},close:{bsonType:["double","int","long"]},
        amount_cny:{bsonType:["double","int","long"]},volume_shares:{bsonType:["double","int","long"]}
      }
    }}
  });
}
d.bars.createIndex(
  {dataset_id:1,release_id:1,contract_version:1,date:1,market:1,code:1,instrument_class:1},
  {unique:true,name:"rustdx_full_release_key",collation:{locale:"simple"}}
);
// Publisher: find+insert; consumer: find. No update/remove/drop/createIndex grants.
// Provision validation/indexes before the runner starts; do not configure TTL.

