//! Scene geometry of presented widgets: rotated content boxes, their corners and hit tests, the
//! ports of `widgetGestureGeometry.js` (`resizeAnchor`, `groupContentBounds`, `normalizeRotation`,
//! `widgetResizeGeometry`) and the snapping that `moveable-behaviour.md` records (Q4).

use ovrley_widgets::preview_instance::PresentedGeometry;
use serde_json::{json, Map, Value};

/// An axis-aligned rectangle in scene pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub left: f64,
    pub top: f64,
    pub width: f64,
    pub height: f64,
}

impl Rect {
    pub fn right(&self) -> f64 {
        self.left + self.width
    }
    pub fn bottom(&self) -> f64 {
        self.top + self.height
    }
    pub fn center_x(&self) -> f64 {
        self.left + self.width / 2.0
    }
    pub fn center_y(&self) -> f64 {
        self.top + self.height / 2.0
    }
    pub fn translated(&self, dx: f64, dy: f64) -> Self {
        Self {
            left: self.left + dx,
            top: self.top + dy,
            ..*self
        }
    }
    /// `rectanglesIntersect`: touching edges count.
    pub fn intersects(&self, other: &Rect) -> bool {
        !(self.right() < other.left
            || other.right() < self.left
            || self.bottom() < other.top
            || other.bottom() < self.top)
    }
    /// The bounding box of points.
    pub fn bounding(points: &[(f64, f64)]) -> Self {
        let left = points
            .iter()
            .map(|point| point.0)
            .fold(f64::INFINITY, f64::min);
        let top = points
            .iter()
            .map(|point| point.1)
            .fold(f64::INFINITY, f64::min);
        let right = points
            .iter()
            .map(|point| point.0)
            .fold(f64::NEG_INFINITY, f64::max);
        let bottom = points
            .iter()
            .map(|point| point.1)
            .fold(f64::NEG_INFINITY, f64::max);
        Self {
            left,
            top,
            width: right - left,
            height: bottom - top,
        }
    }
}

/// A widget's content box: its rectangle before rotation, the rotation and its origin (relative to
/// the rectangle's top-left corner).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ContentBox {
    pub rect: Rect,
    pub rotation: f64,
    pub origin: (f64, f64),
}

impl ContentBox {
    pub fn of(geometry: &PresentedGeometry) -> Self {
        let content = &geometry.widget.content;
        let origin = geometry.rotation_origin.unwrap_or([0.0, 0.0]);
        Self {
            rect: Rect {
                left: f64::from(content.left),
                top: f64::from(content.top),
                width: f64::from(content.width),
                height: f64::from(content.height),
            },
            rotation: f64::from(geometry.rotation.unwrap_or(0.0)),
            origin: (f64::from(origin[0]), f64::from(origin[1])),
        }
    }

    /// The scene point at fractions `(fx, fy)` of the box, rotated about its origin.
    pub fn point(&self, fx: f64, fy: f64) -> (f64, f64) {
        rotated_point(&self.rect, (fx, fy), self.rotation, self.origin)
    }

    /// The four corners, clockwise from the top-left.
    pub fn corners(&self) -> [(f64, f64); 4] {
        [
            self.point(0.0, 0.0),
            self.point(1.0, 0.0),
            self.point(1.0, 1.0),
            self.point(0.0, 1.0),
        ]
    }

    /// The axis-aligned box of the rotated corners (what `getBoundingClientRect` measures).
    pub fn bounds(&self) -> Rect {
        Rect::bounding(&self.corners())
    }

    /// Whether a scene point is inside the rotated box.
    pub fn contains(&self, point: (f64, f64)) -> bool {
        let pivot = (
            self.rect.left + self.origin.0,
            self.rect.top + self.origin.1,
        );
        let (sin, cos) = (-self.rotation).to_radians().sin_cos();
        let (dx, dy) = (point.0 - pivot.0, point.1 - pivot.1);
        let local = (pivot.0 + dx * cos - dy * sin, pivot.1 + dx * sin + dy * cos);
        local.0 >= self.rect.left
            && local.0 <= self.rect.right()
            && local.1 >= self.rect.top
            && local.1 <= self.rect.bottom()
    }
}

fn rotated_point(
    rect: &Rect,
    fraction: (f64, f64),
    rotation: f64,
    origin: (f64, f64),
) -> (f64, f64) {
    let (sin, cos) = rotation.to_radians().sin_cos();
    let x = rect.width * fraction.0 - origin.0;
    let y = rect.height * fraction.1 - origin.1;
    (
        rect.left + origin.0 + x * cos - y * sin,
        rect.top + origin.1 + x * sin + y * cos,
    )
}

