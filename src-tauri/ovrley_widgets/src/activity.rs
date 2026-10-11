//! Original-density preview samples. Preparation is once per activity, never per frame.
use crate::error::{WidgetError, WidgetResult};
use serde::Deserialize;

/// Subset of the canonical finalized activity required by this migration slice.
/// Serde skips other telemetry columns without constructing them in WASM.
#[derive(Deserialize)]
pub struct PreviewActivity {
    sample_elapsed_seconds: Vec<f64>,
    #[serde(default)]
    speed: Vec<Option<f64>>,
    trim_end_seconds: f64,
}

pub struct ActivitySampler {
    activity: PreviewActivity,
    valid_speed_indices: Vec<usize>,
    right: usize,
}

impl ActivitySampler {
    pub fn from_json(bytes: &[u8]) -> WidgetResult<Self> {
        let activity: PreviewActivity = serde_json::from_slice(bytes)
            .map_err(|e| WidgetError::Config(format!("preview activity: {e}")))?;
        if !activity.trim_end_seconds.is_finite()
            || activity.trim_end_seconds < 0.0
            || activity
                .sample_elapsed_seconds
                .iter()
                .any(|t| !t.is_finite())
            || activity
                .sample_elapsed_seconds
                .windows(2)
                .any(|p| p[0] >= p[1])
            || (!activity.speed.is_empty()
                && activity.speed.len() != activity.sample_elapsed_seconds.len())
        {
            return Err(WidgetError::Config(
                "finalized activity timestamps and speed samples must align".into(),
            ));
        }
        let valid_speed_indices = activity
            .speed
            .iter()
            .enumerate()
            .filter_map(|(i, value)| value.filter(|v| v.is_finite()).map(|_| i))
            .collect();
        Ok(Self {
            activity,
            valid_speed_indices,
            right: 0,
        })
    }

    /// Bridge missing speed samples, matching export's linear interpolation policy.
    pub fn speed_at(&mut self, second: f64) -> Option<f64> {
        if second < 0.0
            || second > self.activity.trim_end_seconds
            || self.valid_speed_indices.is_empty()
        {
            return None;
        }
        let indices = &self.valid_speed_indices;
        let times = &self.activity.sample_elapsed_seconds;
        let values = &self.activity.speed;
        let last = indices.len() - 1;
        if second <= times[indices[0]] {
            return values[indices[0]];
        }
        if second >= times[indices[last]] {
            return values[indices[last]];
        }
        let inside = self.right > 0
            && second > times[indices[self.right - 1]]
            && second <= times[indices[self.right]];
        if !inside {
            if self.right < last
                && second > times[indices[self.right]]
                && second <= times[indices[self.right + 1]]
            {
                self.right += 1;
            } else {
                self.right = indices.partition_point(|&i| times[i] < second);
            }
        }
        let left = indices[self.right - 1];
        let right = indices[self.right];
        let a = values[left].expect("prepared speed index is present");
        let b = values[right].expect("prepared speed index is present");
        crate::interpolation::interpolate_points(&[(times[left], a), (times[right], b)], second)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_samples_support_playback_gaps_and_backward_seeks() {
        let mut sampler = ActivitySampler::from_json(
            br#"{"sample_elapsed_seconds":[0,1,2,3],"speed":[0,null,20,30],"trim_end_seconds":3}"#,
        )
        .unwrap();
        for (t, expected) in [(0.5, 5.0), (0.6, 6.0), (2.5, 25.0), (0.2, 2.0), (3.0, 30.0)] {
            assert!((sampler.speed_at(t).unwrap() - expected).abs() < 1e-9);
        }
        assert_eq!(sampler.speed_at(-1.0), None);
        assert_eq!(sampler.speed_at(4.0), None);
        assert_eq!(sampler.activity.speed.len(), 4);
        assert_eq!(sampler.valid_speed_indices, [0, 2, 3]);
    }
}
