//! Migration diagnostic surface. Production widget drawing remains in ovrley_core.
use skia_safe::{
    paint::Style, surfaces, AlphaType, Color, ColorType, Data, Font, FontMgr, ImageInfo, Paint,
    Rect,
};

pub const WIDTH: u32 = 640;
pub const HEIGHT: u32 = 364;
pub const RGBA_LEN: usize = WIDTH as usize * HEIGHT as usize * 4;
pub const WIDGET_COUNT: u32 = 6;

const COLS: u32 = 2;
const MARGIN: f32 = 16.0;
const GAP: f32 = 16.0;
const BOX_WIDTH: f32 = 296.0;
const BOX_HEIGHT: f32 = 100.0;
const PAD: f32 = 14.0;
const BAR_HEIGHT: f32 = 18.0;
const BAR_MAX_WIDTH: f32 = BOX_WIDTH - PAD * 2.0;

pub struct Renderer {
    font: Font,
}

impl Renderer {
    /// Own the supplied font independently of the frontend's upload buffer.
    pub fn new(bytes: &[u8]) -> Option<Self> {
        let typeface = FontMgr::default().new_from_data(Data::new_copy(bytes), None)?;
        let mut font = Font::from_typeface(typeface, 18.0);
        font.set_subpixel(true);
        font.set_edging(skia_safe::font::Edging::AntiAlias);
        Some(Self { font })
    }

    /// Render an opaque RGBA diagnostic frame into caller-owned memory.
    /// Every widget box is drawn inside this single call so the frontend's
    /// frame timing measures the whole batch, never an individual widget.
    pub fn render(&self, pixels: &mut [u8], frame: u32) -> Option<()> {
        let info = ImageInfo::new(
            (WIDTH as i32, HEIGHT as i32),
            ColorType::RGBA8888,
            AlphaType::Premul,
            None,
        );
        let mut surface = surfaces::wrap_pixels(&info, pixels, WIDTH as usize * 4, None)?;
        let canvas = surface.canvas();
        canvas.clear(Color::from_rgb(24, 28, 36));
        let bar_width = (frame % BAR_MAX_WIDTH as u32) as f32;
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        for index in 0..WIDGET_COUNT {
            let x = MARGIN + (index % COLS) as f32 * (BOX_WIDTH + GAP);
            let y = MARGIN + (index / COLS) as f32 * (BOX_HEIGHT + GAP);
            let bounds = Rect::from_xywh(x, y, BOX_WIDTH, BOX_HEIGHT);

            paint.set_style(Style::Fill);
            paint.set_color(Color::from_rgb(34, 40, 52));
            canvas.draw_round_rect(bounds, 8.0, 8.0, &paint);

            paint.set_style(Style::Stroke);
            paint.set_stroke_width(1.0);
            paint.set_color(Color::from_argb(96, 64, 196, 255));
            canvas.draw_round_rect(bounds, 8.0, 8.0, &paint);

            paint.set_style(Style::Fill);
            paint.set_color(Color::WHITE);
            canvas.draw_str(
                "OVRLEY / Rust + Skia",
                (x + PAD, y + 30.0),
                &self.font,
                &paint,
            );
            canvas.draw_str(
                format!("Frame {frame}"),
                (x + PAD, y + 54.0),
                &self.font,
                &paint,
            );

            paint.set_color(Color::from_rgb(64, 196, 255));
            canvas.draw_rect(
                Rect::from_xywh(x + PAD, y + 68.0, bar_width, BAR_HEIGHT),
                &paint,
            );
        }
        Some(())
    }
}
