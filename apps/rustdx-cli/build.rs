fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!(
        "cargo:rustc-env=RUSTDX_VERSION=v{}",
        std::env::var("CARGO_PKG_VERSION").expect("CARGO_PKG_VERSION")
    );
}
