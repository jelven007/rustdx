db = connect(process.env.RUSTDX_RECOVERY_MONGO_URI).getSiblingDB("rustdx_recovery_3291b0df13ac4945bff839756d5e6b9a");
print(db.version());
