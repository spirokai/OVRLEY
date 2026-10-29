//! Rational frame-rate helpers for video source and output paths.
//!
//! Source probing and composite rendering keep video frame rates as exact
//! rationals so NTSC rates such as `30000/1001` are not rounded during command
//! construction or metadata handoff.

use crate::error::{CoreError, CoreResult};
use std::time::Duration;

const FRAME_BOUNDARY_TOLERANCE_DENOMINATOR: u128 = 10_000;

/// Exact rational frames-per-second value used for FFmpeg arguments and timing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fps {
    num: u32,
    den: u32,
}

impl Fps {
    /// Creates a reduced rational FPS after validating both components.
    ///
    /// The numerator and denominator must be non-zero because FFmpeg and frame
    /// timing code cannot represent zero-rate streams.
    pub fn new(num: u32, den: u32) -> CoreResult<Self> {
        if num == 0 {
            return Err(CoreError::Encode(
                "FPS numerator must be greater than zero".to_string(),
            ));
        }
        if den == 0 {
            return Err(CoreError::Encode(
                "FPS denominator must be greater than zero".to_string(),
            ));
        }
        let gcd = gcd_u32(num, den);
        Ok(Self {
            num: num / gcd,
            den: den / gcd,
        })
    }

    /// Converts this rational FPS to a floating point value for duration math.
    pub fn as_f64(&self) -> f64 {
        self.num as f64 / self.den as f64
    }

    /// Returns the reduced numerator and denominator.
    pub fn components(&self) -> (u32, u32) {
        (self.num, self.den)
    }

    /// Formats this FPS as the rational string expected by FFmpeg.
    pub fn ffmpeg_arg(&self) -> String {
        format!("{}/{}", self.num, self.den)
    }

    /// Counts the canonical half-open frame interval for a duration.
    ///
    /// Duration is converted once to Rust's nanosecond timebase, then division
    /// is performed with integers. Metadata within 1/10,000 of a frame boundary
    /// is snapped to that boundary; larger remainders use ceiling division.
    /// This avoids phantom frames caused by container timestamp jitter.
    pub fn frame_count_for_duration(&self, duration_seconds: f64) -> CoreResult<u64> {
        let duration = Duration::try_from_secs_f64(duration_seconds).map_err(|_| {
            CoreError::Encode(format!(
                "Frame duration must be finite and zero or greater: {duration_seconds}"
            ))
        })?;
        let scaled_numerator = duration
            .as_nanos()
            .checked_mul(u128::from(self.num))
            .ok_or_else(|| CoreError::Encode("Frame count numerator overflow".to_string()))?;
        let scaled_denominator = u128::from(self.den) * 1_000_000_000;
        let whole_frames = scaled_numerator / scaled_denominator;
        let remainder = scaled_numerator % scaled_denominator;
        let frame_count = if remainder == 0
            || (whole_frames > 0
                && remainder * FRAME_BOUNDARY_TOLERANCE_DENOMINATOR <= scaled_denominator)
        {
            whole_frames
        } else {
            whole_frames + 1
        };
        u64::try_from(frame_count)
            .map_err(|_| CoreError::Encode("Frame count exceeds u64 capacity".to_string()))
    }

    /// Returns the timestamp of a frame index in seconds.
    pub fn seconds_at_frame(&self, frame_index: u64) -> f64 {
        frame_index as f64 * self.den as f64 / self.num as f64
    }

    /// Builds the canonical half-open frame timeline for a duration.
    pub fn timeline_for_duration(&self, duration_seconds: f64) -> CoreResult<Vec<f64>> {
        let frame_count = usize::try_from(self.frame_count_for_duration(duration_seconds)?)
            .map_err(|_| CoreError::Encode("Frame timeline exceeds usize capacity".to_string()))?;
        Ok((0..frame_count)
            .map(|frame_index| self.seconds_at_frame(frame_index as u64))
            .collect())
    }

    /// Divides this FPS by a positive integer overlay update factor.
    ///
    /// Composite mode uses this to derive overlay pipe FPS from source video FPS
    /// without rounding fractional NTSC rates.
    pub fn divided_by(&self, factor: std::num::NonZeroU32) -> CoreResult<Fps> {
        let den = self.den.checked_mul(factor.get()).ok_or_else(|| {
            CoreError::Encode("FPS denominator overflow while dividing rate".to_string())
        })?;
        Fps::new(self.num, den)
    }

    /// Converts floating point FPS metadata to rational rates.
    ///
    /// This is the ingress adapter for external metadata sources that expose
    /// only a floating-point rate. Common broadcast/video rates are mapped to
    /// their canonical rationals; other positive finite values use the nearest
    /// integer rate supplied by that external format.
    pub fn from_f64_metadata(value: f64) -> CoreResult<Fps> {
        if !value.is_finite() || value <= 0.0 {
            return Err(CoreError::Encode(format!(
                "FPS value must be finite and positive: {value}"
            )));
        }

        for (num, den) in [
            (24000, 1001),
            (24, 1),
            (25, 1),
            (30000, 1001),
            (30, 1),
            (48, 1),
            (50, 1),
            (60000, 1001),
            (60, 1),
            (120, 1),
        ] {
            let candidate = num as f64 / den as f64;
            if (value - candidate).abs() <= 0.01 {
                return Fps::new(num, den);
            }
        }

        Fps::new(value.round() as u32, 1)
    }
}

/// Computes the greatest common divisor for two unsigned integers.
///
/// The helper uses Euclid's algorithm and returns at least `1` for non-zero FPS
/// inputs so callers can safely divide numerator and denominator.
fn gcd_u32(mut left: u32, mut right: u32) -> u32 {
    while right != 0 {
        let next = left % right;
        left = right;
        right = next;
    }
    left
}
