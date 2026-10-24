use insta::assert_debug_snapshot;
use std::mem::size_of;

#[test]
fn tcp() {
    use rustdx_complete::tcp;
    let tcp_size = size_of::<tcp::Tcp>();
    assert!(
        // 配置和连接各有共享IP池句柄、连接代次与进程内ID；
        // 自动心跳增加帧锁Arc、worker句柄和可选间隔，仍有固定上限。
        // 仍限制连接对象本身的堆栈开销，池及测速结果存储在共享堆内存。
        (128..=232 + 2 * size_of::<usize>() + 2 * size_of::<u64>()).contains(&tcp_size),
        "Tcp包含平台相关的TcpStream，结构体大小超出预期：{tcp_size}"
    );
    assert_debug_snapshot!(size_of::<tcp::SecurityCount>(),    @"24");
    assert_debug_snapshot!(size_of::<tcp::SecurityList>(),     @"72");
    assert_debug_snapshot!(size_of::<tcp::SecurityListData>(), @"48");
    assert_debug_snapshot!(size_of::<tcp::stock::Kline>(),     @"88");
    assert_debug_snapshot!(size_of::<tcp::stock::KlineData>(), @"80");
    assert_debug_snapshot!(size_of::<tcp::stock::Xdxr>(),      @"104");
    assert_debug_snapshot!(size_of::<tcp::stock::XdxrData>(),  @"48");
}
