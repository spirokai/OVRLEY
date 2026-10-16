//! Embed exactly the metric icons declared by the shared manifests.
use serde_json::Value;
use std::{collections::BTreeSet, env, fmt::Write, fs, path::PathBuf};

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
    for widget in widgets["definitions"]
        .as_array()
        .expect("widget definitions must be an array")
    {
        if let Some(asset_file) = widget["icon"]["assetFile"].as_str() {
            asset_files.insert(asset_file);
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
    fs::write(out.join("icon_assets.rs"), generated).unwrap();
}
