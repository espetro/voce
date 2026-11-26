fn main() {
    println!("cargo:rerun-if-changed=voce_adapter.c");
    println!("cargo:rerun-if-changed=voce_adapter.h");
    println!("cargo:rerun-if-changed=voce_native.h");
    cc::Build::new()
        .file("voce_adapter.c")
        .include(".")
        .warnings(true)
        .compile("voce_adapter");
}
