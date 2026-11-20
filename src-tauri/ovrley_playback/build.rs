// Links MDK from the SDK that `MDK_SDK` points at (spike D4): nothing is downloaded, copied or
// bundled.
// - macOS: mdk.framework. The binary that uses this crate adds an rpath to `$MDK_SDK/lib` itself
//   (a dependency's build script cannot add link arguments to the final binary); this crate's
//   own examples get theirs here.
// - Windows: mdk.lib. Windows has no rpath: `$MDK_SDK/bin/x64` (mdk.dll, ffmpeg-*.dll) must be on
//   `PATH` when the binary runs.

// MDK's nightly SDK: the v0.40.1 release (2026-11-06, MSVC) crashes with the
// D3D12 zero-copy decoders (`D3D12`, `MFT:d3d=12`) on Windows, also in MDK's own glfwplay; the
// nightly of 2026-11-20 (Clang) does not.
const SDK_URL: &str = "https://sourceforge.net/projects/mdk-sdk/files/nightly/";

fn main() {
    println!("cargo:rerun-if-env-changed=MDK_SDK");
    println!("cargo:rerun-if-changed=src/lib.rs");
    println!("cargo:rerun-if-changed=src/player.rs");
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").expect("cargo sets CARGO_CFG_TARGET_OS");
    let archive = match target_os.as_str() {
        "macos" => "mdk-sdk-macOS.tar.xz",
        "windows" => "mdk-sdk-windows-clang.7z",
        other => panic!("ovrley_playback: macOS and Windows only for now, not {other}"),
    };
    let sdk = std::env::var("MDK_SDK")
        .unwrap_or_else(|_| panic!("ovrley_playback: set MDK_SDK to the extracted mdk-sdk folder ({archive} from {SDK_URL})"));
    let sdk = std::path::PathBuf::from(sdk);

    let mut cpp = cpp_build::Config::new();
    cpp.include(sdk.join("include"));
    if target_os == "macos" {
        let lib = sdk.join("lib");
        if !lib.join("mdk.framework").is_dir() {
            panic!("ovrley_playback: MDK_SDK={} has no lib/mdk.framework (expected {archive} from {SDK_URL})", sdk.display());
        }
        cpp.flag("-std=c++17").build("src/lib.rs");
        println!("cargo:rustc-link-search=framework={}", lib.display());
        println!("cargo:rustc-link-lib=framework=mdk");
        println!("cargo:rustc-link-arg-examples=-Wl,-rpath,{}", lib.display());
    } else {
        let lib = sdk.join("lib/x64");
        if !lib.join("mdk.lib").is_file() {
            panic!("ovrley_playback: MDK_SDK={} has no lib/x64/mdk.lib (expected {archive} from {SDK_URL})", sdk.display());
        }
        cpp.flag("/std:c++17")
            .flag("/EHsc")
            .define("NOMINMAX", None)
            .define("WIN32_LEAN_AND_MEAN", None)
            .build("src/lib.rs");
        println!("cargo:rustc-link-search=native={}", lib.display());
        println!("cargo:rustc-link-lib=dylib=mdk");
    }
}