/// `resizeAnchor`: the fixed corner opposite a handle `direction` (each -1, 0 or 1) and its scene
/// point.
pub fn resize_anchor(
    rect: &Rect,
    direction: (f64, f64),
    rotation: f64,
    origin: (f64, f64),
) -> ([f64; 2], (f64, f64)) {
    let fixed = [(1.0 - direction.0) / 2.0, (1.0 - direction.1) / 2.0];
    (
        fixed,
        rotated_point(rect, (fixed[0], fixed[1]), rotation, origin),
    )
}

/// `groupContentBounds`: the box around every widget's rotated content.
pub fn group_bounds(boxes: &[ContentBox]) -> Rect {
    let points: Vec<(f64, f64)> = boxes.iter().flat_map(ContentBox::corners).collect();
    Rect::bounding(&points)
}

/// `normalizeRotation`: an angle in [0, 360).
pub fn normalize_rotation(rotation: f64) -> f64 {
    ((rotation % 360.0) + 360.0) % 360.0
}

/// Families whose resize anchor follows their rotation (`resizeAnchor`'s rotated callers).
pub fn rotates_anchor(kind: &str) -> bool {
    matches!(kind, "course" | "elevation" | "diving_profile" | "raster")
}

/// `widgetResizeGeometry`: framed families resize to a size, the others by a factor of the width.
pub fn resize_geometry(
    kind: &str,
    data: &Map<String, Value>,
    start: &Rect,
    width: f64,
    height: f64,
) -> Map<String, Value> {
    let display = data
        .get("display_type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let sized = matches!(
        kind,
        "backdrop" | "raster" | "elevation" | "diving_profile" | "rolling_chart" | "map"
    ) || matches!(
        display,
        "altitude_tape" | "heading_tape" | "heart_zones" | "linear"
    );
    let value = if sized {
        json!({ "size": [width.max(0.01), height.max(0.01)] })
    } else {
        json!({ "factor": (width / start.width).max(0.01) })
    };
    value.as_object().expect("an object").clone()
}

/// `rotationPatch`: course and raster rotate by whole degrees about their gesture-start content
/// centre; others keep their position.
pub fn rotation_patch(
    kind: &str,
    data: &Map<String, Value>,
    rotation: f64,
    content: &ContentBox,
) -> Map<String, Value> {
    let number = |key: &str| data.get(key).and_then(Value::as_f64).unwrap_or_default();
    let centered = kind == "course" || kind == "raster";
    let rotation = normalize_rotation(if centered {
        ovrley_widgets::model::document::js_round(rotation)
    } else {
        rotation
    });
    let (mut x, mut y) = (number("x"), number("y"));
    if centered {
        let center = (
            content.rect.left + content.origin.0,
            content.rect.top + content.origin.1,
        );
        let (sin, cos) = (rotation - number("rotation")).to_radians().sin_cos();
        let (dx, dy) = (x - center.0, y - center.1);
        x = center.0 + dx * cos - dy * sin;
        y = center.1 + dx * sin + dy * cos;
    }
    let mut patch = Map::new();
    patch.insert("x".into(), ovrley_widgets::model::document::number(x));
    patch.insert("y".into(), ovrley_widgets::model::document::number(y));
    patch.insert(
        "rotation".into(),
        ovrley_widgets::model::document::number(rotation),
    );
    patch
}

/// The snap distance Moveable uses (`snapHorizontalThreshold`, `snapVerticalThreshold`): 5 scene
/// px, not OVRLEY's intended 8 screen px (Phase 3 findings, Q4).
pub const SNAP_THRESHOLD: f64 = 5.0;
/// Guides draw while the box sits within this distance of a line (`snapRenderThreshold`).
pub const SNAP_RENDER_THRESHOLD: f64 = 1.0;

/// A guideline: a scene or grid line spanning the scene, or an unselected widget's edge or centre
/// spanning that widget.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Guideline {
    /// A vertical line at x (true) or a horizontal line at y.
    pub vertical: bool,
    pub position: f64,
    /// The widget the line belongs to; `None` for scene and grid lines.
    pub element: Option<Rect>,
}

fn round_tenth(value: f64) -> f64 {
    (value * 10.0).round() / 10.0
}

