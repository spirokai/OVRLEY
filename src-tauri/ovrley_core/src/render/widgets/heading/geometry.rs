//! Heading tape geometry: tick positions, label placement, and scroll offsets.
//!
//! Pure functions that compute which ticks and labels are visible at a given
//! heading value, widget dimensions, and scale. These are shared conceptually
//! between the Skia backend (cached tape image) and the frontend SVG preview.

pub const LABEL_DESCENT_PCT: f32 = 0.25;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HeadingTapeLayout {
    pub body_y: f32,
    pub body_height: f32,
    pub tick_scale_height: f32,
    pub total_height: f32,
    pub has_top_chevron: bool,
    pub has_bottom_chevron: bool,
}

pub fn heading_tape_has_chevron(
    show_indicator: bool,
    indicator_style: &str,
    indicator_placement: &str,
    placement: &str,
) -> bool {
    show_indicator
        && indicator_style == "chevron"
        && (indicator_placement == placement || indicator_placement == "both")
}

pub fn heading_tape_layout(
    tick_scale_height: f32,
    show_indicator: bool,
    indicator_style: &str,
    indicator_placement: &str,
    indicator_size: f32,
    major_tick_length_pct: f32,
    label_offset: f32,
    font_size: f32,
) -> HeadingTapeLayout {
    let tick_scale_height = tick_scale_height.max(1.0);
    let body_height = heading_tape_body_height(
        tick_scale_height,
        major_tick_length_pct,
        label_offset,
        font_size,
    );
    let indicator_size = indicator_size.max(0.0);
    let gap = indicator_size * 0.5;
    let has_top_chevron =
        heading_tape_has_chevron(show_indicator, indicator_style, indicator_placement, "top");
    let has_bottom_chevron = heading_tape_has_chevron(
        show_indicator,
        indicator_style,
        indicator_placement,
        "bottom",
    );
    let top_slot = if has_top_chevron {
        indicator_size + gap
    } else {
        0.0
    };
    let bottom_slot = if has_bottom_chevron {
        indicator_size + gap
    } else {
        0.0
    };

    HeadingTapeLayout {
        body_y: top_slot,
        body_height,
        tick_scale_height,
        total_height: top_slot + body_height + bottom_slot,
        has_top_chevron,
        has_bottom_chevron,
    }
}

pub fn heading_tick_position(
    body_height: f32,
    major_tick_length_pct: f32,
    minor_tick_length_pct: f32,
    tick_alignment: &str,
    is_major: bool,
) -> (f32, f32) {
    let major_length = body_height * major_tick_length_pct / 100.0;
    let minor_length = body_height * minor_tick_length_pct / 100.0;
    let length = if is_major { major_length } else { minor_length };
    let top = if !is_major && tick_alignment == "centered" {
        (major_length - minor_length) / 2.0
    } else {
        0.0
    };

    (top, length)
}

pub fn heading_label_baseline(
    body_height: f32,
    major_tick_length_pct: f32,
    label_offset: f32,
    font_size: f32,
) -> f32 {
    body_height * major_tick_length_pct / 100.0 + label_offset + font_size
}

pub fn heading_label_bottom(
    body_height: f32,
    major_tick_length_pct: f32,
    label_offset: f32,
    font_size: f32,
) -> f32 {
    heading_label_baseline(body_height, major_tick_length_pct, label_offset, font_size)
        + font_size * LABEL_DESCENT_PCT
}

pub fn heading_tape_body_height(
    body_height: f32,
    major_tick_length_pct: f32,
    label_offset: f32,
    font_size: f32,
) -> f32 {
    heading_label_bottom(body_height, major_tick_length_pct, label_offset, font_size)
}

/// A single visible tick on the tape.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TapeTick {
    /// Degree position (0–360) on the compass.
    pub degree: f32,
    /// Pixel x-position relative to the tape image origin.
    pub x: f32,
    /// Whether this tick is at a cardinal/intercardinal position (45° multiple).
    pub is_cardinal: bool,
    /// Whether this tick is a major tick (at `major_tick_interval` multiples).
    pub is_major: bool,
}

/// A single visible label on the tape.
#[derive(Clone, Debug, PartialEq)]
pub struct TapeLabel {
    /// Degree position (0–360) on the compass.
    pub degree: f32,
    /// Pixel x-position relative to the tape image origin.
    pub x: f32,
    /// Text to display (e.g. "N", "NE", "30", "60").
    pub text: String,
    /// Whether this is a major label (takes priority over minor numeric labels).
    pub is_major_label: bool,
}

/// Cardinal direction labels at 45° multiples.
const CARDINAL_LABELS: &[(f32, &str)] = &[
    (0.0, "N"),
    (45.0, "NE"),
    (90.0, "E"),
    (135.0, "SE"),
    (180.0, "S"),
    (225.0, "SW"),
    (270.0, "W"),
    (315.0, "NW"),
];

/// Checks if a degree value is at a cardinal/intercardinal position (45° multiple).
pub fn is_cardinal_degree(degree: f32) -> bool {
    CARDINAL_LABELS
        .iter()
        .any(|(cardinal, _)| (degree - cardinal).abs() < 0.01)
}

/// Returns the cardinal label for a degree, or None if not cardinal.
pub fn cardinal_label_for_degree(degree: f32) -> Option<&'static str> {
    CARDINAL_LABELS
        .iter()
        .find(|(cardinal, _)| (degree - cardinal).abs() < 0.01)
        .map(|(_, label)| *label)
}

/// Computes the scroll offset in pixels for a given heading.
///
/// The active heading should sit under the widget's center indicator, so the
/// tape offset is left-anchored heading minus half the visible width.
pub fn heading_offset(heading: f32, pixels_per_degree: f32, width: f32) -> f32 {
    heading * pixels_per_degree - width / 2.0
}

