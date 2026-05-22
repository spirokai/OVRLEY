//! Rational frame-rate helpers for video encoding paths.
//!
//! Composite rendering keeps source-video frame rates as exact rationals so
//! NTSC rates such as `30000/1001` are not rounded during command construction.

use crate::error::{CoreError, CoreResult};

/// Exact rational frames-per-second value used for FFmpeg arguments and timing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fps {
    pub num: u32,
    pub den: u32,
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
        Ok(Self { num, den }.reduced())
    }

    /// Converts this rational FPS to a floating point value for duration math.
    pub fn as_f64(&self) -> f64 {
        self.num as f64 / self.den as f64
    }

    /// Formats this FPS as the rational string expected by FFmpeg.
    pub fn ffmpeg_arg(&self) -> String {
        format!("{}/{}", self.num, self.den)
    }

    /// Divides this FPS by a positive integer overlay update factor.
    ///
    /// Composite mode uses this to derive overlay pipe FPS from source video FPS
    /// without rounding fractional NTSC rates.
    pub fn divided_by(&self, factor: u32) -> CoreResult<Fps> {
        if factor == 0 {
            return Err(CoreError::Encode(
                "FPS division factor must be greater than zero".to_string(),
            ));
        }
        Ok(Fps {
            num: self.num,
            den: self.den.saturating_mul(factor),
        }
        .reduced())
    }

    /// Returns the mathematically reduced form of this rational FPS.
    pub fn reduced(&self) -> Fps {
        let gcd = gcd_u32(self.num, self.den);
        Fps {
            num: self.num / gcd,
            den: self.den / gcd,
        }
    }

    /// Converts common floating point FPS metadata to exact rational rates.
    ///
    /// This is a fallback for callers that do not yet have numerator and
    /// denominator metadata; exact rational fields should be preferred.
    pub fn from_f64_fallback(value: f64) -> CoreResult<Fps> {
        if !value.is_finite() || value <= 0.0 {
            return Err(CoreError::Encode(format!(
                "FPS value must be finite and positive: {value}"
            )));
        }

        for (approx, num, den) in [
            (23.976, 24000, 1001),
            (29.97, 30000, 1001),
            (59.94, 60000, 1001),
            (25.0, 25, 1),
            (30.0, 30, 1),
            (60.0, 60, 1),
        ] {
            if (value - approx).abs() <= 0.001 {
                return Fps::new(num, den);
            }
        }

        Err(CoreError::Encode(format!(
            "Unsupported non-rational FPS fallback value: {value}"
        )))
    }
}

/// Computes the greatest common divisor for two unsigned integers.
///
/// The helper uses Euclid's algorithm and returns at least `1` for non-zero FPS
/// inputs so callers can safely divide numerator and denominator.
pub fn gcd_u32(mut left: u32, mut right: u32) -> u32 {
    while right != 0 {
        let next = left % right;
        left = right;
        right = next;
    }
    left.max(1)
}
