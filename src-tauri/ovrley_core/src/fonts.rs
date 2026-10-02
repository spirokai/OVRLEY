//! Session font catalog discovered from bundled assets and system faces.
//! Stable IDs use typographic family names; capabilities come from Skia.

use crate::error::{CoreError, CoreResult};
use serde::Serialize;
use skia_safe::{
    font_arguments::{variation_position::Coordinate, VariationPosition},
    font_style::Slant,
    FontArguments, FontMgr, Typeface,
};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

#[derive(Clone, Debug, Serialize)]
pub struct FontAxis {
    pub tag: String,
    pub min: f32,
    pub default: f32,
    pub max: f32,
    pub hidden: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct FontFace {
    pub style: String,
    pub weight: f32,
    pub axes: Vec<FontAxis>,
    /// Bundled asset filename, or null for a system face.
    pub file: Option<String>,
    /// Exact system face identity for browser local() registration.
    pub local_name: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct FontFamily {
    pub id: String,
    pub name: String,
    /// Null means system capabilities have not been requested yet.
    pub faces: Option<Vec<FontFace>>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FontCatalog {
    pub recommended_fonts: Vec<FontFamily>,
    pub system_fonts: Vec<FontFamily>,
}

#[derive(Clone)]
struct LoadedFamily {
    family: FontFamily,
    typefaces: Vec<Typeface>,
    data: Vec<Option<Vec<u8>>>,
}

#[derive(Default)]
struct SessionFonts {
    catalog: Option<FontCatalog>,
    families: HashMap<String, Arc<LoadedFamily>>,
    variations: HashMap<VariationKey, Typeface>,
}

#[derive(Hash, PartialEq, Eq)]
struct VariationKey {
    id: String,
    face: usize,
    coordinates: Vec<(u32, u32)>,
}

fn sessions() -> &'static Mutex<HashMap<Vec<PathBuf>, SessionFonts>> {
    static SESSIONS: OnceLock<Mutex<HashMap<Vec<PathBuf>, SessionFonts>>> = OnceLock::new();
    SESSIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn describe_face(typeface: &Typeface, file: Option<String>) -> FontFace {
    let style = match typeface.font_style().slant() {
        Slant::Upright => "normal",
        Slant::Italic => "italic",
        Slant::Oblique => "oblique",
    };
    let axes = typeface
        .variation_design_parameters()
        .unwrap_or_default()
        .into_iter()
        .map(|axis| FontAxis {
            tag: String::from_utf8((*axis.tag).to_be_bytes().to_vec())
                .expect("font axis tag must be ASCII"),
            min: axis.min,
            default: axis.def,
            max: axis.max,
            hidden: axis.is_hidden(),
        })
        .collect();
    FontFace {
        style: style.into(),
        weight: *typeface.font_style().weight() as f32,
        axes,
        local_name: file.is_none().then(|| {
            typeface
                .post_script_name()
                .unwrap_or_else(|| typeface.family_name())
        }),
        file,
    }
}

// OpenType name ID 16 is the typographic family. Legacy ID 1 may include a
// weight (e.g. "Teko Light"), so it cannot identify a variable family.
fn bundled_family_name(typeface: &Typeface) -> String {
    if let Some(table) = typeface.copy_table_data(u32::from_be_bytes(*b"name")) {
        let bytes = table.as_bytes();
        let read = |offset| {
            bytes
                .get(offset..offset + 2)
                .map(|pair| u16::from_be_bytes(pair.try_into().unwrap()) as usize)
        };
        if let (Some(count), Some(strings)) = (read(2), read(4)) {
            for index in 0..count {
                let record = 6 + index * 12;
                if read(record + 6) != Some(16) || !matches!(read(record), Some(0 | 3)) {
                    continue;
                }
                if let (Some(length), Some(offset)) = (read(record + 8), read(record + 10)) {
                    if let Some(value) = bytes.get(strings + offset..strings + offset + length) {
                        let utf16: Vec<_> = value
                            .chunks_exact(2)
                            .map(|pair| u16::from_be_bytes(pair.try_into().unwrap()))
                            .collect();
                        if let Ok(name) = String::from_utf16(&utf16) {
                            if !name.is_empty() {
                                return name;
                            }
                        }
                    }
                }
            }
        }
    }
    typeface.family_name()
}

fn discover_bundled(font_dirs: &[PathBuf]) -> CoreResult<Vec<LoadedFamily>> {
    let manager = FontMgr::default();
    let mut paths = BTreeMap::new();
    for dir in font_dirs {
        for entry in fs::read_dir(dir).map_err(|source| CoreError::Io {
            path: dir.clone(),
            source,
        })? {
            let entry = entry.map_err(|source| CoreError::Io {
                path: dir.clone(),
                source,
            })?;
            let path = entry.path();
            let Some(extension) = path.extension().and_then(|value| value.to_str()) else {
                continue;
            };
            if !["ttf", "otf", "ttc"].contains(&extension.to_ascii_lowercase().as_str()) {
                continue;
            }
            let file = entry.file_name().to_string_lossy().into_owned();
            // User-supplied backups are intentionally outside the active font catalog.
            if path
                .file_stem()
                .unwrap()
                .to_string_lossy()
                .ends_with("_backup")
            {
                continue;
            }
            paths.entry(file).or_insert(path);
        }
    }
    let mut families: BTreeMap<String, LoadedFamily> = BTreeMap::new();
    for (file, path) in paths {
        let bytes = fs::read(&path).map_err(|source| CoreError::Io {
            path: path.clone(),
            source,
        })?;
        let typeface = manager
            .new_from_data(&bytes, None)
            .ok_or_else(|| CoreError::Config(format!("invalid font asset: {file}")))?;
        let name = bundled_family_name(&typeface);
        let extension = path
            .extension()
            .unwrap()
            .to_string_lossy()
            .to_ascii_lowercase();
        let id = format!("{name}.{extension}");
        let face = describe_face(&typeface, Some(file.clone()));
        if let Some(axis) = face.axes.iter().find(|axis| axis.tag == "wght") {
            if !(axis.min..=axis.max).contains(&400.0) {
                return Err(CoreError::Config(format!(
                    "{file}: weight range must include 400"
                )));
            }
        }
        let loaded = families.entry(id.clone()).or_insert_with(|| LoadedFamily {
            family: FontFamily {
                id,
                name,
                faces: Some(Vec::new()),
            },
            typefaces: Vec::new(),
            data: Vec::new(),
        });
        loaded.family.faces.as_mut().unwrap().push(face);
        loaded.typefaces.push(typeface);
        loaded.data.push(Some(bytes));
    }
    Ok(families.into_values().collect())
}

/// Discovers bundled faces once per session; system faces remain lazy.
pub fn font_catalog(font_dirs: &[PathBuf]) -> CoreResult<FontCatalog> {
    let mut sessions = sessions().lock().unwrap();
    let session = sessions.entry(font_dirs.to_vec()).or_default();
    if let Some(catalog) = &session.catalog {
        return Ok(catalog.clone());
    }
    let mut recommended_fonts = Vec::new();
    for loaded in discover_bundled(font_dirs)? {
        recommended_fonts.push(loaded.family.clone());
        session
            .families
            .insert(loaded.family.id.clone(), Arc::new(loaded));
    }
    let mut system_fonts: Vec<_> = FontMgr::default()
        .family_names()
        .map(|name| FontFamily {
            id: name.clone(),
            name,
            faces: None,
        })
        .collect();
    system_fonts.sort_by_key(|font| font.name.to_lowercase());
    system_fonts.dedup_by(|a, b| a.id == b.id);
    let catalog = FontCatalog {
        recommended_fonts,
        system_fonts,
    };
    session.catalog = Some(catalog.clone());
    Ok(catalog)
}

fn loaded_family(font_dirs: &[PathBuf], id: &str) -> CoreResult<Arc<LoadedFamily>> {
    if let Some(loaded) = sessions()
        .lock()
        .unwrap()
        .get(font_dirs)
        .and_then(|session| session.families.get(id))
    {
        return Ok(Arc::clone(loaded));
    }
    font_catalog(font_dirs)?;
    let mut sessions = sessions().lock().unwrap();
    let session = sessions.get_mut(font_dirs).unwrap();
    if let Some(loaded) = session.families.get(id) {
        return Ok(Arc::clone(loaded));
    }
    let mut styles = FontMgr::default().match_family(id);
    let default_width = styles
        .match_style(skia_safe::FontStyle::normal())
        .ok_or_else(|| CoreError::Config(format!("font family is unavailable: {id}")))?
        .font_style()
        .width();
    let mut typefaces = Vec::new();
    let mut faces = Vec::new();
    for index in 0..styles.count() {
        let typeface = styles
            .new_typeface(index)
            .ok_or_else(|| CoreError::Config(format!("unable to load {id} face {index}")))?;
        if typeface.font_style().width() != default_width {
            continue;
        }
        faces.push(describe_face(&typeface, None));
        typefaces.push(typeface);
    }
    if faces.is_empty() {
        return Err(CoreError::Config(format!(
            "font family is unavailable: {id}"
        )));
    }
    let loaded = Arc::new(LoadedFamily {
        family: FontFamily {
            id: id.into(),
            name: id.into(),
            faces: Some(faces),
        },
        data: vec![None; typefaces.len()],
        typefaces,
    });
    session.families.insert(id.into(), Arc::clone(&loaded));
    Ok(loaded)
}

/// Resolves and caches system capabilities on first use.
pub fn font_capabilities(font_dirs: &[PathBuf], id: &str) -> CoreResult<FontFamily> {
    Ok(loaded_family(font_dirs, id)?.family.clone())
}

/// Returns the same bundled bytes that Skia loaded, including after asset replacement.
pub fn bundled_face_data(font_dirs: &[PathBuf], id: &str, index: usize) -> CoreResult<Vec<u8>> {
    loaded_family(font_dirs, id)?
        .data
        .get(index)
        .and_then(Clone::clone)
        .ok_or_else(|| CoreError::Config(format!("no bundled font data for {id} face {index}")))
}

// CSS Fonts 4 weight matching. Browser and Rust use this policy for static
// faces and weights outside a variable face's range; no synthetic bolding.
fn weight_rank(requested: f32, candidate: f32) -> (u8, f32) {
    if requested < 400.0 {
        if candidate <= requested {
            (0, requested - candidate)
        } else {
            (1, candidate - requested)
        }
    } else if requested <= 500.0 {
        if candidate >= requested && candidate <= 500.0 {
            (0, candidate - requested)
        } else if candidate < requested {
            (1, requested - candidate)
        } else {
            (2, candidate - requested)
        }
    } else if candidate >= requested {
        (0, candidate - requested)
    } else {
        (1, requested - candidate)
    }
}

fn supports_italic(face: &FontFace) -> bool {
    face.style == "italic"
        || face.style == "oblique"
        || face.axes.iter().any(|axis| {
            (axis.tag == "ital" && axis.max > 0.0)
                || (axis.tag == "slnt" && (axis.min < 0.0 || axis.max > 0.0))
        })
}

fn italic_rank(face: &FontFace) -> u8 {
    if face.style == "italic"
        || face
            .axes
            .iter()
            .any(|axis| axis.tag == "ital" && axis.max > 0.0)
    {
        0
    } else {
        1
    }
}

/// Resolves genuine italic faces or ital/slnt instances, then matches weight.
/// Unsupported families stay upright without synthesis. ital selects 1;
/// slnt selects -12 degrees (or +12 for a positive-only axis), clamped to its
/// range. All other axes keep defaults, including opsz.
pub fn resolve_typeface(
    font_dirs: &[PathBuf],
    id: &str,
    weight: f32,
    italic: bool,
) -> CoreResult<Typeface> {
    let loaded = loaded_family(font_dirs, id)?;
    let faces = loaded.family.faces.as_ref().unwrap();
    let italic = italic && faces.iter().any(supports_italic);
    let (index, resolved_weight) = faces
        .iter()
        .enumerate()
        .filter(|(_, face)| {
            if italic {
                supports_italic(face)
            } else {
                face.style == "normal"
            }
        })
        .map(|(index, face)| {
            let candidate = match face.axes.iter().find(|axis| axis.tag == "wght") {
                Some(axis) if !axis.hidden => weight.clamp(axis.min, axis.max),
                Some(axis) => axis.default,
                None => face.weight,
            };
            (index, candidate)
        })
        .min_by(|(a_index, a), (b_index, b)| {
            (
                if italic {
                    italic_rank(&faces[*a_index])
                } else {
                    0
                },
                weight_rank(weight, *a),
            )
                .partial_cmp(&(
                    if italic {
                        italic_rank(&faces[*b_index])
                    } else {
                        0
                    },
                    weight_rank(weight, *b),
                ))
                .unwrap()
        })
        .ok_or_else(|| CoreError::Config(format!("{id}: no matching font face")))?;
    let face = &faces[index];
    let typeface = &loaded.typefaces[index];
    if face.axes.is_empty() {
        return Ok(typeface.clone());
    }
    let coordinates: Vec<_> = face
        .axes
        .iter()
        .map(|axis| Coordinate {
            axis: u32::from_be_bytes(axis.tag.as_bytes().try_into().unwrap()).into(),
            value: match axis.tag.as_str() {
                "wght" => resolved_weight,
                "ital" => {
                    if italic {
                        1.0_f32.clamp(axis.min, axis.max)
                    } else {
                        0.0
                    }
                }
                "slnt" => {
                    if italic {
                        (if axis.min < 0.0 { -12.0_f32 } else { 12.0_f32 })
                            .clamp(axis.min, axis.max)
                    } else {
                        0.0
                    }
                }
                _ => axis.default,
            },
        })
        .collect();
    let key = VariationKey {
        id: id.into(),
        face: index,
        coordinates: coordinates
            .iter()
            .map(|coord| (*coord.axis, coord.value.to_bits()))
            .collect(),
    };
    let mut sessions = sessions().lock().unwrap();
    let cache = &mut sessions.get_mut(font_dirs).unwrap().variations;
    if let Some(typeface) = cache.get(&key) {
        return Ok(typeface.clone());
    }
    let args = FontArguments::new().set_variation_design_position(VariationPosition {
        coordinates: &coordinates,
    });
    let typeface = typeface
        .clone_with_arguments(&args)
        .ok_or_else(|| CoreError::Render(format!("{id}: unable to resolve font variation")))?;
    cache.insert(key, typeface.clone());
    Ok(typeface)
}
