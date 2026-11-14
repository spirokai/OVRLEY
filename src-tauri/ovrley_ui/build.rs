// With the `dev-windows` feature, `ui/dev-index.slint` re-exports `ui/index.slint` plus the
// showcase (ui/showcase/) and the test harness windows (tests/harness/), so they share the app's
// globals. It also emits the debug
// info the testing backend's element search needs.
fn main() {
    let dev = std::env::var_os("CARGO_FEATURE_DEV_WINDOWS").is_some();
    let root = if dev {
        "ui/dev-index.slint"
    } else {
        "ui/index.slint"
    };
    let config = slint_build::CompilerConfiguration::new().with_debug_info(dev);
    slint_build::compile_with_config(root, config).expect("ovrley_ui: Slint compilation failed");
}
