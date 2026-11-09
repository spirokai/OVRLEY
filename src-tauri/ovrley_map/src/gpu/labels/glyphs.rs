//! The glyph service: the bundled font files, opened once, and every glyph made from them. Given a
//! style's font and a character it answers with the face that draws it, an SDF bitmap and its
//! metrics. Export's tile threads open the files in place; the preview's decode workers have no files,
//! so they are handed the same bytes and make the same glyphs.
//!
//! The SDF is made from the outline's coverage by a Euclidean distance transform with radius 8 and
//! cutoff 0.25 at 24 px with a 3 px buffer, the values the label shader's constants assume. Faces
//! with a `texture_scale` of 2 (CJK) are made at twice that.
//!
//! Everything here is pure Rust on plain IEEE arithmetic, so native and WASM make the same bytes. The
//! one transcendental (the coverage gamma table) goes through `libm`, not the platform's `pow`.

use super::text::{font_chain, Face, FontGlyph, GlyphBitmap, GlyphMetrics, BUFFER, ONE_EM};
use ab_glyph::{point, Font, FontRef, PxScale, ScaleFont};
#[cfg(not(target_arch = "wasm32"))]
use memmap2::Mmap;
use std::collections::HashMap;
#[cfg(not(target_arch = "wasm32"))]
use std::fs::File;
#[cfg(not(target_arch = "wasm32"))]
use std::path::Path;
use std::sync::{Arc, OnceLock, RwLock};

/// The em size, in pixels, glyph bitmaps are made at.
const FONT_SIZE: f32 = ONE_EM as f32;
/// Distance, in pixels at 1x, the SDF encodes either side of the edge.
const RADIUS: f64 = 8.0;
/// Where the edge sits in the byte range: 0.25 of it is outside, the rest is inside.
const CUTOFF: f64 = 0.25;
/// Larger than any squared distance in a glyph bitmap.
const INF: f64 = 1e20;
/// Glyph metrics set `top` against an origin above the em box; the rasterizer's is the baseline, so
/// it is moved up by this much.
const TOP_ADJUSTMENT: f32 = 27.5;
/// And `left` is moved right by this much.
const LEFT_ADJUSTMENT: f32 = 0.5;

/// A glyph in the atlas: the face that draws it and its character. The face fixes the resolution the
/// bitmap is made at.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct GlyphId {
    pub face: Face,
    pub character: char,
}

/// Where layout learns how big a font's glyphs are: the glyph service, or a test's stand-in.
pub(crate) trait GlyphLookup: Sync {
    /// The glyph `font` draws `character` with, or `None` when no face in its chain has it. A font
    /// with no chain is an error, since layout skips such a layer before asking.
    fn glyph(&self, font: &str, character: char) -> Result<Option<FontGlyph>, String>;
}

/// A font file's bytes: memory-mapped natively, so the OS loads only the outlines actually used, or
/// handed over whole where there are no files.
enum FontBytes {
    #[cfg(not(target_arch = "wasm32"))]
    Mapped(Mmap),
    Owned(Box<[u8]>),
}

impl FontBytes {
    fn as_slice(&self) -> &[u8] {
        match self {
            #[cfg(not(target_arch = "wasm32"))]
            Self::Mapped(map) => map,
            Self::Owned(bytes) => bytes,
        }
    }
}

/// One face's font, over its file's bytes.
struct FaceFile {
    /// Borrows `bytes`. Declared first so it is dropped first.
    font: FontRef<'static>,
    bytes: FontBytes,
}

impl FaceFile {
    fn new(face: Face, bytes: FontBytes) -> Result<Self, String> {
        // SAFETY: neither a mapping's nor a boxed slice's bytes move when their owner does, and `font`
        // is dropped before `bytes`.
        let slice: &'static [u8] = unsafe {
            std::slice::from_raw_parts(bytes.as_slice().as_ptr(), bytes.as_slice().len())
        };
        let font = FontRef::try_from_slice(slice)
            .map_err(|error| format!("{}: {error}", face.file_name()))?;
        Ok(Self { font, bytes })
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn open(face: Face, path: &Path) -> Result<Self, String> {
        let file = File::open(path).map_err(|error| format!("{}: {error}", path.display()))?;
        // SAFETY: the bundled font files are not modified while the app runs.
        let map =
            unsafe { Mmap::map(&file) }.map_err(|error| format!("{}: {error}", path.display()))?;
        Self::new(face, FontBytes::Mapped(map))
    }
}