/// The guidelines of a gesture: element lines (each unselected widget's left, right, centre, then
/// top, bottom, middle), then the scene's horizontal and vertical lines (`getGridGuidelines`).
pub fn guidelines(elements: &[Rect], scene: (f64, f64), grid: Option<f64>) -> Vec<Guideline> {
    let mut lines = Vec::new();
    for element in elements {
        for x in [element.left, element.right(), element.center_x()] {
            lines.push(Guideline {
                vertical: true,
                position: round_tenth(x),
                element: Some(*element),
            });
        }
        for y in [element.top, element.bottom(), element.center_y()] {
            lines.push(Guideline {
                vertical: false,
                position: round_tenth(y),
                element: Some(*element),
            });
        }
    }
    let axis = |max: f64| {
        let mut values = vec![0.0, max / 2.0, max];
        if let Some(grid) = grid {
            let mut value = grid;
            while value < max {
                values.push(value);
                value += grid;
            }
        }
        values.sort_by(f64::total_cmp);
        values.dedup();
        values
    };
    for y in axis(scene.1) {
        lines.push(Guideline {
            vertical: false,
            position: round_tenth(y),
            element: None,
        });
    }
    for x in axis(scene.0) {
        lines.push(Guideline {
            vertical: true,
            position: round_tenth(x),
            element: None,
        });
    }
    lines
}

/// `checkSnap` for one axis: the correction that puts the candidate nearest to a line on it, or
/// none. Ties go to the earlier candidate, then the earlier line.
pub fn snap_axis(candidates: &[f64], lines: &[Guideline], vertical: bool) -> Option<f64> {
    let mut best: Option<(f64, f64)> = None;
    for value in candidates {
        for line in lines.iter().filter(|line| line.vertical == vertical) {
            let offset = value - line.position;
            let distance = offset.abs();
            if distance <= SNAP_THRESHOLD && best.is_none_or(|(_, current)| distance < current) {
                best = Some((-offset, distance));
            }
        }
    }
    best.map(|(correction, _)| correction)
}

/// How a guide line draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GuideStyle {
    /// A scene or grid line, 1 px.
    Normal,
    /// A widget's line or the box's own snapped line, 2 px.
    Bold,
    /// A connector between the box and an aligned widget, dashed, in ice.
    Dashed,
}

/// A guide segment in scene pixels, with its distance label (connectors only).
#[derive(Clone, Debug, PartialEq)]
pub struct Guide {
    pub from: (f64, f64),
    pub to: (f64, f64),
    pub style: GuideStyle,
    pub label: Option<String>,
}

