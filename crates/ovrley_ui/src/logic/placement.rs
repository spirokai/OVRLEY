use crate::{
    ItemAlignedPlacement, Placement, PlacementAlign, PlacementLogic, PlacementRect, PlacementSide,
};
use ovrley_app::lib::placement::{self as pl, Align, Rect, Side, Size};

fn rect(r: PlacementRect) -> Rect {
    Rect {
        x: r.x as f64,
        y: r.y as f64,
        width: r.width as f64,
        height: r.height as f64,
    }
}

fn size(width: f32, height: f32) -> Size {
    Size {
        width: width as f64,
        height: height as f64,
    }
}

pub(super) fn bind(logic: &PlacementLogic) {
    logic.on_place(
        |anchor, content_w, content_h, window_w, window_h, side, align, offset| {
            let side = match side {
                PlacementSide::Top => Side::Top,
                PlacementSide::Right => Side::Right,
                PlacementSide::Bottom => Side::Bottom,
                PlacementSide::Left => Side::Left,
            };
            let align = match align {
                PlacementAlign::Start => Align::Start,
                PlacementAlign::Center => Align::Center,
                PlacementAlign::End => Align::End,
            };
            let placed = pl::place(
                rect(anchor),
                size(content_w, content_h),
                size(window_w, window_h),
                side,
                align,
                offset as f64,
            );
            Placement {
                x: placed.x as f32,
                y: placed.y as f32,
                side: match placed.side {
                    Side::Top => PlacementSide::Top,
                    Side::Right => PlacementSide::Right,
                    Side::Bottom => PlacementSide::Bottom,
                    Side::Left => PlacementSide::Left,
                },
                available_height: placed.available_height as f32,
            }
        },
    );
    logic.on_item_aligned(
        |trigger, content_w, content_h, window_w, window_h, item_top, item_height| {
            let placed = pl::item_aligned(
                rect(trigger),
                size(content_w, content_h),
                size(window_w, window_h),
                item_top as f64,
                item_height as f64,
            );
            ItemAlignedPlacement {
                x: placed.x as f32,
                y: placed.y as f32,
                width: placed.width as f32,
                height: placed.height as f32,
                scroll_y: placed.scroll_y as f32,
            }
        },
    );
}
