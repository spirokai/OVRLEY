//! Popup placement for tooltips, popovers, the colour picker and select lists: what Radix's
//! popper positioning (side, alignment, side offset, flip, shift, available height) and Radix
//! Select's `item-aligned` position do for these components. Coordinates are window logical
//! pixels; the boundary is the window with no collision padding, as Radix's default.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Top,
    Right,
    Bottom,
    Left,
}

impl Side {
    fn opposite(self) -> Side {
        match self {
            Side::Top => Side::Bottom,
            Side::Bottom => Side::Top,
            Side::Left => Side::Right,
            Side::Right => Side::Left,
        }
    }

    fn is_vertical(self) -> bool {
        matches!(self, Side::Top | Side::Bottom)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Start,
    Center,
    End,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Size {
    pub width: f64,
    pub height: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub x: f64,
    pub y: f64,
    /// The side actually used, after flipping.
    pub side: Side,
    /// The height between the anchor (plus offset) and the window edge on the used side; the
    /// full window height for left and right. Select lists cap their height at it.
    pub available_height: f64,
}

/// How far content placed on `side` would overflow the window along the main axis.
fn main_axis_overflow(anchor: Rect, content: Size, window: Size, side: Side, offset: f64) -> f64 {
    let overflow = match side {
        Side::Top => content.height + offset - anchor.y,
        Side::Bottom => anchor.y + anchor.height + offset + content.height - window.height,
        Side::Left => content.width + offset - anchor.x,
        Side::Right => anchor.x + anchor.width + offset + content.width - window.width,
    };
    overflow.max(0.0)
}

/// Places `content` against `anchor` on `side` with `align`, `side_offset` away. It flips to
/// the opposite side when the content does not fit and that side overflows less, then shifts
/// along the edge to stay inside the window.
pub fn place(
    anchor: Rect,
    content: Size,
    window: Size,
    side: Side,
    align: Align,
    side_offset: f64,
) -> Placement {
    let preferred = main_axis_overflow(anchor, content, window, side, side_offset);
    let flipped = main_axis_overflow(anchor, content, window, side.opposite(), side_offset);
    let side = if preferred > 0.0 && flipped < preferred {
        side.opposite()
    } else {
        side
    };

    let aligned = |start: f64, anchor_len: f64, content_len: f64| match align {
        Align::Start => start,
        Align::Center => start + (anchor_len - content_len) / 2.0,
        Align::End => start + anchor_len - content_len,
    };
    let shift = |position: f64, content_len: f64, window_len: f64| {
        position.min(window_len - content_len).max(0.0)
    };

    let (x, y) = match side {
        Side::Top => (
            aligned(anchor.x, anchor.width, content.width),
            anchor.y - side_offset - content.height,
        ),
        Side::Bottom => (
            aligned(anchor.x, anchor.width, content.width),
            anchor.y + anchor.height + side_offset,
        ),
        Side::Left => (
            anchor.x - side_offset - content.width,
            aligned(anchor.y, anchor.height, content.height),
        ),
        Side::Right => (
            anchor.x + anchor.width + side_offset,
            aligned(anchor.y, anchor.height, content.height),
        ),
    };
    let (x, y) = if side.is_vertical() {
        (shift(x, content.width, window.width), y)
    } else {
        (x, shift(y, content.height, window.height))
    };
    let available_height = match side {
        Side::Top => anchor.y - side_offset,
        Side::Bottom => window.height - (anchor.y + anchor.height + side_offset),
        Side::Left | Side::Right => window.height,
    };
    Placement {
        x,
        y,
        side,
        available_height: available_height.max(0.0),
    }
}

/// Margin Radix Select keeps between an `item-aligned` list and the window edges.
pub const ITEM_ALIGNED_MARGIN: f64 = 10.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ItemAligned {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    /// How far the list is scrolled so the selected item stays over the trigger.
    pub scroll_y: f64,
}

/// Radix Select's `item-aligned` position: the list is at least as wide as the trigger, starts
/// at the trigger's left edge, and is placed so the selected item lands over the trigger.
/// Where the window is too short, the list is clamped to the margins and scrolled instead.
///
/// `content` is the list's natural size; `item_top` and `item_height` locate the selected item
/// inside it.
pub fn item_aligned(
    trigger: Rect,
    content: Size,
    window: Size,
    item_top: f64,
    item_height: f64,
) -> ItemAligned {
    let width = content.width.max(trigger.width);
    let x = trigger
        .x
        .min(window.width - ITEM_ALIGNED_MARGIN - width)
        .max(ITEM_ALIGNED_MARGIN);
    let height = content
        .height
        .min(window.height - 2.0 * ITEM_ALIGNED_MARGIN);
    let max_scroll = content.height - height;
    let item_center = item_top + item_height / 2.0;
    let trigger_center = trigger.y + trigger.height / 2.0;
    let desired_y = trigger_center - item_center;
    let y = desired_y
        .min(window.height - ITEM_ALIGNED_MARGIN - height)
        .max(ITEM_ALIGNED_MARGIN);
    // The item is still centred on the trigger when the list cannot move far enough up.
    let scroll_y = (item_center - (trigger_center - y)).clamp(0.0, max_scroll);
    ItemAligned {
        x,
        y,
        width,
        height,
        scroll_y,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WINDOW: Size = Size {
        width: 1000.0,
        height: 700.0,
    };

    fn rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
        Rect {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn places_on_each_side_with_alignment_and_offset() {
        let anchor = rect(400.0, 300.0, 100.0, 40.0);
        let content = Size {
            width: 200.0,
            height: 100.0,
        };
        let top = place(anchor, content, WINDOW, Side::Top, Align::Center, 8.0);
        assert_eq!((top.x, top.y, top.side), (350.0, 192.0, Side::Top));
        let bottom = place(anchor, content, WINDOW, Side::Bottom, Align::Start, 4.0);
        assert_eq!((bottom.x, bottom.y), (400.0, 344.0));
        assert_eq!(bottom.available_height, 700.0 - 344.0);
        let right = place(anchor, content, WINDOW, Side::Right, Align::End, 8.0);
        assert_eq!((right.x, right.y), (508.0, 240.0));
        let left = place(anchor, content, WINDOW, Side::Left, Align::Center, 8.0);
        assert_eq!((left.x, left.y), (192.0, 270.0));
    }

    #[test]
    fn flips_when_the_preferred_side_does_not_fit() {
        let near_top = rect(400.0, 20.0, 100.0, 40.0);
        let placed = place(
            near_top,
            Size {
                width: 200.0,
                height: 100.0,
            },
            WINDOW,
            Side::Top,
            Align::Center,
            8.0,
        );
        assert_eq!((placed.side, placed.y), (Side::Bottom, 68.0));
    }

    #[test]
    fn keeps_the_preferred_side_when_neither_fits_better() {
        let tall = Size {
            width: 100.0,
            height: 650.0,
        };
        let placed = place(
            rect(400.0, 340.0, 100.0, 20.0),
            tall,
            WINDOW,
            Side::Bottom,
            Align::Start,
            0.0,
        );
        assert_eq!(placed.side, Side::Bottom);
    }

    #[test]
    fn shifts_along_the_edge_to_stay_inside_the_window() {
        let at_right_edge = rect(950.0, 300.0, 40.0, 20.0);
        let placed = place(
            at_right_edge,
            Size {
                width: 200.0,
                height: 50.0,
            },
            WINDOW,
            Side::Bottom,
            Align::Center,
            4.0,
        );
        assert_eq!(placed.x, 800.0);
        let at_left_edge = rect(5.0, 300.0, 40.0, 20.0);
        assert_eq!(
            place(
                at_left_edge,
                Size {
                    width: 200.0,
                    height: 50.0
                },
                WINDOW,
                Side::Top,
                Align::End,
                4.0
            )
            .x,
            0.0
        );
    }

    #[test]
    fn item_aligned_puts_the_selected_item_over_the_trigger() {
        let trigger = rect(100.0, 300.0, 150.0, 36.0);
        let placed = item_aligned(
            trigger,
            Size {
                width: 120.0,
                height: 200.0,
            },
            WINDOW,
            80.0,
            28.0,
        );
        assert_eq!(placed.width, 150.0);
        assert_eq!(placed.x, 100.0);
        assert_eq!(placed.y + 80.0 + 14.0, 318.0);
        assert_eq!(placed.scroll_y, 0.0);
    }

    #[test]
    fn item_aligned_clamps_to_the_margins_and_scrolls() {
        let trigger = rect(100.0, 20.0, 150.0, 36.0);
        let placed = item_aligned(
            trigger,
            Size {
                width: 200.0,
                height: 2000.0,
            },
            WINDOW,
            900.0,
            28.0,
        );
        assert_eq!(placed.height, 680.0);
        assert_eq!(placed.y, 10.0);
        assert_eq!(placed.y + 900.0 + 14.0 - placed.scroll_y, 38.0);
    }
}
