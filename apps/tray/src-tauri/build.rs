fn main() {
    // Load AVFoundation so AVCaptureDevice is registered for the mic prompt.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        println!("cargo:rustc-link-lib=framework=AVFoundation");
    }
    tauri_build::build()
}
