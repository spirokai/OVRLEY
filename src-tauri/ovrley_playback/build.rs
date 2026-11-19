// Links mdk.framework from the SDK that `MDK_SDK` points at (spike D4): nothing is downloaded,
// copied or bundled. The binary that uses this crate adds an rpath to `$MDK_SDK/lib` itself
// (a dependency's build script cannot add link arguments to the final binary); this crate's own
// examples get theirs here.

const RELEASE_URL: &str = "https://github.com/wang-bin/mdk-sdk/releases/tag/v0.40.1";

fn main() {
    println!("cargo:rerun-if-env-changed=MDK_SDK");
    println!("cargo:rerun-if-changed=src/lib.rs");
    println!("cargo:rerun-if-changed=src/player.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        panic!("ovrley_playback: macOS only for now");
    }
    let sdk = std::env::var("MDK_SDK").unwrap_or_else(|_| {
        panic!("ovrley_playback: set MDK_SDK to the extracted mdk-sdk folder (mdk-sdk-macOS.tar.xz from {RELEASE_URL})")
    });
    let sdk = std::path::PathBuf::from(sdk);
    let framework = sdk.join("lib/mdk.framework");
    if !framework.is_dir() {
        panic!("ovrley_playback: MDK_SDK={} has no lib/mdk.framework (expected mdk-sdk-macOS.tar.xz from {RELEASE_URL})", sdk.display());
    }
    let lib = sdk.join("lib");

    cpp_build::Config::new()
        .include(sdk.join("include"))
        .flag("-std=c++17")
        .build("src/lib.rs");

    println!("cargo:rustc-link-search=framework={}", lib.display());
    println!("cargo:rustc-link-lib=framework=mdk");
    println!("cargo:rustc-link-arg-examples=-Wl,-rpath,{}", lib.display());
}
