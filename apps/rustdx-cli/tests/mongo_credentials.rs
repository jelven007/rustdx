//! Local tool doubles verify credential delivery and cleanup, not MongoDB service behavior.
#![cfg(unix)]
use std::{fs, os::unix::fs::PermissionsExt, process::Command};

#[test]
fn authenticated_uri_stays_out_of_argv_and_private_config_is_removed() {
    let dir = std::env::temp_dir().join(format!("rustdx-mongo-credentials-{}", std::process::id()));
    fs::create_dir_all(dir.join("tools")).unwrap();
    let secret = "mongodb://synthetic_user:synthetic_password@127.0.0.1:27018";
    let script = r#"#!/usr/bin/python3
import csv,json,os,pathlib,stat,sys
root=pathlib.Path(os.environ["RUSTDX_TEST_TOOLS"])
secret=os.environ["MONGODB_URI"]
assert all(secret not in arg and "synthetic_password" not in arg for arg in sys.argv)
if pathlib.Path(sys.argv[0]).name=="mongoimport":
    assert "--uri" not in sys.argv
    p=pathlib.Path(sys.argv[sys.argv.index("--config")+1])
    assert stat.S_IMODE(p.stat().st_mode)==0o600
    assert json.loads(p.read_text())["uri"]==secret
    (root/"config-path").write_text(str(p))
    source=sys.argv[sys.argv.index("--file")+1]
    rows=[]
    for row in csv.DictReader(open(source)):
        rows.append({key:(value if key in ["date","code"] else float(value)) for key,value in row.items()})
    (root/"rows.json").write_text(json.dumps(rows))
    sys.exit(9 if os.environ.get("RUSTDX_TEST_IMPORT_FAIL")=="1" else 0)
assert "--nodb" in sys.argv and os.environ["RUSTDX_MONGODB_URI"]==secret
if "rustdx_unique_index_verified" in sys.argv[-1]:
    print("rustdx_unique_index_verified")
else:
    rows=json.loads((root/"rows.json").read_text())
    for line in sys.stdin:
        request=json.loads(line)
        selected=[row for row in rows if [row["date"],row["code"]] in request["keys"]]
        print(json.dumps({"id":request["id"],"rows":selected}),flush=True)
"#;
    for name in ["mongosh", "mongoimport"] {
        let path = dir.join("tools").join(name);
        fs::write(&path, script).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
    }
    let mut bytes = Vec::new();
    for value in [20250106u32, 1000, 1100, 900, 1000] {
        bytes.extend(value.to_le_bytes());
    }
    bytes.extend(1000f32.to_le_bytes());
    bytes.extend(1000u32.to_le_bytes());
    bytes.extend(0u32.to_le_bytes());
    fs::write(dir.join("sz000001.day"), bytes).unwrap();
    for fail in [true, false] {
        let output = Command::new(env!("CARGO_BIN_EXE_rustdx"))
            .arg("--no-tcp-startup")
            .current_dir(&dir)
            .env(
                "PATH",
                format!("{}:/usr/bin:/bin", dir.join("tools").display()),
            )
            .env("MONGODB_URI", secret)
            .env("RUSTDX_TEST_TOOLS", &dir)
            .env("RUSTDX_TEST_IMPORT_FAIL", if fail { "1" } else { "0" })
            .args([
                "day",
                ".",
                "-o",
                "mongodb",
                "--keep-csv",
                "--report",
                "run.json",
            ])
            .output()
            .unwrap();
        assert_eq!(
            output.status.success(),
            !fail,
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let path = fs::read_to_string(dir.join("config-path")).unwrap();
        assert!(!std::path::Path::new(&path).exists());
        let report: serde_json::Value =
            serde_json::from_slice(&fs::read(dir.join("run.json")).unwrap()).unwrap();
        assert_eq!(
            report["database_status"],
            if fail { "unknown" } else { "verified" }
        );
        assert!(
            !serde_json::to_string(&report)
                .unwrap()
                .contains("synthetic_password")
        );
        assert!(
            dir.join(if fail { "mongodb" } else { "mongodb.csv" })
                .exists()
        );
    }
    fs::remove_dir_all(dir).unwrap();
}
