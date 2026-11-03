//! The installed map styles, and `MapData`: the shell's map source, with them and the tile service.
//!
//! At startup every `*.json` in the build's `map-styles` folder, then in the user's
//! `Documents/OVRLEY/map-styles`, is read with [`read_style_file`]. A file that passes is installed
//! under its file name; one that doesn't is logged and left out. A user style whose name collides
//! with a bundled one is left out too, so a bundled style always means the same map. Nothing is read
//! again until the next start.
//!
//! Liberty, the default for a new widget, lists first when installed, so it is also what a widget
//! naming a missing style draws. The rest list by label, bundled before the user's.

use crate::map_tile_service::{MapTileService, TileCoordinates};
use ovrley_map::{
    read_style_file, MapDataSource, MapStyleCatalog, MapStyleId, MapStyleInfo, VECTOR_SOURCE,
};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// What a new map widget draws.
const DEFAULT_MAP_STYLE: &str = "liberty";
const ATTRIBUTION: &str = "<a href=\"https://openfreemap.org\">OpenFreeMap</a> <a href=\"https://www.openmaptiles.org/\">&copy; OpenMapTiles</a> Data from <a href=\"https://www.openstreetmap.org/copyright\">OpenStreetMap</a>";

/// Where installed styles are read from.
pub(crate) struct MapStyleDirs {
    /// Shipped with the build. It must exist.
    pub(crate) bundled: PathBuf,
    /// The user's own. It is created when missing, so there is somewhere to put them.
    pub(crate) user: PathBuf,
}

#[derive(Default)]
pub(crate) struct MapStyleRegistry {
    catalog: MapStyleCatalog,
    documents: HashMap<MapStyleId, Vec<u8>>,
}

impl MapStyleRegistry {
    /// Fails only when the bundled folder can't be read.
    pub(crate) fn load(dirs: &MapStyleDirs) -> Result<Self, String> {
        let bundled = read_dir(&dirs.bundled)?;
        if let Err(error) = fs::create_dir_all(&dirs.user) {
            log::warn!(
                "Map styles in {} are not read: {error}",
                dirs.user.display()
            );
        }
        let user = read_dir(&dirs.user).unwrap_or_else(|error| {
            log::warn!("{error}");
            Vec::new()
        });

        let mut documents = HashMap::new();
        let mut groups: [Vec<MapStyleInfo>; 2] = [Vec::new(), Vec::new()];
        for (group, files) in groups.iter_mut().zip([bundled, user]) {
            for path in files {
                let Some((info, bytes)) = read_style(&path) else {
                    continue;
                };
                if let Some(installed) = documents
                    .keys()
                    .find(|installed: &&MapStyleId| installed.collides(&info.id))
                {
                    log::warn!(
                        "Map style {} is not installed: {installed} is already installed",
                        path.display()
                    );
                    continue;
                }
                documents.insert(info.id.clone(), bytes);
                group.push(info);
            }
            group.sort_by(|a, b| {
                a.label
                    .cmp(&b.label)
                    .then_with(|| a.id.as_str().cmp(b.id.as_str()))
            });
        }
        let mut styles: Vec<MapStyleInfo> = groups.into_iter().flatten().collect();
        if let Some(default) = styles
            .iter()
            .position(|style| style.id.as_str() == DEFAULT_MAP_STYLE)
        {
            let default = styles.remove(default);
            styles.insert(0, default);
        }
        Ok(Self {
            catalog: MapStyleCatalog::new(styles)?,
            documents,
        })
    }

    pub(crate) fn catalog(&self) -> &MapStyleCatalog {
        &self.catalog
    }

    /// The style as the preview's MapLibre reads it: its vector tiles from `tile_url_template`, the
    /// local tile service. `None` when it isn't installed.
    pub(crate) fn served(
        &self,
        id: &str,
        tile_url_template: &str,
    ) -> Option<Result<Vec<u8>, String>> {
        let info = self.catalog.get(id)?;
        let bytes = &self.documents[&info.id];
        Some(serve(bytes, tile_url_template))
    }
}

/// Every `*.json` in `dir`, by file name.
fn read_dir(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let entries = fs::read_dir(dir)
        .map_err(|error| format!("Map styles in {} are not readable: {error}", dir.display()))?;
    let mut files: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
        })
        .collect();
    files.sort();
    Ok(files)
}

/// The style in `path` with its bytes, or `None` after logging why it isn't installed.
fn read_style(path: &Path) -> Option<(MapStyleInfo, Vec<u8>)> {
    let not_installed = |reason: &str| {
        log::warn!("Map style {} is not installed: {reason}", path.display());
    };
    let id = match path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .ok_or_else(|| "its file name is not UTF-8".to_string())
        .and_then(MapStyleId::new)
    {
        Ok(id) => id,
        Err(error) => {
            not_installed(&error);
            return None;
        }
    };
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) => {
            not_installed(&error.to_string());
            return None;
        }
    };
    match read_style_file(id, &bytes) {
        Ok(info) => Some((info, bytes)),
        Err(errors) => {
            not_installed(&errors.join("; "));
            None
        }
    }
}

/// Points the style's vector source at the local tile service, at the zooms it serves.
fn serve(bytes: &[u8], tile_url_template: &str) -> Result<Vec<u8>, String> {
    let mut style: serde_json::Value = serde_json::from_slice(bytes)
        .map_err(|error| format!("Installed map style is not JSON: {error}"))?;
    let source = style["sources"][VECTOR_SOURCE]
        .as_object_mut()
        .ok_or_else(|| format!("Installed map style has no {VECTOR_SOURCE} source"))?;
    source.remove("url");
    source.insert("tiles".into(), serde_json::json!([tile_url_template]));
    source.insert("minzoom".into(), serde_json::json!(0));
    source.insert("maxzoom".into(), serde_json::json!(14));
    source.insert("attribution".into(), serde_json::json!(ATTRIBUTION));
    serde_json::to_vec(&style).map_err(|error| format!("Failed to serialize map style: {error}"))
}

