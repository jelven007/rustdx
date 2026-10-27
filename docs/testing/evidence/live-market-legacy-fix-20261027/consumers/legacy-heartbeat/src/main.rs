use rustdx_complete::tcp::stock::Client;
fn legacy(client: &mut Client) -> std::io::Result<u16> { client.heartbeat() }
fn main() { let _ = legacy; }
