//! Embed exactly the icons declared by the shared manifests (current metrics only).
use serde_json::Value;
use std::{collections::BTreeSet, env, fmt::Write, fs, path::PathBuf};

#[path = "build_support/activity_schema.rs"]
mod activity_schema;

fn main() {
    let assets = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("../../assets");
    let metrics_path = assets.join("standard-metrics.json");
    let widgets_path = assets.join("standard-widgets.json");
    for path in [&metrics_path, &widgets_path] {
        println!("cargo:rerun-if-changed={}", path.display());
    }
    let metrics: Value = serde_json::from_slice(&fs::read(&metrics_path).unwrap())
        .expect("standard metrics manifest must be valid JSON");
    let widgets: Value = serde_json::from_slice(&fs::read(&widgets_path).unwrap())
        .expect("standard widgets manifest must be valid JSON");
    let mut asset_files = BTreeSet::new();
    for metric in metrics["definitions"]
        .as_array()
        .expect("metric definitions must be an array")
    {
        if metric["current"]
            .as_bool()
            .expect("metric current must be a boolean")
        {
            asset_files.insert(
                metric["icon"]["assetFile"]
                    .as_str()
                    .expect("metric icon must declare assetFile"),
            );
        }
    }
    // Every other icon the manifests declare: widgets, displays, display modes and drawer groups,
    // which the native editor's drawer shows.
    fn collect<'a>(value: &'a Value, out: &mut BTreeSet<&'a str>) {
        match value {
            Value::Object(map) => {
                for (key, item) in map {
                    match (key.as_str(), item) {
                        ("assetFile", Value::String(file)) => {
                            out.insert(file);
                        }
                        _ => collect(item, out),
                    }
                }
            }
            Value::Array(items) => items.iter().for_each(|item| collect(item, out)),
            _ => {}
        }
    }
    collect(&widgets, &mut asset_files);
    for (key, value) in metrics.as_object().expect("the metrics manifest is an object") {
        if key != "definitions" {
            collect(value, &mut asset_files);
        }
    }

    let mut generated = String::from(
        "fn icon_asset(asset_file: &str) -> &'static IconAsset {\nmatch asset_file {\n",
    );
    for asset_file in asset_files {
        assert!(
            PathBuf::from(asset_file).components().count() == 1 && asset_file.ends_with(".svg"),
            "icon assetFile must be an SVG filename: {asset_file}"
        );
        let path = assets
            .join("widget-icons")
            .join(asset_file)
            .canonicalize()
            .unwrap_or_else(|error| panic!("cannot bundle manifest icon {asset_file}: {error}"));
        println!("cargo:rerun-if-changed={}", path.display());
        writeln!(generated, "{asset_file:?} => {{ static ASSET: IconAsset = IconAsset {{ markup: include_str!({path:?}), parsed: OnceLock::new() }}; &ASSET }},").unwrap();
    }
    generated.push_str(
        "_ => panic!(\"icon asset missing from the bundled manifests: {asset_file}\"),\n}\n}\n",
    );
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    activity_schema::generate(&metrics, &widgets, &out);
    fs::write(out.join("icon_assets.rs"), generated).unwrap();
}
