use insta::assert_debug_snapshot;
use std::mem::size_of;

#[test]
fn tcp() {
    use rustdx_complete::tcp;
    let tcp_size = size_of::<tcp::Tcp>();
    assert!(
        (128..=160).contains(&tcp_size),
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
