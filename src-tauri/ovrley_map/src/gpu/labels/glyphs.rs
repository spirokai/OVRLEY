//! The native signed-distance-field glyph rasterizer the glyph service makes bitmaps with.
//!
//! The SDF is made from the outline's coverage by a Euclidean distance transform with radius 8 and
//! cutoff 0.25 at 24 px with a 3 px buffer, the values the label shader's constants assume. Faces
//! with a `texture_scale` of 2 (CJK) are made at twice that.
//!
//! Everything here is pure Rust on plain IEEE arithmetic, so native and WASM make the same bytes. The
//! one transcendental (the coverage gamma table) goes through `libm`, not the platform's `pow`.

use super::text::{GlyphBitmap, GlyphMetrics, BUFFER, ONE_EM};
use ab_glyph::{point, Font, PxScale, ScaleFont};
use std::sync::OnceLock;

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

/// A glyph as the atlas and the layout need it.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Glyph {
    pub bitmap: GlyphBitmap,
    /// In glyph units (24 px em), whatever resolution the bitmap is.
    pub metrics: GlyphMetrics,
}

/// Rasterizes one character of `font` to an SDF bitmap. The caller has checked `font` has it.
pub(super) fn rasterize(
    font: &impl Font,
    character: char,
    texture_scale: u32,
) -> Result<Glyph, String> {
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