/// Computes which ticks are visible within the widget bounds.
///
/// The tape image is 360 × pixels_per_degree wide and repeats via TileMode::Repeat.
/// We need to find all tick degree positions whose pixel x falls within
/// [0, width) after accounting for the scroll offset.
///
/// # Arguments
/// * `heading` - Current heading in degrees (0–360)
/// * `pixels_per_degree` - Horizontal scale
/// * `width` - Widget width in pixels
/// * `major_tick_interval` - Degrees between major ticks (default 15)
/// * `minor_ticks_per_major` - Subdivisions between majors (default 3)
/// * `show_major_ticks` - Whether to include major ticks
/// * `show_minor_ticks` - Whether to include minor ticks
pub fn visible_ticks(
    heading: f32,
    pixels_per_degree: f32,
    width: f32,
    major_tick_interval: u32,
    minor_ticks_per_major: u32,
    show_major_ticks: bool,
    show_minor_ticks: bool,
) -> Vec<TapeTick> {
    if pixels_per_degree <= 0.0 || width <= 0.0 {
        return Vec::new();
    }

    let tape_width = 360.0 * pixels_per_degree;
    let offset = heading * pixels_per_degree;
    let minor_interval = major_tick_interval as f32 / minor_ticks_per_major as f32;
    let mut degrees: Vec<f32> = CARDINAL_LABELS.iter().map(|(degree, _)| *degree).collect();

    if major_tick_interval > 0 && minor_ticks_per_major > 0 && minor_interval > 0.0 {
        let mut degree = 0.0_f32;
        while degree < 360.0 {
            degrees.push((degree * 1000.0).round() / 1000.0);
            degree += minor_interval;
        }
    }

    degrees.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    degrees.dedup_by(|a, b| (*a - *b).abs() < 0.01);

    let mut ticks = Vec::new();

    for degree in degrees {
        let is_cardinal = is_cardinal_degree(degree);
        let is_major = is_cardinal || (degree % major_tick_interval as f32).abs() < 0.01;
        let is_minor = !is_major;

        let show = is_cardinal || (is_major && show_major_ticks) || (is_minor && show_minor_ticks);

        if show {
            // Compute pixel position in the tape image
            let tape_x = degree * pixels_per_degree;
            // Apply scroll offset and wrap into [0, tape_width)
            let wrapped_x = ((tape_x - offset) % tape_width + tape_width) % tape_width;

            // Check if this tick falls within the visible widget width
            if wrapped_x < width {
                ticks.push(TapeTick {
                    degree,
                    x: wrapped_x,
                    is_cardinal,
                    is_major,
                });
            }
        }
    }

    ticks
}

/// Computes labels for visible tick positions, with cardinal priority override.
///
/// Cardinal labels (N/NE/E/SE/S/SW/W/NW) at 45° multiples take priority over
/// numeric labels at the same position. Both label types share a single row
/// below the ticks.
///
/// # Arguments
/// * `ticks` - Pre-computed visible ticks from `visible_ticks`
/// * `show_minor_labels` - Whether to show degree numbers
/// * `show_major_labels` - Whether to show cardinal letters
pub fn visible_labels(
    ticks: &[TapeTick],
    show_minor_labels: bool,
    show_major_labels: bool,
) -> Vec<TapeLabel> {
    let mut labels = Vec::new();

    for tick in ticks {
        if tick.is_cardinal {
            if let Some(text) = cardinal_label_for_degree(tick.degree) {
                labels.push(TapeLabel {
                    degree: tick.degree,
                    x: tick.x,
                    text: text.to_string(),
                    is_major_label: true,
                });
            }
        } else if tick.is_major && show_major_labels {
            labels.push(TapeLabel {
                degree: tick.degree,
                x: tick.x,
                text: format!("{}", tick.degree as u32),
                is_major_label: false,
            });
        } else if !tick.is_major && show_minor_labels {
            labels.push(TapeLabel {
                degree: tick.degree,
                x: tick.x,
                text: format!("{}", tick.degree as u32),
                is_major_label: false,
            });
        }
    }

    labels
}

// ── Indicator geometry ────────────────────────────────────────────────

/// A single triangle point for indicator shapes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IndicatorPoint {
    pub x: f32,
    pub y: f32,
}

/// Computes chevron triangle vertices for a given placement edge.
///
/// The chevron is an isosceles triangle pointing toward the tape center.
/// At the top edge it points downward; at the bottom edge it points upward.
///
/// # Arguments
/// * `center_x` - Horizontal center of the widget
/// * `edge_y` - Y coordinate of the edge (top or bottom of widget)
/// * `size` - Chevron height in pixels
/// * `pointing_down` - true for top placement (points toward tape center)
pub fn chevron_vertices(
    center_x: f32,
    edge_y: f32,
    size: f32,
    pointing_down: bool,
) -> [IndicatorPoint; 3] {
    let half_base = size * 0.6;
    if pointing_down {
        // Top edge: triangle points down
        [
            IndicatorPoint {
                x: center_x - half_base,
                y: edge_y,
            },
            IndicatorPoint {
                x: center_x + half_base,
                y: edge_y,
            },
            IndicatorPoint {
                x: center_x,
                y: edge_y + size,
            },
        ]
    } else {
        // Bottom edge: triangle points up
        [
            IndicatorPoint {
                x: center_x - half_base,
                y: edge_y,
            },
            IndicatorPoint {
                x: center_x + half_base,
                y: edge_y,
            },
            IndicatorPoint {
                x: center_x,
                y: edge_y - size,
            },
        ]
    }
}
