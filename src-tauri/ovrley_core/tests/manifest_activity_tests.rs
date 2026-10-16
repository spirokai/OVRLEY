//! Every manifest-declared numeric column passes through native activity preparation.

use ovrley_core::activity::{
    interpolate::densify_activity, schema::ParsedActivity, trim::trim_activity,
};
use ovrley_core::normalize::RenderDataRequirements;
use ovrley_widgets::model::telemetry::{ActivitySeries, InterpolationKind, NumericActivity};
use serde_json::json;

macro_rules! numeric_pipeline_test {
    (numeric { $($field:ident: $variant:ident),* } other { $($other:ident: $unused:ident),* }) => {
        #[test]
        fn manifest_columns_keep_their_identity_gaps_and_sampling_policy() {
            let mut input = json!({
                "sample_elapsed_seconds": [0.0, 1.0, 2.0],
                "trim_end_seconds": 2.0,
            });
            let columns = [$(ActivitySeries::$variant,)*];
            for (index, column) in columns.iter().enumerate() {
                let start = index as f64 * 3.0;
                input[column.as_str()] = json!([start, null, start + 8.0]);
            }
            let activity: ParsedActivity = serde_json::from_value(input).unwrap();
            let requirements = RenderDataRequirements { $($field: true,)* ..Default::default() };
            let trimmed = trim_activity(&activity, 0.0, 2.0, &requirements).unwrap();
            let dense = densify_activity(&trimmed, vec![0.0, 1.0, 2.0], &requirements);
            for (index, column) in columns.iter().enumerate() {
                let start = index as f64 * 3.0;
                let middle = match column.interpolation() {
                    InterpolationKind::Linear | InterpolationKind::Angular => Some(start + 4.0),
                    InterpolationKind::Hold => Some(start),
                    InterpolationKind::Preserve => None,
                };
                assert_eq!(
                    dense.series.numeric_series(*column),
                    [Some(start), middle, Some(start + 8.0)],
                    "{} must keep its own values and manifest policy", column.as_str(),
                );
            }
        }
    };
}

ovrley_widgets::activity_series!(numeric_pipeline_test);
