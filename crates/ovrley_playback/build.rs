// Compiles the `cpp!` blocks and links MDK from the SDK that `MDK_SDK` points at (the repository's
// `.cargo/config.toml` sets it to vendor/mdk-sdk; the environment may name another).
// - macOS: mdk.framework; Linux: libmdk.so, which finds the libc++ and FFmpeg beside it through
//   its own `$ORIGIN` runpath. A dependency's build script cannot add an rpath to the final binary,
//   so the binary's own build script reads the library folder as `DEP_MDK_LIB_DIR` (`links = "mdk"`)
//   and adds it; this crate's own tests get theirs here.
// - Windows: mdk.lib. Windows has no rpath, so the DLLs (mdk.dll, ffmpeg-*.dll) are copied beside
//   the built binaries and test binaries.

use std::path::{Path, PathBuf};

/// Where MDK publishes the nightly SDK the app builds against.
const SDK_URL: &str = "https://sourceforge.net/projects/mdk-sdk/files/nightly/";

fn main() {
    println!("cargo:rerun-if-env-changed=MDK_SDK");
    println!("cargo:rerun-if-changed=src");
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").expect("cargo sets CARGO_CFG_TARGET_OS");
    let target_arch = std::env::var("CARGO_CFG_TARGET_ARCH").expect("cargo sets CARGO_CFG_TARGET_ARCH");
    let (archive, lib, library) = match target_os.as_str() {
        "macos" => ("mdk-sdk-macOS.tar.xz", PathBuf::from("lib"), "mdk.framework"),
        "linux" => ("mdk-sdk-linux.tar.xz", Path::new("lib").join(linux_lib_dir(&target_arch)), "libmdk.so"),
        "windows" => ("mdk-sdk-windows-clang.7z", PathBuf::from("lib/x64"), "mdk.lib"),
        other => panic!("ovrley_playback: macOS, Windows and Linux only, not {other}"),
    };
    let sdk = PathBuf::from(std::env::var_os("MDK_SDK").unwrap_or_else(|| {
        panic!("ovrley_playback: no MDK_SDK; run cargo inside the repository, whose .cargo/config.toml sets it, or point it at the extracted mdk-sdk folder ({archive} from {SDK_URL})")
    }));
    let lib = sdk.join(lib);
    assert!(
        lib.join(library).exists(),
        "ovrley_playback: {} has no {library}; run `node scripts/install-mdk.mjs` (or extract {archive} from {SDK_URL})",
        lib.display()
    );

    let mut cpp = cpp_build::Config::new();
    cpp.include(sdk.join("include"));
    if target_os == "windows" {
        cpp.flag("/std:c++17").flag("/EHsc").define("NOMINMAX", None).define("WIN32_LEAN_AND_MEAN", None);
    } else {
        cpp.flag("-std=c++17");
    }
    cpp.build("src/lib.rs");

    if target_os == "macos" {
        println!("cargo:rustc-link-search=framework={}", lib.display());
        println!("cargo:rustc-link-lib=framework=mdk");
    } else {
        println!("cargo:rustc-link-search=native={}", lib.display());
        println!("cargo:rustc-link-lib=dylib=mdk");
    }
    if target_os == "windows" {
        copy_dlls(&sdk.join("bin/x64"));
    } else {
        println!("cargo:rustc-link-arg=-Wl,-rpath,{}", lib.display());
        println!("cargo:lib_dir={}", lib.display());
    }
}

/// Copies the SDK's DLLs into the profile's folder and its `deps` and `examples`, where `cargo run`
/// and `cargo test` start executables. An identical copy is left alone, because a running app holds it.
fn copy_dlls(bin: &Path) {
    let fail = |path: &Path, error: std::io::Error| -> ! { panic!("ovrley_playback: {}: {error}", path.display()) };
    let out = PathBuf::from(std::env::var_os("OUT_DIR").expect("cargo sets OUT_DIR"));
    // OUT_DIR is <profile>/build/<package>/out.
    let profile = out.ancestors().nth(3).expect("OUT_DIR lies in a profile folder");
    for entry in std::fs::read_dir(bin).unwrap_or_else(|error| fail(bin, error)) {
        let source = entry.unwrap_or_else(|error| fail(bin, error)).path();
        if source.extension().is_none_or(|extension| extension != "dll") {
            continue;
        }
        let name = source.file_name().expect("a directory entry has a name");
        let bytes = std::fs::read(&source).unwrap_or_else(|error| fail(&source, error));
        for folder in [profile.to_path_buf(), profile.join("deps"), profile.join("examples")] {
            std::fs::create_dir_all(&folder).unwrap_or_else(|error| fail(&folder, error));
            let destination = folder.join(name);
            if std::fs::read(&destination).is_ok_and(|copied| copied == bytes) {
                continue;
            }
            std::fs::write(&destination, &bytes).unwrap_or_else(|error| fail(&destination, error));
        }
    }
}

/// The SDK's library folder for a Rust target architecture (`lib/amd64`, `lib/arm64`).
fn linux_lib_dir(target_arch: &str) -> &'static str {
    match target_arch {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        other => panic!("ovrley_playback: no MDK SDK library folder for {other}"),
    }
}
