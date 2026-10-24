db = connect(process.env.RUSTDX_RECOVERY_MONGO_URI).getSiblingDB("rustdx_recovery_c460890aa1f94602bc3495fcbd7c77e2");
if(db.adminCommand({ping:1}).ok!==1) throw Error("ping failed");
