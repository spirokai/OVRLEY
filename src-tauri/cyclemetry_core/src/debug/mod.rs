use serde::Serialize;
use std::collections::BTreeMap;
use std::time::Instant;

#[derive(Clone, Debug, Serialize)]
pub struct RenderProgress {
    pub current: u32,
    pub total: u32,
    pub encoded: u32,
    pub status: String,
    pub message: String,
    pub estimated_seconds_remaining: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
}

impl Default for RenderProgress {
    fn default() -> Self {
        Self {
            current: 0,
            total: 0,
            encoded: 0,
            status: "idle".to_string(),
            message: String::new(),
            estimated_seconds_remaining: None,
            filename: None,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct TimingBucket {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alt_name: Option<String>,
    pub count: u32,
    pub total_ms: f64,
    pub avg_ms: f64,
    pub max_ms: f64,
}

impl TimingBucket {
    pub fn add_sample(&mut self, duration_ms: f64) {
        self.count += 1;
        self.total_ms += duration_ms;
        self.avg_ms = if self.count == 0 {
            0.0
        } else {
            self.total_ms / f64::from(self.count)
        };
        if duration_ms > self.max_ms {
            self.max_ms = duration_ms;
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct RenderProfiler {
    buckets: BTreeMap<String, TimingBucket>,
}

impl RenderProfiler {
    pub fn record_ms(&mut self, name: impl Into<String>, duration_ms: f64) {
        self.buckets
            .entry(name.into())
            .or_default()
            .add_sample(duration_ms);
    }

    pub fn measure<T>(&mut self, name: impl Into<String>, callback: impl FnOnce() -> T) -> T {
        let name = name.into();
        let started = Instant::now();
        let result = callback();
        self.record_ms(name, started.elapsed().as_secs_f64() * 1000.0);
        result
    }

    pub fn summary(&self) -> BTreeMap<String, TimingBucket> {
        self.buckets.clone()
    }
}
