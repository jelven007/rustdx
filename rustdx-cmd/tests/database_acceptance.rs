//! Opt-in acceptance against isolated real services. No production table is used.
use std::{fs, path::Path, process::Command};

fn ch(sql: &str) -> String {
    let url = std::env::var("CLICKHOUSE_URL").expect("CLICKHOUSE_URL is required");
    let mut response = ureq::post(&url).send(sql.as_bytes()).unwrap();
    response.body_mut().read_to_string().unwrap()
}

fn mongo(script: &str) -> String {
    let output = Command::new("mongosh")
        .args([
            &std::env::var("MONGODB_URI").expect("MONGODB_URI is required"),
            "--quiet",
            "--eval",
            script,
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

fn run(dir: &Path, backend: &str, table: &str, keep: bool) -> std::process::Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_rustdx"));
    command.current_dir(dir).args([
        "day",
        ".",
        "-o",
        backend,
        "-t",
        table,
        "--workers",
        "4",
        "--report",
        "run.json",
    ]);
    if keep {
        command.arg("--keep-csv");
    }
    if dir.join("empty.gbbq").exists() {
        command.args(["-g", "empty.gbbq"]);
    }
    command.output().unwrap()
}

fn fixture(dir: &Path, close: u32) {
    for code in 600000..600520 {
        let mut bytes = Vec::new();
        for date in [20261008, 20261009] {
            for value in [date, 1000, 1100, 900, close] {
                bytes.extend(value.to_le_bytes());
            }
            bytes.extend(1234.5f32.to_le_bytes());
            bytes.extend(12345u32.to_le_bytes());
            bytes.extend(0u32.to_le_bytes());
        }
        fs::write(dir.join(format!("sh{code}.day")), bytes).unwrap();
    }
}

#[test]
fn real_databases_verify_all_keys_updates_and_constraints() {
    if std::env::var("RUSTDX_DB_ACCEPTANCE").as_deref() != Ok("1") {
        return;
    }
    let database = format!("rustdx_acceptance_{}", std::process::id());
    let table = format!("{database}.bars");
    let dir = std::env::temp_dir().join(&database);
    fs::create_dir_all(&dir).unwrap();
    fixture(&dir, 1050);
    for backend in ["clickhouse", "mongodb"] {
        for (keep, close) in [(true, 1050), (false, 1080)] {
            fixture(&dir, close);
            let output = run(&dir, backend, &table, keep);
            assert!(
                output.status.success(),
                "{backend}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            let report: serde_json::Value =
                serde_json::from_slice(&fs::read(dir.join("run.json")).unwrap()).unwrap();
            assert_eq!(report["schema_version"], 3);
            assert_eq!(report["status"], "complete");
            assert_eq!(report["database_status"], "verified");
            assert_eq!(report["database_verification"]["rows"], 1040);
            assert_eq!(report["database_verification"]["logical_keys"], 1040);
            assert_eq!(report["database_verification"]["batches"], 3);
            assert_eq!(report["database_verification"]["first_date"], "2026-10-08");
            assert_eq!(report["database_verification"]["last_date"], "2026-10-09");
            assert_eq!(
                report["database_verification"]["numeric_fields"]
                    .as_array()
                    .unwrap()
                    .len(),
                6
            );
            assert_eq!(
                report["artifact"]["state"],
                if keep { "retained" } else { "removed" }
            );
            assert!(!dir.join(backend).exists());
            if backend == "clickhouse" {
                let counts = ch(&format!(
                    "SELECT count(), uniqExact(tuple(date,code)), min(close), max(close), round(sum(vol),2) FROM {table} FINAL FORMAT CSV"
                ));
                assert_eq!(
                    counts.trim(),
                    format!("1040,1040,{0},{0},128388", close as f64 / 100.)
                );
                ch(&format!("OPTIMIZE TABLE {table} FINAL"));
                assert_eq!(ch(&format!("SELECT count() FROM {table}")).trim(), "1040");
            } else {
                let text = mongo(&format!(
                    r#"const c=db.getSiblingDB("{database}").bars; print(JSON.stringify({{count:c.countDocuments(),sample:c.findOne({{date:"2026-10-09",code:"sh600519"}}),indexes:c.getIndexes()}}))"#
                ));
                let value: serde_json::Value = serde_json::from_str(text.trim()).unwrap();
                assert_eq!(value["count"], 1040);
                assert_eq!(value["sample"]["close"], close as f64 / 100.);
                assert_eq!(value["sample"]["vol"], 123.45);
                assert!(
                    value["indexes"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|i| i["unique"] == true
                            && i["key"]["date"] == 1
                            && i["key"]["code"] == 1)
                );
            }
        }
        fs::write(dir.join("empty.gbbq"), 0u32.to_le_bytes()).unwrap();
        let output = run(&dir, backend, &table, false);
        assert!(
            output.status.success(),
            "factor {backend}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let report: serde_json::Value =
            serde_json::from_slice(&fs::read(dir.join("run.json")).unwrap()).unwrap();
        assert_eq!(report["database_status"], "verified");
        assert_eq!(
            report["database_verification"]["numeric_fields"]
                .as_array()
                .unwrap()
                .len(),
            8
        );
        if backend == "clickhouse" {
            assert_eq!(
                ch(&format!(
                    "SELECT preclose,factor FROM {table} FINAL LIMIT 1 FORMAT CSV"
                ))
                .trim(),
                "10.8,1"
            );
        } else {
            let text = mongo(&format!(
                "print(JSON.stringify(db.getSiblingDB(\"{database}\").bars.findOne()));"
            ));
            let row: serde_json::Value = serde_json::from_str(text.trim()).unwrap();
            assert_eq!(row["preclose"], 10.8);
            assert_eq!(row["factor"], 1.0);
        }
        fs::remove_file(dir.join("empty.gbbq")).unwrap();
    }
    // Incompatible ClickHouse keys and existing MongoDB duplicates fail before
    // replacing a previously published CSV.
    let bad_table = format!("{database}.bad");
    ch(&format!(
        "CREATE TABLE {bad_table} AS {table} ENGINE=ReplacingMergeTree(version) ORDER BY (date,code,close)"
    ));
    fs::write(dir.join("clickhouse"), "previous success").unwrap();
    assert!(!run(&dir, "clickhouse", &bad_table, false).status.success());
    assert_eq!(
        fs::read_to_string(dir.join("clickhouse")).unwrap(),
        "previous success"
    );
    mongo(&format!(
        r#"db.getSiblingDB("{database}").bad.insertMany([{{date:"2026-10-09",code:"sh600000"}},{{date:"2026-10-09",code:"sh600000"}}]);"#
    ));
    fs::write(dir.join("mongodb"), "previous success").unwrap();
    assert!(!run(&dir, "mongodb", &bad_table, false).status.success());
    assert_eq!(
        fs::read_to_string(dir.join("mongodb")).unwrap(),
        "previous success"
    );
    ch(&format!("DROP DATABASE {database}"));
    mongo(&format!("db.getSiblingDB(\"{database}\").dropDatabase();"));
    fs::remove_dir_all(dir).unwrap();
}
