#[path = "../../../crates/rustdx-core/tests/common/startup_binary.rs"]
mod startup_binary;

#[test]
fn cli_defaults_to_measured_pool_and_warmed_client_before_dispatch() {
    startup_binary::verify(env!("CARGO_BIN_EXE_rustdx"), &["show", "-v"], "当前版本号");
}