/// Every glyph made so far, by the face that drew it and its character.
type MadeGlyphs = HashMap<(Face, char), (GlyphMetrics, Arc<GlyphBitmap>)>;

/// The bundled faces, and every glyph made from them so far.
pub struct GlyphService {
    faces: HashMap<Face, FaceFile>,
    made: RwLock<MadeGlyphs>,
}

impl GlyphService {
    /// Opens every bundled face in `directory` (`fonts/map/`).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn open(directory: &Path) -> Result<Self, String> {
        let faces = Face::ALL
            .into_iter()
            .map(|face| {
                Ok((
                    face,
                    FaceFile::open(face, &directory.join(face.file_name()))?,
                ))
            })
            .collect::<Result<_, String>>()?;
        Ok(Self::with_faces(faces))
    }

    /// The service over the bundled font files' bytes, one per [`Face::ALL`], in its order: what a
    /// host without the files fetched from the shell.
    pub fn from_files(files: Vec<Vec<u8>>) -> Result<Self, String> {
        if files.len() != Face::ALL.len() {
            return Err(format!(
                "The map glyph service needs {} font files, got {}",
                Face::ALL.len(),
                files.len()
            ));
        }
        let faces = Face::ALL
            .into_iter()
            .zip(files)
            .map(|(face, bytes)| {
                Ok((
                    face,
                    FaceFile::new(face, FontBytes::Owned(bytes.into_boxed_slice()))?,
                ))
            })
            .collect::<Result<_, String>>()?;
        Ok(Self::with_faces(faces))
    }

    fn with_faces(faces: HashMap<Face, FaceFile>) -> Self {
        Self {
            faces,
            made: RwLock::new(HashMap::new()),
        }
    }

    /// The bytes of the bundled font file `name`, which the shell serves to hosts without the files.
    pub fn font_file(&self, name: &str) -> Option<&[u8]> {
        Face::ALL
            .into_iter()
            .find(|face| face.file_name() == name)
            .map(|face| self.faces[&face].bytes.as_slice())
    }

    /// The distance field of a character a face has, which the atlas holds under that key.
    pub fn bitmap(&self, face: Face, character: char) -> Result<Arc<GlyphBitmap>, String> {
        self.made(face, character).map(|(_, bitmap)| bitmap)
    }

    /// How many glyphs have been rasterized so far. Export makes them all before its first frame.
    pub fn rasterized(&self) -> usize {
        self.made.read().expect("glyph cache lock").len()
    }

    /// The glyph `face` draws `character` with, from the cache or by rasterizing it.
    fn made(
        &self,
        face: Face,
        character: char,
    ) -> Result<(GlyphMetrics, Arc<GlyphBitmap>), String> {
        let key = (face, character);
        let cached = self
            .made
            .read()
            .expect("glyph cache lock")
            .get(&key)
            .cloned();
        match cached {
            Some(made) => Ok(made),
            None => {
                let rasterized =
                    rasterize(&self.faces[&face].font, character, face.texture_scale())?;
                let made = (rasterized.metrics, Arc::new(rasterized.bitmap));
                // Two threads may make the same glyph at once; both get the first one stored.
                Ok(self
                    .made
                    .write()
                    .expect("glyph cache lock")
                    .entry(key)
                    .or_insert(made)
                    .clone())
            }
        }
    }
}

impl GlyphLookup for GlyphService {
    fn glyph(&self, font: &str, character: char) -> Result<Option<FontGlyph>, String> {
        let chain = font_chain(font).ok_or_else(|| format!("no font chain for \"{font}\""))?;
        let Some(link) = chain
            .iter()
            .find(|link| self.faces[&link.face].font.glyph_id(character).0 != 0)
        else {
            return Ok(None);
        };
        Ok(Some(FontGlyph {
            face: link.face,
            bold: link.bold,
            metrics: self.made(link.face, character)?.0,
        }))
    }
}

