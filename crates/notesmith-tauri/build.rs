fn main() {
    // Expose the build target triple so main.rs can resolve the sidecar path at runtime.
    let target = std::env::var("TARGET").unwrap_or_default();
    println!("cargo:rustc-env=TARGET_TRIPLE={target}");
    tauri_build::build()
}
