db = connect(process.env.RUSTDX_RECOVERY_MONGO_URI).getSiblingDB("rustdx_recovery_c460890aa1f94602bc3495fcbd7c77e2");

if(db.getCollectionInfos().length) throw Error("restore target is nonempty");
print(JSON.stringify({collections:0}));