/// A glyph as the atlas and the layout need it.
#[derive(Clone, Debug, PartialEq)]
struct Glyph {
    bitmap: GlyphBitmap,
    /// In glyph units (24 px em), whatever resolution the bitmap is.
    metrics: GlyphMetrics,
}

/// Rasterizes one character of `font` to an SDF bitmap. The caller has checked `font` has it.
fn rasterize(font: &impl Font, character: char, texture_scale: u32) -> Result<Glyph, String> {
    let scale = texture_scale as f32;
    let font_size = FONT_SIZE * scale;
    let buffer = BUFFER * texture_scale as usize;
    let radius = RADIUS * f64::from(scale);
    // A canvas fits a glyph into `size`, so a glyph that overflows it is cut off at the bottom and right.
    let max_glyph_dimension = font_size.ceil() as usize + buffer * 3;

    let units_per_em = font
        .units_per_em()
        .ok_or("the font does not say its units per em")?;
    // ab_glyph's scale is the font's height (ascent to descent); an em size is smaller.
    let px_scale = PxScale::from(font_size * font.height_unscaled() / units_per_em);
    let id = font.glyph_id(character);
    let advance = font.as_scaled(px_scale).h_advance(id);

    let mut metrics = GlyphMetrics {
        width: 0.0,
        height: 0.0,
        left: 0.0,
        top: 0.0,
        advance: advance / scale,
    };
    let blank = Glyph {
        bitmap: GlyphBitmap {
            width: 0,
            height: 0,
            data: Vec::new(),
        },
        metrics,
    };
    let Some(outlined) = font.outline_glyph(id.with_scale_and_position(px_scale, point(0.0, 0.0)))
    else {
        return Ok(blank);
    };

    // The pixel bounds are whole numbers, relative to the baseline origin with y down.
    let bounds = outlined.px_bounds();
    let glyph_top = -bounds.min.y as i32;
    let glyph_left = bounds.min.x as i32;
    let glyph_width = ((bounds.max.x - bounds.min.x) as usize).min(max_glyph_dimension);
    let glyph_height = ((bounds.max.y - bounds.min.y) as usize).min(max_glyph_dimension);
    if glyph_width == 0 || glyph_height == 0 {
        return Ok(blank);
    }

    let width = glyph_width + 2 * buffer;
    let height = glyph_height + 2 * buffer;
    let length = width * height;

    // Two grids of squared distances: from outside pixels to the outline, and from inside pixels.
    let mut grid_outer = vec![INF; length];
    let mut grid_inner = vec![0.0; length];
    let alpha_table = ALPHA_TABLE.get_or_init(alpha_table);
    outlined.draw(|x, y, coverage| {
        let (x, y) = (x as usize, y as usize);
        if x >= glyph_width || y >= glyph_height {
            return;
        }
        // The canvas holds 8-bit alpha.
        let alpha = (coverage.clamp(0.0, 1.0) * 255.0).round() as usize;
        if alpha == 0 {
            return;
        }
        let at = (y + buffer) * width + x + buffer;
        let distance = alpha_table[alpha];
        grid_outer[at] = distance.max(0.0);
        grid_inner[at] = (-distance).max(0.0);
    });

    let mut scratch = Edt1d::new(width.max(height));
    // Columns left and right of the glyph have no seeds and stay INF in the column pass.
    edt(
        &mut grid_outer,
        0,
        0,
        width,
        height,
        width,
        buffer,
        &mut scratch,
    );
    // The inner region is padded by a pixel so ink touching the bounding box sees the outside.
    let pad = buffer.min(1);
    edt(
        &mut grid_inner,
        buffer - pad,
        buffer - pad,
        glyph_width + 2 * pad,
        glyph_height + 2 * pad,
        width,
        0,
        &mut scratch,
    );

    // Inside maps to high bytes, with the edge gradient spanning
    // [-radius * cutoff, radius * (1 - cutoff)] pixels around the outline.
    let byte_scale = 255.0 / radius;
    let base = 255.0 * (1.0 - CUTOFF);
    let data = grid_outer
        .iter()
        .zip(&grid_inner)
        .map(|(outer, inner)| {
            let distance = outer.sqrt() - inner.sqrt();
            (base - byte_scale * distance)
                .round_ties_even()
                .clamp(0.0, 255.0) as u8
        })
        .collect();

    metrics.width = glyph_width as f32 / scale;
    metrics.height = glyph_height as f32 / scale;
    metrics.left = glyph_left as f32 / scale + LEFT_ADJUSTMENT;
    metrics.top = glyph_top as f32 / scale - TOP_ADJUSTMENT;
    Ok(Glyph {
        bitmap: GlyphBitmap {
            width: width as u32,
            height: height as u32,
            data,
        },
        metrics,
    })
}

