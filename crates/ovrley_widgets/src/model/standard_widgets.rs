//! Runtime typography and icon metadata, read from the compiled widget catalog.

use super::catalog::catalog;
pub use super::catalog::ChartLimits;
use ovrley_map::MapStyleId;
use std::sync::OnceLock;

/// Shared weight for text outside label widgets.
pub fn widget_font_weight() -> f32 {
    catalog().widget_font_weight
}

pub fn time_icon_asset_file() -> &'static str {
    catalog()
        .require_widget("time")
        .expect("the catalog defines time")
        .icon
        .asset_file
        .as_deref()
        .expect("time icon must declare assetFile")
}

pub fn chart_limits() -> ChartLimits {
    catalog()
        .require_widget("rolling_chart")
        .expect("the catalog defines rolling_chart")
        .chart_limits
        .expect("rolling_chart must declare chartLimits")
}

/// The style a new map widget draws, which every build must install.
pub fn default_map_style() -> &'static MapStyleId {
    static STYLE: OnceLock<MapStyleId> = OnceLock::new();
    STYLE.get_or_init(|| {
        let map = catalog().require_widget("map").expect("the catalog defines map");
        serde_json::from_value(map.defaults["map_style"].clone()).expect("map defaults must name a map style")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_matches_the_manifest() {
        assert_eq!(widget_font_weight(), 700.0);
        assert_eq!(time_icon_asset_file(), "widget-time.svg");
        let limits = chart_limits();
        assert_eq!((limits.max_series, limits.min_window_seconds, limits.max_window_seconds), (5, 5.0, 300.0));
        assert_eq!(default_map_style(), &MapStyleId::new("liberty").unwrap());
    }
}
