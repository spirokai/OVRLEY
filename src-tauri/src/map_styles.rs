//! The installed map styles, and `MapData`: the shell's map source, with them and the tile service.
//!
//! At startup every `*.json` in the build's `map-styles` folder, then in the user's
//! `Documents/OVRLEY/map-styles`, is read with [`read_style_file`], its id the file name in
//! lowercase. Every bundled style must install, the one a new map widget draws among them, or the
//! app does not start: they ship with it. A user style that doesn't install is logged and left out,
//! and so is one whose id is already installed, so a bundled style always means the same map.
//! Nothing is read again until the next start.
//!
//! The default style lists first, so it is also what a widget naming a missing style draws. The rest
//! list by label, bundled before the user's.

use crate::map_tile_service::{MapTileService, TileCoordinates};
use ovrley_map::{
    read_style_file, MapDataSource, MapSource, MapStyleCatalog, MapStyleId, MapStyleInfo,
    VECTOR_SOURCE,
};
use ovrley_widgets::model::standard_widgets::default_map_style;
use serde_json::{json, Value as Json};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const ATTRIBUTION: &str = "<a href=\"https://openfreemap.org\">OpenFreeMap</a> <a href=\"https://www.openmaptiles.org/\">&copy; OpenMapTiles</a> Data from <a href=\"https://www.openstreetmap.org/copyright\">OpenStreetMap</a>";

/// Where installed styles are read from.
pub(crate) struct MapStyleDirs {
    /// Shipped with the build. It must exist.
    pub(crate) bundled: PathBuf,
    /// The user's own. It is created when missing, so there is somewhere to put them.
    pub(crate) user: PathBuf,
}

pub(crate) struct MapStyleRegistry {
    catalog: MapStyleCatalog,
    /// Each installed style's document, as its file holds it.
    documents: HashMap<MapStyleId, Json>,
}

impl MapStyleRegistry {
    /// Fails when a bundled style can't be read or doesn't install, or the default isn't bundled.
    pub(crate) fn load(dirs: &MapStyleDirs) -> Result<Self, String> {
        let mut documents = HashMap::new();
        let mut bundled = Vec::new();
        for path in read_dir(&dirs.bundled)? {
            let (info, document) = read_style(&path).map_err(|reason| {
                format!(
                    "Bundled map style {} does not install: {reason}",
                    path.display()
                )
            })?;
            documents.insert(info.id.clone(), document);
            bundled.push(info);
        }

        if let Err(error) = fs::create_dir_all(&dirs.user) {
            log::warn!(
                "Map styles in {} are not read: {error}",
                dirs.user.display()
            );
        }
        let mut user = Vec::new();
        for path in read_dir(&dirs.user).unwrap_or_else(|error| {
            log::warn!("{error}");
            Vec::new()
        }) {
            let not_installed = |reason: &str| {
                log::warn!("Map style {} is not installed: {reason}", path.display());
            };
            match read_style(&path) {
                Err(reason) => not_installed(&reason),
                Ok((info, _)) if documents.contains_key(&info.id) => {
                    not_installed(&format!("{} is already installed", info.id))
                }
                Ok((info, document)) => {
                    documents.insert(info.id.clone(), document);
                    user.push(info);
                }
            }
        }

        let by_label = |a: &MapStyleInfo, b: &MapStyleInfo| {
            a.label
                .cmp(&b.label)
                .then_with(|| a.id.as_str().cmp(b.id.as_str()))
        };
        bundled.sort_by(by_label);
        user.sort_by(by_label);
        let default = bundled
            .iter()
            .position(|style| &style.id == default_map_style())
            .ok_or_else(|| {
                format!(
                    "The build has no {} map style, which new map widgets draw",
                    default_map_style()
                )
            })?;
        let default = bundled.remove(default);
        Ok(Self {
            // Two bundled files may differ only in case: then they share an id, and this fails.
            catalog: MapStyleCatalog::new(
                std::iter::once(default)
                    .chain(bundled)
                    .chain(user)
                    .collect(),
            )?,
            documents,
        })
    }

    pub(crate) fn catalog(&self) -> &MapStyleCatalog {
        &self.catalog
    }