static ALPHA_TABLE: OnceLock<[f64; 256]> = OnceLock::new();

/// Signed squared distance for each 8-bit coverage, gamma-corrected: positive for
/// mostly-empty pixels, negative for mostly-covered ones, and `-INF` for fully covered.
fn alpha_table() -> [f64; 256] {
    let mut table = [0.0; 256];
    for (alpha, entry) in table.iter_mut().enumerate() {
        let distance = 0.5 - libm::pow(alpha as f64 / 255.0, 1.0 / 2.2);
        *entry = distance * distance.abs();
    }
    table[255] = -INF;
    table
}

/// Working arrays for the 1D transform, sized for the longest line.
struct Edt1d {
    f: Vec<f64>,
    z: Vec<f64>,
    v: Vec<usize>,
}

impl Edt1d {
    fn new(length: usize) -> Self {
        Self {
            f: vec![0.0; length],
            z: vec![0.0; length + 1],
            v: vec![0; length],
        }
    }
}

/// 2D Euclidean squared distance transform (Felzenszwalb and Huttenlocher), over a `width` x `height`
/// region at (`x0`, `y0`) of a grid `grid_size` wide. The column pass skips `skip_cols` columns at each side.
#[allow(clippy::too_many_arguments)]
fn edt(
    data: &mut [f64],
    x0: usize,
    y0: usize,
    width: usize,
    height: usize,
    grid_size: usize,
    skip_cols: usize,
    scratch: &mut Edt1d,
) {
    for x in x0 + skip_cols..x0 + width - skip_cols {
        edt1d(data, y0 * grid_size + x, grid_size, height, scratch);
    }
    for y in y0..y0 + height {
        edt1d(data, y * grid_size + x0, 1, width, scratch);
    }
}

