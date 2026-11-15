//! The committed i18n string code matches `en-translation.json`, which the React app keeps editing.
//! Regenerate with `cargo run -p ovrley_ui --example generate_strings`.

#[path = "../codegen/strings.rs"]
mod strings;

#[test]
fn generated_strings_are_not_stale() {
    let root = strings::repo_root();
    let english = std::fs::read_to_string(strings::english_catalog_path(&root)).unwrap();
    for file in strings::generate(&root, &english) {
        let committed = std::fs::read_to_string(&file.path).unwrap_or_default();
        assert!(
            committed == file.contents,
            "{} is stale: run `cargo run -p ovrley_ui --example generate_strings`",
            file.path.display()
        );
    }
}

#[test]
fn kebab_case_rules() {
    assert_eq!(strings::kebab("pinDrawer"), "pin-drawer");
    assert_eq!(strings::kebab("blueColorComponent0255"), "blue-color-component-0255");
    assert_eq!(strings::kebab("sections.general.title"), "sections-general-title");
    assert_eq!(strings::kebab("000000Or800"), "k-000000-or-800");
}

#[test]
fn apply_locale_writes_the_catalog_into_the_window_globals() {
    use ovrley_app::i18n::{catalog, keys, Locale};
    use ovrley_ui::{AppShellStrings, AppWindow, ComponentStrings};
    use slint::ComponentHandle;

    i_slint_backend_testing::init_no_event_loop();
    let window = AppWindow::new().unwrap();
    assert_eq!(window.global::<AppShellStrings>().get_close(), catalog().t(Locale::En, keys::APP_SHELL_CLOSE));

    ovrley_ui::i18n_strings::apply_locale(&window, catalog(), Locale::De);
    assert_eq!(window.global::<AppShellStrings>().get_close(), catalog().t(Locale::De, keys::APP_SHELL_CLOSE));
    assert_ne!(catalog().t(Locale::De, keys::APP_SHELL_CLOSE), catalog().t(Locale::En, keys::APP_SHELL_CLOSE));
    assert_eq!(window.global::<ComponentStrings>().get_gradient(), "Gradient");
}