/// The guides of a box that sits on lines (within 1 scene px), for the given candidate values.
pub fn guides(
    target: &Rect,
    xs: &[f64],
    ys: &[f64],
    lines: &[Guideline],
    scene: (f64, f64),
) -> Vec<Guide> {
    let mut drawn = Vec::new();
    for line in lines {
        let values = if line.vertical { xs } else { ys };
        if !values
            .iter()
            .any(|value| (value - line.position).abs() <= SNAP_RENDER_THRESHOLD)
        {
            continue;
        }
        let p = line.position;
        match line.element {
            None => drawn.push(if line.vertical {
                Guide {
                    from: (p, 0.0),
                    to: (p, scene.1),
                    style: GuideStyle::Normal,
                    label: None,
                }
            } else {
                Guide {
                    from: (0.0, p),
                    to: (scene.0, p),
                    style: GuideStyle::Normal,
                    label: None,
                }
            }),
            Some(element) => {
                let (element_start, element_end, target_start, target_end) = if line.vertical {
                    (element.top, element.bottom(), target.top, target.bottom())
                } else {
                    (element.left, element.right(), target.left, target.right())
                };
                let segment = |start: f64, end: f64, style: GuideStyle, label: Option<String>| {
                    if line.vertical {
                        Guide {
                            from: (p, start),
                            to: (p, end),
                            style,
                            label,
                        }
                    } else {
                        Guide {
                            from: (start, p),
                            to: (end, p),
                            style,
                            label,
                        }
                    }
                };
                drawn.push(segment(element_start, element_end, GuideStyle::Bold, None));
                drawn.push(segment(target_start, target_end, GuideStyle::Bold, None));
                // A connector to a widget entirely before or after the box, labelled with the gap.
                let gap = if element_end < target_start {
                    Some((element_end, target_start))
                } else if target_end < element_start {
                    Some((target_end, element_start))
                } else {
                    None
                };
                if let Some((start, end)) = gap {
                    let length = (end - start).abs().round();
                    let label = (length > 0.0).then(|| format!("{length}"));
                    drawn.push(segment(start, end, GuideStyle::Dashed, label));
                }
            }
        }
    }
    drawn.dedup();
    drawn
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(left: f64, top: f64, width: f64, height: f64) -> Rect {
        Rect {
            left,
            top,
            width,
            height,
        }
    }

    #[test]
    fn rotated_boxes_hit_test_and_bound_by_their_corners() {
        let content = ContentBox {
            rect: rect(0.0, 0.0, 100.0, 20.0),
            rotation: 90.0,
            origin: (50.0, 10.0),
        };
        assert!(
            content.contains((50.0, 50.0)),
            "rotated upright, the box covers its centre column"
        );
        assert!(
            !content.contains((95.0, 10.0)),
            "and no longer its old right end"
        );
        let bounds = content.bounds();
        assert!((bounds.width - 20.0).abs() < 1e-9 && (bounds.height - 100.0).abs() < 1e-9);
    }

    #[test]
    fn anchors_are_the_corner_opposite_the_handle() {
        let (fixed, point) =
            resize_anchor(&rect(10.0, 20.0, 100.0, 50.0), (1.0, 1.0), 0.0, (0.0, 0.0));
        assert_eq!((fixed, point), ([0.0, 0.0], (10.0, 20.0)));
        let (fixed, point) =
            resize_anchor(&rect(10.0, 20.0, 100.0, 50.0), (1.0, 0.0), 0.0, (0.0, 0.0));
        assert_eq!((fixed, point), ([0.0, 0.5], (10.0, 45.0)));
    }

    #[test]
    fn snapping_picks_the_nearest_line_within_five_scene_pixels() {
        let lines = guidelines(&[rect(200.0, 0.0, 50.0, 50.0)], (1000.0, 500.0), None);
        assert_eq!(
            snap_axis(&[197.0, 247.0, 222.0], &lines, true),
            Some(3.0),
            "the left edge snaps to the widget's left"
        );
        assert_eq!(
            snap_axis(&[490.0], &lines, true),
            None,
            "the scene centre is 10 px away"
        );
        assert_eq!(snap_axis(&[496.0], &lines, true), Some(4.0));
        let grid = guidelines(&[], (1000.0, 500.0), Some(30.0));
        assert_eq!(
            snap_axis(&[62.0], &grid, true),
            Some(-2.0),
            "grid lines join while snap is on"
        );
    }

    #[test]
    fn guides_draw_scene_lines_and_widget_connectors() {
        let element = rect(0.0, 0.0, 50.0, 50.0);
        let lines = guidelines(&[element], (1000.0, 500.0), None);
        let target = rect(100.0, 0.0, 40.0, 30.0);
        let drawn = guides(
            &target,
            &[100.0, 140.0, 120.0],
            &[0.0, 30.0, 15.0],
            &lines,
            (1000.0, 500.0),
        );
        assert!(drawn.iter().any(|guide| guide.style == GuideStyle::Normal
            && guide.from == (0.0, 0.0)
            && guide.to == (1000.0, 0.0)));
        let connector = drawn
            .iter()
            .find(|guide| guide.style == GuideStyle::Dashed)
            .unwrap();
        assert_eq!(
            (connector.from, connector.to, connector.label.as_deref()),
            ((50.0, 0.0), (100.0, 0.0), Some("50"))
        );
    }

    #[test]
    fn rotation_keeps_course_and_raster_centred_and_whole() {
        let data = serde_json::json!({"x": 0, "y": 0, "rotation": 0})
            .as_object()
            .unwrap()
            .clone();
        let content = ContentBox {
            rect: rect(0.0, 0.0, 100.0, 100.0),
            rotation: 0.0,
            origin: (50.0, 50.0),
        };
        let patch = rotation_patch("raster", &data, 89.6, &content);
        assert_eq!(patch["rotation"], 90);
        assert!(
            (patch["x"].as_f64().unwrap() - 100.0).abs() < 1e-9
                && patch["y"].as_f64().unwrap().abs() < 1e-9
        );
        assert_eq!(
            rotation_patch("raster", &data, -10.2, &content)["rotation"],
            350
        );
    }

    #[test]
    fn framed_families_resize_to_a_size_and_others_by_a_factor() {
        let start = rect(0.0, 0.0, 100.0, 50.0);
        let none = Map::new();
        assert_eq!(
            resize_geometry("backdrop", &none, &start, 200.0, 80.0),
            serde_json::json!({"size": [200.0, 80.0]})
                .as_object()
                .unwrap()
                .clone()
        );
        assert_eq!(
            resize_geometry("speed", &none, &start, 150.0, 80.0)["factor"],
            1.5
        );
    }
}
