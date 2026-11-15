// With the `dev-windows` feature, `ui/dev-index.slint` re-exports `ui/index.slint` plus the
// showcase (ui/showcase/) and the test harness windows (tests/harness/), so they share the app's
// globals. It also emits the debug info the testing backend's element search needs.
//
// The translations are bundled from the React app's locale files, converted to gettext catalogs
// in OUT_DIR (`build_support/translations.rs`).
#[path = "build_support/translations.rs"]
mod translations;

fn main() {
    let dev = std::env::var_os("CARGO_FEATURE_DEV_WINDOWS").is_some();
    let root = if dev { "ui/dev-index.slint" } else { "ui/index.slint" };

    let manifest_dir = std::path::PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR"));
    let locales = manifest_dir.join("../../app/src/i18n/locales");
    let catalogs = std::path::PathBuf::from(std::env::var_os("OUT_DIR").expect("cargo sets OUT_DIR")).join("translations");
    let inputs = translations::convert(&locales, &catalogs).unwrap_or_else(|error| panic!("ovrley_ui: translations: {error}"));
    println!("cargo:rerun-if-changed={}", locales.display());
    for input in inputs {
        println!("cargo:rerun-if-changed={}", input.display());
    }

    let config = slint_build::CompilerConfiguration::new()
        .with_debug_info(dev)
        .with_bundled_translations(catalogs);
    slint_build::compile_with_config(root, config).expect("ovrley_ui: Slint compilation failed");
}
