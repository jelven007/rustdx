#[path = "../../../crates/rustdx-core/tests/common/startup_binary.rs"]
mod startup_binary;

#[test]
fn runner_is_offline_by_default_and_explicit_warmup_is_governed() {
    startup_binary::verify(
        env!("CARGO_BIN_EXE_rustdx-runner"),
        &["status", "--root", "."],
        "[]",
    );
}