/// 1D squared distance transform.
fn edt1d(grid: &mut [f64], offset: usize, stride: usize, length: usize, scratch: &mut Edt1d) {
    let Edt1d { f, z, v } = scratch;
    v[0] = 0;
    z[0] = -INF;
    z[1] = INF;
    f[0] = grid[offset];

    let mut k = 0;
    for q in 1..length {
        let fq = grid[offset + q * stride];
        f[q] = fq;
        let q2 = (q * q) as f64;
        // Drop parabolas the new one hides, then stand it after the last that stays.
        let s = loop {
            let r = v[k];
            let s = (fq - f[r] + q2 - (r * r) as f64) / (q - r) as f64 / 2.0;
            if s > z[k] {
                k += 1;
                break s;
            }
            if k == 0 {
                break s;
            }
            k -= 1;
        };
        v[k] = q;
        z[k] = s;
        z[k + 1] = INF;
    }

    let mut k = 0;
    for q in 0..length {
        while z[k + 1] < q as f64 {
            k += 1;
        }
        let r = v[k];
        let distance = q as f64 - r as f64;
        grid[offset + q * stride] = f[r] + distance * distance;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gpu::labels::text::{shape, ShapeError, ShapeStyle, TextAnchor, TextJustify};
    use serde_json::Value;

    fn service() -> GlyphService {
        let fonts = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fonts/map");
        GlyphService::open(&fonts).unwrap()
    }

    fn style() -> ShapeStyle {
        ShapeStyle {
            max_width: Some(10.0),
            line_height: 1.2,
            letter_spacing: 0.0,
            anchor: TextAnchor::Center,
            justify: TextJustify::Auto,
            offset: [0.0, 0.0],
            uppercase: false,
        }
    }

    fn shape_with(service: &GlyphService, font: &str, text: &str) -> Result<Vec<Face>, ShapeError> {
        let lookup = |character| service.glyph(font, character).unwrap();
        let shaping = shape(text, &style(), &lookup)?;
        Ok(shaping
            .lines
            .iter()
            .flatten()
            .map(|glyph| glyph.face)
            .collect())
    }

    #[test]
    fn every_font_the_bundled_styles_name_has_a_chain() {
        let styles = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/map-styles");
        let mut named = 0;
        for entry in std::fs::read_dir(styles).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_none_or(|extension| extension != "json") {
                continue;
            }
            let style: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            for layer in style["layers"].as_array().unwrap() {
                let Some(fonts) = layer["layout"].get("text-font") else {
                    continue;
                };
                for font in fonts.as_array().unwrap() {
                    let font = font.as_str().unwrap();
                    assert!(font_chain(font).is_some(), "{}: {font}", path.display());
                    named += 1;
                }
            }
        }
        assert!(named > 50);
    }

    #[test]
    fn names_in_every_supported_script_shape_without_missing_glyphs() {
        let service = service();
        let samples = [
            ("Praha-Žižkov", Face::NotoSansRegular),
            ("Αθήνα", Face::NotoSansRegular),
            ("Москва", Face::NotoSansRegular),
            ("תל אביב-יפו", Face::NotoSansHebrewRegular),
            ("北京市", Face::CjkPack),
            ("東京都 ひらがな カタカナ", Face::CjkPack),
            ("서울특별시", Face::CjkPack),
        ];
        for (name, face) in samples {
            let faces = shape_with(&service, "Noto Sans Regular", name).unwrap();
            assert!(faces.contains(&face), "{name}");
        }
        // Bold draws Latin from the bold face and CJK from the Regular pack, thickened by the renderer.
        let bold = service.glyph("Noto Sans Bold", '北').unwrap().unwrap();
        assert!(bold.bold && bold.face == Face::CjkPack);
        assert!(!service.glyph("Noto Sans Bold", 'P').unwrap().unwrap().bold);
    }

    #[test]
    fn a_character_the_cjk_pack_lacks_drops_the_label() {
        // A rare GBK-only character, outside GB2312, JIS X 0208, Big5 and KS X 1001.
        let service = service();
        assert_eq!(
            shape_with(&service, "Noto Sans Regular", "滧"),
            Err(ShapeError::MissingGlyph('滧'))
        );
    }

    #[test]
    fn a_font_without_a_chain_is_an_error() {
        assert!(service().glyph("Open Sans Regular", 'a').is_err());
    }

    #[test]
    fn glyphs_are_distance_fields_cached_across_calls() {
        let service = service();
        let latin = service.glyph("Noto Sans Regular", 'I').unwrap().unwrap();
        let bitmap = service.bitmap(latin.face, 'I').unwrap();
        assert!(Arc::ptr_eq(
            &bitmap,
            &service.bitmap(latin.face, 'I').unwrap()
        ));

        // The bitmap is the glyph plus a 3 px border, and the distance field is 191 at the outline.
        let metrics = latin.metrics;
        assert_eq!(bitmap.width as f32, metrics.width + 6.0);
        assert_eq!(bitmap.height as f32, metrics.height + 6.0);
        let (width, height) = (bitmap.width as usize, bitmap.height as usize);
        let row = &bitmap.data[(height / 2) * width..][..width];
        assert!(row[0] < 64, "outside the border: {}", row[0]);
        assert!(row[width / 2] > 200, "inside the stem: {}", row[width / 2]);

        let cjk = service.glyph("Noto Sans Regular", '北').unwrap().unwrap();
        assert_eq!(
            service.bitmap(cjk.face, '北').unwrap().width as f32,
            cjk.metrics.width * 2.0 + 12.0
        );
    }

    #[test]
    fn a_service_over_the_files_it_serves_makes_the_same_glyphs() {
        let opened = service();
        let files = Face::ALL
            .iter()
            .map(|face| opened.font_file(face.file_name()).unwrap().to_vec())
            .collect();
        let handed = GlyphService::from_files(files).unwrap();
        for character in ['P', 'ž', 'ש', '北'] {
            let [a, b] = [&opened, &handed].map(|service| {
                service
                    .glyph("Noto Sans Regular", character)
                    .unwrap()
                    .unwrap()
            });
            assert_eq!(a, b);
            assert_eq!(
                opened.bitmap(a.face, character).unwrap(),
                handed.bitmap(b.face, character).unwrap()
            );
        }
    }
}
