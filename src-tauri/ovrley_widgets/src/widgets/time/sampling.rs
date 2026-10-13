//! Prepared absolute timestamps shared by native densification and live preview.
use chrono::{DateTime, SecondsFormat, Utc};

pub enum TimeSampler {
    Recorded(Vec<(f64, f64)>),
    Sync(Option<DateTime<Utc>>),
}

#[cfg(test)]
mod tests {
    use super::TimeSampler;

    #[test]
    fn recorded_times_bridge_gaps_and_seek_but_only_absent_samples_use_sync_time() {
        let sync = Some("2000-01-01T00:00:00Z");
        let samples = vec![
            Some("2025-04-21T13:05:00Z".into()),
            None,
            Some("2025-04-21T13:05:02Z".into()),
        ];
        let sampler = TimeSampler::new(sync, &[0.0, 1.0, 2.0], &samples);
        assert_eq!(sampler.at(1.5).as_deref(), Some("2025-04-21T13:05:01.500Z"));
        assert_eq!(
            sampler.at(0.25).as_deref(),
            Some("2025-04-21T13:05:00.250Z")
        );
        assert_eq!(sampler.at(-1.0), None);
        assert_eq!(sampler.at(3.0), None);
        assert_eq!(
            TimeSampler::new(sync, &[0.0], &[Some("invalid".into())]).at(0.0),
            None
        );
        assert_eq!(
            TimeSampler::new(sync, &[], &[]).at(-1.25).as_deref(),
            Some("1999-12-31T23:59:58.750Z")
        );
        assert_eq!(TimeSampler::new(None, &[], &[]).at(0.0), None);
    }
}

impl TimeSampler {
    pub fn new(sync_time: Option<&str>, seconds: &[f64], values: &[Option<String>]) -> Self {
        // Recorded timestamps are authoritative, even when malformed external samples
        // leave no usable points. Only an absent series permits the sync-time fallback.
        if values.iter().any(Option::is_some) {
            Self::Recorded(
                seconds
                    .iter()
                    .copied()
                    .zip(values)
                    .filter_map(|(second, raw)| {
                        raw.as_deref()
                            .and_then(|raw| DateTime::parse_from_rfc3339(raw).ok())
                            .map(|time| (second, time.timestamp_millis() as f64))
                    })
                    .collect(),
            )
        } else {
            Self::Sync(
                sync_time
                    .and_then(|raw| DateTime::parse_from_rfc3339(raw).ok())
                    .map(|time| time.to_utc()),
            )
        }
    }

    pub fn at(&self, second: f64) -> Option<String> {
        let time = match self {
            Self::Recorded(points) => {
                if points.first().is_some_and(|p| second < p.0)
                    || points.last().is_some_and(|p| second > p.0)
                {
                    return None;
                }
                crate::interpolation::interpolate_points(points, second).map(|millis| {
                    DateTime::<Utc>::from_timestamp_millis(millis.round() as i64)
                        .unwrap_or(DateTime::<Utc>::UNIX_EPOCH)
                })
            }
            Self::Sync(start) => start.map(|start| {
                start + chrono::TimeDelta::milliseconds((second * 1000.0).round() as i64)
            }),
        };
        time.map(|time| time.to_rfc3339_opts(SecondsFormat::Millis, true))
    }
}
