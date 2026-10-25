use insta::assert_debug_snapshot;
use std::{mem::size_of, net::TcpStream, time::Instant};

#[test]
fn tcp() {
    use rustdx_complete::tcp;
    let tcp_size = size_of::<tcp::Tcp>();
    // 两个 socket 句柄（其中一个在 BufReader 内）及截止时钟的平台布局单独计入；
    // 配置、共享池/帧锁句柄、连接代次/ID等固定包装字段仍限于200字节。
    let upper_bound =
        200 + size_of::<TcpStream>() + size_of::<tcp::BufTcp>() + size_of::<Option<Instant>>();
    assert!(
        (128..=upper_bound).contains(&tcp_size),
        "Tcp结构体大小超出包装预算及平台布局：{tcp_size}，上限：{upper_bound}"
    );
    assert_debug_snapshot!(size_of::<tcp::SecurityCount>(),    @"24");
    assert_debug_snapshot!(size_of::<tcp::SecurityList>(),     @"72");
    assert_debug_snapshot!(size_of::<tcp::SecurityListData>(), @"48");
    assert_debug_snapshot!(size_of::<tcp::stock::Kline>(),     @"88");
    assert_debug_snapshot!(size_of::<tcp::stock::KlineData>(), @"80");
    assert_debug_snapshot!(size_of::<tcp::stock::Xdxr>(),      @"104");
    assert_debug_snapshot!(size_of::<tcp::stock::XdxrData>(),  @"48");
}
