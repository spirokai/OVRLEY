// Links MDK from the SDK that `MDK_SDK` points at: vendor/mdk-sdk, which scripts/install-mdk.mjs
// installs (`.cargo/config.toml`), unless the environment names another.
// - macOS: mdk.framework. The binary that uses this crate adds an rpath to the library folder
//   itself (a dependency's build script cannot add link arguments to the final binary), which it
//   reads as `DEP_MDK_LIB_DIR` (`links = "mdk"`); this crate's own examples get theirs here.
// - Linux: libmdk.so, found the same way through an rpath to `$MDK_SDK/lib/<arch>`; libmdk finds
//   the libc++ and FFmpeg next to it through its own `$ORIGIN` runpath.
// - Windows: mdk.lib. Windows has no rpath, so the DLLs (mdk.dll, ffmpeg-*.dll) are copied beside
//   the built binaries, test binaries and examples.

// MDK's nightly SDK: the v0.40.1 release (2026-11-06, MSVC) crashes with the
// D3D12 zero-copy decoders (`D3D12`, `MFT:d3d=12`) on Windows, also in MDK's own glfwplay; the
// nightly of 2026-11-20 (Clang) does not.
const SDK_URL: &str = "https://sourceforge.net/projects/mdk-sdk/files/nightly/";

fn main() {
    println!("cargo:rerun-if-env-changed=MDK_SDK");
    println!("cargo:rerun-if-changed=src/lib.rs");
    println!("cargo:rerun-if-changed=src/player.rs");
    println!("cargo:rerun-if-changed=src/target.rs");
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").expect("cargo sets CARGO_CFG_TARGET_OS");
    let target_arch = std::env::var("CARGO_CFG_TARGET_ARCH").expect("cargo sets CARGO_CFG_TARGET_ARCH");
    let archive = match target_os.as_str() {
        "macos" => "mdk-sdk-macOS.tar.xz",
        "windows" => "mdk-sdk-windows-clang.7z",
        "linux" => "mdk-sdk-linux.tar.xz",
        other => panic!("ovrley_playback: macOS, Windows and Linux only, not {other}"),
    };
    let sdk = std::env::var("MDK_SDK")
        .unwrap_or_else(|_| panic!("ovrley_playback: no MDK_SDK; run cargo inside the repository, whose .cargo/config.toml sets it, or point it at the extracted mdk-sdk folder ({archive} from {SDK_URL})"));
    let sdk = std::path::PathBuf::from(sdk);

    let mut cpp = cpp_build::Config::new();
    cpp.include(sdk.join("include"));
    match target_os.as_str() {
        "macos" => {
            let lib = sdk.join("lib");
            if !lib.join("mdk.framework").is_dir() {
                panic!("ovrley_playback: MDK_SDK={} has no lib/mdk.framework; run `node scripts/install-mdk.mjs` (or extract {archive} from {SDK_URL})", sdk.display());
            }
            cpp.flag("-std=c++17").build("src/lib.rs");
            println!("cargo:rustc-link-search=framework={}", lib.display());
            println!("cargo:rustc-link-lib=framework=mdk");
            println!("cargo:rustc-link-arg=-Wl,-rpath,{}", lib.display());
            println!("cargo:lib_dir={}", lib.display());
        }
        "linux" => {
            let lib = sdk.join("lib").join(linux_lib_dir(&target_arch));
            if !lib.join("libmdk.so").is_file() {
                panic!("ovrley_playback: MDK_SDK={} has no {}/libmdk.so; run `node scripts/install-mdk.mjs` (or extract {archive} from {SDK_URL})", sdk.display(), lib.display());
            }
            cpp.flag("-std=c++17").build("src/lib.rs");
            println!("cargo:rustc-link-search=native={}", lib.display());
            println!("cargo:rustc-link-lib=dylib=mdk");
            println!("cargo:rustc-link-arg=-Wl,-rpath,{}", lib.display());
            println!("cargo:lib_dir={}", lib.display());
        }
        _ => {
            let lib = sdk.join("lib/x64");
            if !lib.join("mdk.lib").is_file() {
                panic!("ovrley_playback: MDK_SDK={} has no lib/x64/mdk.lib; run `node scripts/install-mdk.mjs` (or extract {archive} from {SDK_URL})", sdk.display());
            }
            cpp.flag("/std:c++17")
                .flag("/EHsc")
                .define("NOMINMAX", None)
                .define("WIN32_LEAN_AND_MEAN", None)
                .build("src/lib.rs");
            println!("cargo:rustc-link-search=native={}", lib.display());
            println!("cargo:rustc-link-lib=dylib=mdk");
            copy_dlls(&sdk.join("bin/x64"));
        }
    }
}

/// Copies the SDK's DLLs into the profile's folder and its `deps` and `examples`, where `cargo run`
/// and `cargo test` start executables. An identical copy is left alone, because a running app holds it.
fn copy_dlls(bin: &std::path::Path) {
    let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").expect("cargo sets OUT_DIR"));
    // OUT_DIR is <profile>/build/<package>/out.
    let profile = out.ancestors().nth(3).expect("OUT_DIR lies in a profile folder");
    let entries = std::fs::read_dir(bin).unwrap_or_else(|error| panic!("ovrley_playback: {}: {error}", bin.display()));
    for entry in entries {
        let source = entry.expect("the SDK's bin folder is readable").path();
        if source.extension().is_none_or(|extension| extension != "dll") {
            continue;
        }
        let name = source.file_name().expect("a file has a name");
        let bytes = std::fs::read(&source).unwrap_or_else(|error| panic!("ovrley_playback: {}: {error}", source.display()));
        for folder in [profile.to_path_buf(), profile.join("deps"), profile.join("examples")] {
            std::fs::create_dir_all(&folder).unwrap_or_else(|error| panic!("ovrley_playback: {}: {error}", folder.display()));
            let destination = folder.join(name);
            if std::fs::read(&destination).is_ok_and(|copied| copied == bytes) {
                continue;
            }
            std::fs::write(&destination, &bytes).unwrap_or_else(|error| panic!("ovrley_playback: {}: {error}", destination.display()));
        }
    }
}

/// The SDK's library folder for a Rust target architecture (`lib/amd64`, `lib/arm64`, …).
fn linux_lib_dir(target_arch: &str) -> &'static str {
    match target_arch {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        other => panic!("ovrley_playback: no MDK SDK library folder for {other}"),
    }
}