/// Everything a map's bytes come from: the installed styles and the tile service.
#[derive(Clone)]
pub(crate) struct MapData {
    styles: Arc<MapStyleRegistry>,
    tiles: MapTileService,
}

impl MapData {
    pub(crate) fn new(styles: MapStyleRegistry, tiles: MapTileService) -> Self {
        Self {
            styles: Arc::new(styles),
            tiles,
        }
    }

    pub(crate) fn registry(&self) -> &MapStyleRegistry {
        &self.styles
    }

    pub(crate) fn tiles(&self) -> &MapTileService {
        &self.tiles
    }
}

impl MapDataSource for MapData {
    fn styles(&self) -> &MapStyleCatalog {
        self.styles.catalog()
    }

    fn style(&self, style: &MapStyleId) -> Result<Vec<u8>, String> {
        self.styles
            .documents
            .get(style)
            .cloned()
            .ok_or_else(|| format!("Map style {style} is not installed"))
    }

    fn tile(&self, z: u8, x: u32, y: u32) -> Result<Vec<u8>, String> {
        self.tiles
            .resolve(TileCoordinates { zoom: z, x, y })
            .map(|payload| payload.bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn bundled_dir() -> PathBuf {
        crate::runtime_paths::source_repo_root()
            .join("assets")
            .join("map-styles")
    }

    fn temp_dir(label: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("ovrley-map-styles-{label}-{}", Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn style(name: Option<&str>) -> serde_json::Value {
        let mut style = serde_json::json!({
            "version": 8,
            "sources": { "openmaptiles": { "type": "vector", "url": "https://example.com/tiles.json" } },
            "layers": [{ "id": "bg", "type": "background", "paint": { "background-color": "#102030" } }],
        });
        if let Some(name) = name {
            style["name"] = serde_json::json!(name);
        }
        style
    }

    fn ids(registry: &MapStyleRegistry) -> Vec<&str> {
        registry
            .catalog()
            .styles()
            .iter()
            .map(|style| style.id.as_str())
            .collect()
    }

    #[test]
    fn the_bundled_styles_install_with_liberty_first() {
        let user = temp_dir("bundled");
        let registry = MapStyleRegistry::load(&MapStyleDirs {
            bundled: bundled_dir(),
            user: user.clone(),
        })
        .unwrap();
        assert_eq!(
            ids(&registry),
            ["liberty", "bright", "dark", "fiord", "positron"]
        );
        fs::remove_dir_all(user).unwrap();
    }

    #[test]
    fn user_styles_install_after_the_bundled_ones_unless_they_fail_or_collide() {
        let (bundled, user) = (temp_dir("ordered-bundled"), temp_dir("ordered-user"));
        let write = |dir: &Path, file: &str, style: serde_json::Value| {
            fs::write(dir.join(file), serde_json::to_vec(&style).unwrap()).unwrap()
        };
        write(&bundled, "zulu.json", style(Some("Alpha")));
        write(&bundled, "liberty.json", style(Some("Liberty")));
        write(&user, "beta.json", style(None));
        write(&user, "Liberty.json", style(Some("Mine")));
        write(&user, "my style.json", style(None));
        write(&user, "broken.json", serde_json::json!({ "version": 8 }));
        fs::write(user.join("notes.txt"), "not a style").unwrap();

        let registry = MapStyleRegistry::load(&MapStyleDirs {
            bundled: bundled.clone(),
            user: user.clone(),
        })
        .unwrap();
        assert_eq!(ids(&registry), ["liberty", "zulu", "beta"]);
        assert_eq!(registry.catalog().styles()[2].label, "beta");
        assert!(registry
            .catalog()
            .styles()
            .iter()
            .all(|style| style.background == [0x10, 0x20, 0x30]));
        fs::remove_dir_all(bundled).unwrap();
        fs::remove_dir_all(user).unwrap();
    }

    #[test]
    fn a_missing_user_folder_is_created_and_a_missing_bundled_one_fails() {
        let root = temp_dir("folders");
        let user = root.join("user");
        MapStyleRegistry::load(&MapStyleDirs {
            bundled: bundled_dir(),
            user: user.clone(),
        })
        .unwrap();
        assert!(user.is_dir());
        assert!(MapStyleRegistry::load(&MapStyleDirs {
            bundled: root.join("missing"),
            user,
        })
        .is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_served_style_reads_its_tiles_from_the_local_service() {
        let user = temp_dir("served");
        let registry = MapStyleRegistry::load(&MapStyleDirs {
            bundled: bundled_dir(),
            user: user.clone(),
        })
        .unwrap();
        let served: serde_json::Value = serde_json::from_slice(
            &registry
                .served("liberty", "http://127.0.0.1:1/tiles/{z}/{x}/{y}.pbf")
                .unwrap()
                .unwrap(),
        )
        .unwrap();
        let source = &served["sources"][VECTOR_SOURCE];
        assert_eq!(
            source["tiles"][0],
            "http://127.0.0.1:1/tiles/{z}/{x}/{y}.pbf"
        );
        assert!(source.get("url").is_none());
        assert!(registry.served("unknown", "x").is_none());
        fs::remove_dir_all(user).unwrap();
    }
}
