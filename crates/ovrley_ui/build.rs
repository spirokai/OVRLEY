// With the `dev-windows` feature, `ui/dev-index.slint` re-exports `ui/index.slint` plus the
// showcase (ui/showcase/) and the test harness windows (tests/harness/), so they share the app's
// globals. It also emits the debug info the testing backend's element search needs.
//
// The translations are bundled from the catalogs in the root locales/ folder.

fn main() {
    // On macOS and Linux MDK is found through an rpath to the SDK's library folder (ovrley_playback's
    // `links` metadata), which only the linked executables can carry. Windows has no rpath:
    // ovrley_playback's build script copies the DLLs beside them.
    if let Ok(lib) = std::env::var("DEP_MDK_LIB_DIR") {
        println!("cargo:rustc-link-arg=-Wl,-rpath,{lib}");
    }
    #[cfg(windows)]
    windows_resources();
    let dev = std::env::var_os("CARGO_FEATURE_DEV_WINDOWS").is_some();
    let root = if dev { "ui/dev-index.slint" } else { "ui/index.slint" };

    let catalogs = bundle_catalogs().unwrap_or_else(|error| panic!("ovrley_ui: translations: {error}"));

    let config = slint_build::CompilerConfiguration::new()
        .with_debug_info(dev)
        .with_bundled_translations(catalogs);
    slint_build::compile_with_config(root, config).expect("ovrley_ui: Slint compilation failed");
}

/// Copies `locales/<lang>.po` to the `<lang>/LC_MESSAGES/<domain>.po` tree Slint reads, in OUT_DIR.
/// The domain is the crate's name.
fn bundle_catalogs() -> Result<std::path::PathBuf, String> {
    let locales = std::path::Path::new(&std::env::var_os("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR")).join("../../locales");
    let bundle = std::path::PathBuf::from(std::env::var_os("OUT_DIR").expect("cargo sets OUT_DIR")).join("translations");
    if bundle.exists() {
        std::fs::remove_dir_all(&bundle).map_err(|error| format!("{}: {error}", bundle.display()))?;
    }
    println!("cargo:rerun-if-changed={}", locales.display());
    for entry in std::fs::read_dir(&locales).map_err(|error| format!("{}: {error}", locales.display()))? {
        let path = entry.map_err(|error| error.to_string())?.path();
        if path.extension().is_none_or(|extension| extension != "po") {
            continue;
        }
        println!("cargo:rerun-if-changed={}", path.display());
        let language = path.file_stem().ok_or_else(|| format!("{} has no name", path.display()))?;
        let target = bundle.join(language).join("LC_MESSAGES");
        std::fs::create_dir_all(&target).map_err(|error| format!("{}: {error}", target.display()))?;
        std::fs::copy(&path, target.join("ovrley_ui.po")).map_err(|error| format!("{}: {error}", path.display()))?;
    }
    Ok(bundle)
}

/// The app's icon, version information and manifest: common controls v6 for the native message
/// dialogs, per-monitor DPI awareness v2. Only the app's executable gets them.
#[cfg(windows)]
fn windows_resources() {
    const MANIFEST: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" xmlns:asmv3="urn:schemas-microsoft-com:asm.v3" manifestVersion="1.0">
  <dependency>
    <dependentAssembly>
      <assemblyIdentity type="win32" name="Microsoft.Windows.Common-Controls" version="6.0.0.0" processorArchitecture="*" publicKeyToken="6595b64144ccf1df" language="*" />
    </dependentAssembly>
  </dependency>
  <asmv3:application>
    <asmv3:windowsSettings>
      <dpiAware xmlns="http://schemas.microsoft.com/SMI/2005/WindowsSettings">true/pm</dpiAware>
      <dpiAwareness xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">PerMonitorV2, PerMonitor</dpiAwareness>
    </asmv3:windowsSettings>
  </asmv3:application>
</assembly>
"#;
    let icon = std::path::Path::new(&std::env::var_os("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR")).join("../../assets/icons/icon.ico");
    println!("cargo:rerun-if-changed={}", icon.display());
    winresource::WindowsResource::new()
        .set_icon(icon.to_str().expect("the checkout's path is UTF-8"))
        .set("ProductName", "OVRLEY")
        .set("FileDescription", "OVRLEY")
        .set_manifest(MANIFEST)
        .compile_for("ovrley")
        .expect("ovrley_ui: compiling the Windows resources failed");
}
