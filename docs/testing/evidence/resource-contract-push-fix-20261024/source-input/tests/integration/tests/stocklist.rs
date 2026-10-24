use rustdx_cli::fetch_code;
use std::sync::LazyLock;

macro_rules! get {
    (sz) => {{
        let mut set = ::std::collections::HashSet::with_capacity(3000);
        fetch_code::get_sz_stocks(&mut set).unwrap();
        let mut v = set.into_iter().collect::<Vec<_>>();
        v.sort();
        v
    }};
    (sh, $a:literal, $b:literal) => {{
        let mut set = ::std::collections::HashSet::with_capacity(3000);
        fetch_code::get_sh_stocks(&mut set, $a, $b).unwrap();
        let mut v = set.into_iter().collect::<Vec<_>>();
        v.sort();
        v
    }};
}

static SH8: LazyLock<Vec<String>> = LazyLock::new(|| get!(sh, "8", "1000"));
static SH1: LazyLock<Vec<String>> = LazyLock::new(|| get!(sh, "1", "2500"));
static SZ: LazyLock<Vec<String>> = LazyLock::new(|| get!(sz));

/// 打印当前抓取结果（证券名单随上市/退市每日变动，无法快照对比，仅供人工查看）。
#[test]
fn head() {
    if std::env::var_os("RUSTDX_SKIP_INTEGRATION_TESTS").is_some() {
        return;
    }
    let (sh8, sh1, sz) = (&*SH8, &*SH1, &*SZ);
    println!(
        "sh8: {}\n{:?}\n{:?}",
        sh8.len(),
        &sh8[..10],
        &sh8[sh8.len() - 10..]
    );
    println!(
        "sh1: {}\n{:?}\n{:?}",
        sh1.len(),
        &sh1[..10],
        &sh1[sh1.len() - 10..]
    );
    println!(
        "sz: {}\n{:?}\n{:?}",
        sz.len(),
        &sz[..10],
        &sz[sz.len() - 10..]
    );
}

/// 验证沪深交易所官网数据获取通道均可用，
/// 并对名单数量与代码格式做合理性断言（不对比具体名单）。
#[test]
fn stocklist() {
    if std::env::var_os("RUSTDX_SKIP_INTEGRATION_TESTS").is_some() {
        return;
    }
    let (sh8, sh1, sz) = (&*SH8, &*SH1, &*SZ);
    let (lsh8, lsh1, lsz) = (sh8.len(), sh1.len(), sz.len());

    // 数量级断言：证券数量随时间缓慢增长，异常缩小才说明抓取出了问题
    assert!(lsh1 > 1000, "沪市主板(60x)股票数异常: {lsh1}");
    assert!(lsh8 > 300, "科创板(688)股票数异常: {lsh8}");
    assert!(lsz > 2000, "深市股票数异常: {lsz}");

    // 代码格式断言
    assert!(
        sh1.iter().all(|s| s.starts_with("sh60")),
        "沪市主板代码前缀异常"
    );
    assert!(
        sh8.iter()
            .all(|s| s.starts_with("sh688") || s.starts_with("sh689")),
        "科创板代码前缀异常（688/689，689 为 CDR 存托凭证）"
    );
    assert!(
        sz.iter()
            .all(|s| s.starts_with("sz00") || s.starts_with("sz30")),
        "深市代码前缀异常"
    );
}