    /// The style as the preview's MapLibre reads it: its vector tiles from `tile_url_template`, the
    /// local tile service, at the zooms it serves. `None` when `id` isn't installed.
    pub(crate) fn served(&self, id: &str, tile_url_template: &str) -> Option<Vec<u8>> {
        let mut style = self.documents.get(&MapStyleId::new(id).ok()?)?.clone();
        let source = style["sources"][VECTOR_SOURCE]
            .as_object_mut()
            .expect("an installed style draws from an openmaptiles source");
        source.remove("url");
        source.insert("tiles".into(), json!([tile_url_template]));
        source.insert("minzoom".into(), json!(0));
        source.insert("maxzoom".into(), json!(14));
        source.insert("attribution".into(), json!(ATTRIBUTION));
        Some(serde_json::to_vec(&style).expect("a JSON document serializes"))
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

/// The style in `path` with its document, or why it doesn't install.
fn read_style(path: &Path) -> Result<(MapStyleInfo, Json), String> {
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .ok_or("its file name is not UTF-8")?;
    let id = MapStyleId::new(&stem.to_ascii_lowercase())?;
    let bytes = fs::read(path).map_err(|error| error.to_string())?;
    read_style_file(id, &bytes).map_err(|errors| errors.join("; "))
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

    /// What a render's maps draw from.
    pub(crate) fn map_source(&self) -> MapSource {
        MapSource {
            styles: self.styles.catalog.clone(),
            data: Arc::new(self.clone()),
        }
    }
}

impl MapDataSource for MapData {
    fn style(&self, style: &MapStyleId) -> Result<Vec<u8>, String> {
        let document = self
            .styles
            .documents
            .get(style)
            .ok_or_else(|| format!("Map style {style} is not installed"))?;
        Ok(serde_json::to_vec(document).expect("a JSON document serializes"))
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

    fn style(name: Option<&str>) -> Json {
        let mut style = json!({
            "version": 8,
            "sources": { "openmaptiles": { "type": "vector", "url": "https://example.com/tiles.json" } },
            "layers": [{ "id": "bg", "type": "background", "paint": { "background-color": "#102030" } }],
        });
        if let Some(name) = name {
            style["name"] = json!(name);
        }
        style
    }

    fn write(dir: &Path, file: &str, style: Json) {
        fs::write(dir.join(file), serde_json::to_vec(&style).unwrap()).unwrap()
    }

    fn load(bundled: &Path, user: &Path) -> Result<MapStyleRegistry, String> {
        MapStyleRegistry::load(&MapStyleDirs {
            bundled: bundled.to_path_buf(),
            user: user.to_path_buf(),
        })
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
    fn the_bundled_styles_install_with_the_default_first() {
        let user = temp_dir("bundled");
        let registry = load(&bundled_dir(), &user).unwrap();
        assert_eq!(default_map_style().as_str(), "liberty");
        assert_eq!(
            ids(&registry),
            ["liberty", "bright", "dark", "fiord", "positron"]
        );
        fs::remove_dir_all(user).unwrap();
    }

    #[test]
    fn user_styles_install_after_the_bundled_ones_unless_they_fail_or_collide() {
        let (bundled, user) = (temp_dir("ordered-bundled"), temp_dir("ordered-user"));
        write(&bundled, "zulu.json", style(Some("Alpha")));
        write(&bundled, "liberty.json", style(Some("Liberty")));
        write(&user, "Beta.json", style(None));
        write(&user, "Liberty.json", style(Some("Mine")));
        write(&user, "my style.json", style(None));
        write(&user, "broken.json", json!({ "version": 8 }));
        fs::write(user.join("notes.txt"), "not a style").unwrap();

        let registry = load(&bundled, &user).unwrap();
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
    fn a_bundled_style_that_does_not_install_or_a_missing_default_stops_the_load() {
        let (bundled, user) = (temp_dir("strict-bundled"), temp_dir("strict-user"));
        write(&bundled, "zulu.json", style(None));
        assert!(load(&bundled, &user)
            .err()
            .unwrap()
            .contains("no liberty map style"));

        write(&bundled, "liberty.json", style(None));
        write(&bundled, "broken.json", json!({ "version": 8 }));
        assert!(load(&bundled, &user)
            .err()
            .unwrap()
            .contains("broken.json does not install"));
        fs::remove_dir_all(bundled).unwrap();
        fs::remove_dir_all(user).unwrap();
    }

    #[test]
    fn a_missing_user_folder_is_created_and_a_missing_bundled_one_fails() {
        let root = temp_dir("folders");
        let user = root.join("user");
        load(&bundled_dir(), &user).unwrap();
        assert!(user.is_dir());
        assert!(load(&root.join("missing"), &user).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_served_style_reads_its_tiles_from_the_local_service() {
        let user = temp_dir("served");
        let registry = load(&bundled_dir(), &user).unwrap();
        let served: Json = serde_json::from_slice(
            &registry
                .served("liberty", "http://127.0.0.1:1/tiles/{z}/{x}/{y}.pbf")
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
        assert!(registry.served("Liberty", "x").is_none());
        fs::remove_dir_all(user).unwrap();
    }
}
