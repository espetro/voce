fn main() {
    println!("cargo:rerun-if-changed=voce_adapter.c");
    println!("cargo:rerun-if-changed=voce_adapter.h");
    println!("cargo:rerun-if-changed=voce_native.h");
    cc::Build::new()
        .file("voce_adapter.c")
        .include(".")
        .warnings(true)
        .compile("voce_adapter");

    // Consumer manifests need a stable archive path beside the Rust
    // staticlib, not the per-build OUT_DIR chosen by cc.
    let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let archive = out_dir.join("libvoce_adapter.a");
    let target_dir = out_dir.ancestors().nth(3).unwrap();
    std::fs::copy(&archive, target_dir.join("libvoce_adapter.a")).unwrap();
}
