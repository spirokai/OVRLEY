//! The glyph service: the bundled font files, opened once, and every glyph made from them.
//!
//! Given (font, character) keys it answers with the face that draws each character, an SDF bitmap and its
//! metrics. Export's tile threads open the files in place; the preview's decode workers have no files, so
//! they are handed the same bytes and make the same glyphs.

use super::glyphs::rasterize;
use super::layout::GlyphLookup;
use super::text::{font_chain, Face, FontGlyph, GlyphBitmap, GlyphMetrics};
use ab_glyph::{Font, FontRef};
#[cfg(not(target_arch = "wasm32"))]
use memmap2::Mmap;
use std::collections::HashMap;
#[cfg(not(target_arch = "wasm32"))]
use std::fs::File;
#[cfg(not(target_arch = "wasm32"))]
use std::path::Path;
use std::sync::{Arc, RwLock};

/// One character in the font a style layer names, as `text-font` names it.
#[derive(Clone, Copy, Debug)]
pub struct GlyphKey<'a> {
    pub font: &'a str,
    pub character: char,
}

/// What the service answers a key with: the glyph as shaping sees it, and its distance field.
#[derive(Clone, Debug)]
pub struct ServedGlyph {
    pub glyph: FontGlyph,
    pub bitmap: Arc<GlyphBitmap>,
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
            .map(|face| Ok((face, FaceFile::open(face, &directory.join(face.file_name()))?)))
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
                Ok((face, FaceFile::new(face, FontBytes::Owned(bytes.into_boxed_slice()))?))
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

    /// The glyph for each key, in order: `None` where no face in the key's font chain has the
    /// character. A font with no chain is an error, since the caller skips such a layer before asking.
    pub fn glyphs(&self, keys: &[GlyphKey]) -> Result<Vec<Option<ServedGlyph>>, String> {
        keys.iter()
            .map(|key| self.glyph(key.font, key.character))
            .collect()
    }

    /// The glyph for one character of `font`. See [`GlyphService::glyphs`].
    fn glyph(&self, font: &str, character: char) -> Result<Option<ServedGlyph>, String> {
        let chain = font_chain(font).ok_or_else(|| format!("no font chain for \"{font}\""))?;
        let Some(link) = chain
            .iter()
            .find(|link| self.faces[&link.face].font.glyph_id(character).0 != 0)
        else {
            return Ok(None);
        };
        let (metrics, bitmap) = self.made(link.face, character)?;
        Ok(Some(ServedGlyph {
            glyph: FontGlyph {
                face: link.face,
                bold: link.bold,
                metrics,
            },
            bitmap,
        }))
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
        Ok(GlyphService::glyph(self, font, character)?.map(|served| served.glyph))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gpu::labels::{shape, ShapeError, ShapeStyle, TextAnchor, TextJustify};
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
        let lookup = |character| {
            service
                .glyph(font, character)
                .unwrap()
                .map(|served| served.glyph)
        };
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
        assert!(bold.glyph.bold && bold.glyph.face == Face::CjkPack);
        assert!(
            !service
                .glyph("Noto Sans Bold", 'P')
                .unwrap()
                .unwrap()
                .glyph
                .bold
        );
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
        let again = service.glyph("Noto Sans Regular", 'I').unwrap().unwrap();
        assert!(Arc::ptr_eq(&latin.bitmap, &again.bitmap));

        // The bitmap is the glyph plus a 3 px border, and the distance field is 191 at the outline.
        let (bitmap, metrics) = (&latin.bitmap, latin.glyph.metrics);
        assert_eq!(bitmap.width as f32, metrics.width + 6.0);
        assert_eq!(bitmap.height as f32, metrics.height + 6.0);
        let (width, height) = (bitmap.width as usize, bitmap.height as usize);
        let row = &bitmap.data[(height / 2) * width..][..width];
        assert!(row[0] < 64, "outside the border: {}", row[0]);
        assert!(row[width / 2] > 200, "inside the stem: {}", row[width / 2]);

        let cjk = service.glyph("Noto Sans Regular", '北').unwrap().unwrap();
        assert_eq!(
            cjk.bitmap.width as f32,
            cjk.glyph.metrics.width * 2.0 + 12.0
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
            let [a, b] = [&opened, &handed]
                .map(|service| service.glyph("Noto Sans Regular", character).unwrap().unwrap());
            assert_eq!(a.glyph, b.glyph);
            assert_eq!(a.bitmap, b.bitmap);
        }
    }
}
