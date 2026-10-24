db = connect(process.env.RUSTDX_RECOVERY_MONGO_URI).getSiblingDB("rustdx_recovery_3291b0df13ac4945bff839756d5e6b9a");

const row=db.bars.findOne(); const before=db.bars.countDocuments({});
const invalid={...row,contract_version:"invalid"}; delete invalid._id;
try { db.bars.insertOne(invalid); throw Error("validator accepted invalid contract"); }
catch(e) { if(e.code!==121) throw e; }
const duplicate={...row}; delete duplicate._id;
try { db.bars.insertOne(duplicate); throw Error("unique index accepted duplicate"); }
catch(e) { if(e.code!==11000) throw e; }
if(db.bars.countDocuments({})!==before) throw Error("rejection changed row count");
print(JSON.stringify({validator_error:121,unique_error:11000,rows:before}));

