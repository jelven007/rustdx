use eyre::{Result, eyre};

pub(super) fn validate_clickhouse_schema(metadata: &str, columns: &str) -> Result<()> {
    let fields: Vec<_> = metadata.trim_end_matches('\n').split('\t').collect();
    let compact = |value: &str| {
        value
            .chars()
            .filter(|c| !c.is_whitespace() && *c != '`')
            .collect::<String>()
    };
    let engine = fields.first().map(|s| compact(s)).unwrap_or_default();
    let engine_suffix = engine.strip_prefix("ReplacingMergeTree(version)");
    if fields.len() != 3
        || engine_suffix.is_none_or(|suffix| {
            !suffix.is_empty()
                && !suffix.starts_with("ORDERBY")
                && !suffix.starts_with("PARTITIONBY")
                && !suffix.starts_with("SETTINGS")
        })
        || compact(fields[1]) != "date,code"
        || !matches!(
            compact(fields[2]).as_str(),
            "" | "tuple()" | "toYYYYMM(date)" | "toYear(date)" | "date"
        )
    {
        eyre::bail!("要求ReplacingMergeTree(version)、ORDER BY(date,code)，分区只能由date确定");
    }
    let columns: std::collections::HashMap<_, _> = columns
        .lines()
        .map(|line| {
            let fields: Vec<_> = line.split('\t').collect();
            (fields.first().copied().unwrap_or(""), fields)
        })
        .collect();
    for (name, expected_type) in [
        ("date", "Date"),
        ("code", "String"),
        ("open", "Float64"),
        ("high", "Float64"),
        ("low", "Float64"),
        ("close", "Float64"),
        ("amount", "Float64"),
        ("vol", "Float64"),
        ("preclose", "Float64"),
        ("factor", "Float64"),
        ("version", "UInt64"),
    ] {
        let fields = columns.get(name).ok_or_else(|| eyre!("缺少字段{name}"))?;
        if fields.get(1) != Some(&expected_type) {
            eyre::bail!("{name}类型应为{expected_type}");
        }
        if matches!(name, "preclose" | "factor" | "version")
            && (fields.get(2) != Some(&"DEFAULT") || fields.get(3).is_none_or(|s| s.is_empty()))
        {
            eyre::bail!("{name}缺少默认值");
        }
        if name == "version" && compact(fields[3]) != "toUnixTimestamp64Nano(now64(9))" {
            eyre::bail!("version必须为每次导入自动生成纳秒版本");
        }
    }
    Ok(())
}
