//! Regenerates the i18n string code from `app/src/i18n/locales/en-translation.json`:
//! `cargo run -p ovrley_ui --example generate_strings`. `tests/generated_strings.rs` fails while
//! the committed files are stale.

#[path = "../codegen/strings.rs"]
mod strings;

fn main() {
    let root = strings::repo_root();
    let english = std::fs::read_to_string(strings::english_catalog_path(&root)).expect("read en-translation.json");
    for file in strings::generate(&root, &english) {
        std::fs::create_dir_all(file.path.parent().expect("generated files live in a directory")).expect("create the output directory");
        std::fs::write(&file.path, file.contents).expect("write a generated file");
        println!("wrote {}", file.path.display());
    }
}
