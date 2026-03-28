use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    // Link CoreAudio and CoreFoundation frameworks on macOS
    #[cfg(target_os = "macos")]
    {
        println!("cargo:rustc-link-lib=framework=CoreAudio");
        println!("cargo:rustc-link-lib=framework=CoreFoundation");
        println!("cargo:rustc-link-lib=framework=AudioToolbox");
    }

    // Write Info.plist to OUT_DIR for reference; the .app bundle setup is manual for dev.
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let plist_path = out_dir.join("Info.plist");

    let plist = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleIdentifier</key>
    <string>io.voce.app</string>
    <key>CFBundleName</key>
    <string>Voce</string>
    <key>CFBundleDisplayName</key>
    <string>Voce</string>
    <key>CFBundleVersion</key>
    <string>0.1.0</string>
    <key>CFBundleShortVersionString</key>
    <string>0.1.0</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleSignature</key>
    <string>????</string>
    <key>NSPrincipalClass</key>
    <string>NSApplication</string>
    <key>LSUIElement</key>
    <true/>
    <key>NSMicrophoneUsageDescription</key>
    <string>Voce needs microphone access to filter your voice on calls.</string>
    <key>NSHighResolutionCapable</key>
    <true/>
</dict>
</plist>
"#;

    fs::write(&plist_path, plist).expect("failed to write Info.plist");

    // Tell cargo to re-run build.rs if it changes
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=assets/");
}
