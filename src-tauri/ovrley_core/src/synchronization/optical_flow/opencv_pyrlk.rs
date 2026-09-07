// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright © 2021-2022 Adrian <adrian.eddy at gmail>
// Adapted for OVRLEY; see ../UPSTREAM.md.

use super::{coverage, Point, PointPair, TrackedPairs, TrackingOutcome, TrackingQuality};
use crate::synchronization::{decode::GrayFrame, AnalysisResult};
use opencv::{
    core::{Mat, Point2f, Size, TermCriteria, Vector},
    prelude::*,
};

pub(crate) fn track(before: &GrayFrame, after: &GrayFrame) -> AnalysisResult<TrackingOutcome> {
    let a1_data = Mat::from_slice(&before.pixels)?;
    let a2_data = Mat::from_slice(&after.pixels)?;
    let a1_img = a1_data.reshape(1, before.height as i32)?;
    let a2_img = a2_data.reshape(1, after.height as i32)?;
    let mut a1_pts = Vector::<Point2f>::new();
    opencv::imgproc::good_features_to_track(
        &a1_img,
        &mut a1_pts,
        200,
        0.01,
        10.0,
        &Mat::default(),
        3,
        false,
        0.04,
    )?;
    let detected = a1_pts.len();
    if detected == 0 {
        return Ok(TrackingOutcome::Insufficient(TrackingQuality {
            detected,
            retained: 0,
            coverage: 0.0,
            mean_forward_backward_error_pixels: 0.0,
        }));
    }
    let mut a2_pts = Vector::<Point2f>::new();
    let mut status = Vector::<u8>::new();
    let mut err = Vector::<f32>::new();
    opencv::video::calc_optical_flow_pyr_lk(
        &a1_img,
        &a2_img,
        &a1_pts,
        &mut a2_pts,
        &mut status,
        &mut err,
        Size::new(21, 21),
        3,
        TermCriteria::new(3, 30, 0.01)?,
        0,
        1e-4,
    )?;
    let mut returned = Vector::<Point2f>::new();
    let mut back_status = Vector::<u8>::new();
    let mut back_err = Vector::<f32>::new();
    opencv::video::calc_optical_flow_pyr_lk(
        &a2_img,
        &a1_img,
        &a2_pts,
        &mut returned,
        &mut back_status,
        &mut back_err,
        Size::new(21, 21),
        3,
        TermCriteria::new(3, 30, 0.01)?,
        0,
        1e-4,
    )?;
    let in_bounds = |p: Point2f| {
        p.x >= 0.0 && p.y >= 0.0 && p.x < before.width as f32 && p.y < before.height as f32
    };
    let normalize = |p: Point2f| Point {
        x: p.x as f64 / before.width as f64 - 0.5,
        y: p.y as f64 / before.height as f64 - 0.5,
    };
    let mut pairs = Vec::with_capacity(detected);
    let mut total_error = 0.0;
    for i in 0..detected {
        if status.get(i)? != 1 || back_status.get(i)? != 1 {
            continue;
        }
        let pt1 = a1_pts.get(i)?;
        let pt2 = a2_pts.get(i)?;
        let back = returned.get(i)?;
        let error = ((pt1.x - back.x).powi(2) + (pt1.y - back.y).powi(2)).sqrt();
        if in_bounds(pt1) && in_bounds(pt2) && error.is_finite() && error <= 1.5 {
            pairs.push(PointPair {
                before: normalize(pt1),
                after: normalize(pt2),
            });
            total_error += error as f64;
        }
    }
    let retained = pairs.len();
    let quality = TrackingQuality {
        detected,
        retained,
        coverage: coverage(pairs.iter().map(|pair| pair.before)),
        mean_forward_backward_error_pixels: if retained == 0 {
            0.0
        } else {
            total_error / retained as f64
        },
    };
    // Starting quality gates; low texture/cuts are unavailable motion, never zero motion.
    if retained < 10 || retained * 4 < detected || quality.coverage < 0.375 {
        return Ok(TrackingOutcome::Insufficient(quality));
    }
    Ok(TrackingOutcome::Tracked(TrackedPairs {
        pairs,
        quality,
        height_over_width: before.height as f64 / before.width as f64,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translated_texture_flows_through_tracking_and_homography() {
        let width = 256;
        let height = 192;
        let mut pixels = vec![0; width * height];
        for y in 0..height {
            for x in 0..width {
                // Deterministic texture with distinctive local patches, no external fixture.
                let v = (x as u32).wrapping_mul(73856093) ^ (y as u32).wrapping_mul(19349663);
                pixels[y * width + x] = ((v ^ (v >> 13)) & 255) as u8;
            }
        }
        let mut shifted = vec![0; pixels.len()];
        for y in 0..height - 2 {
            for x in 0..width - 3 {
                shifted[(y + 2) * width + x + 3] = pixels[y * width + x];
            }
        }
        let before = GrayFrame {
            seconds: 0.0,
            width,
            height,
            pixels,
        };
        let after = GrayFrame {
            seconds: 0.1,
            width,
            height,
            pixels: shifted,
        };
        let TrackingOutcome::Tracked(tracks) = track(&before, &after).unwrap() else {
            panic!("texture should track");
        };
        let crate::synchronization::MotionOutcome::Estimated(motion) =
            crate::synchronization::motion_estimation::homography::estimate(tracks, 0.1).unwrap()
        else {
            panic!("translation should fit");
        };
        assert!((motion.horizontal_per_second - 3.0 / 256.0 / 0.1).abs() < 0.01);
        assert!((motion.vertical_per_second - 2.0 / 192.0 / 0.1).abs() < 0.01);
        assert!(motion.roll_per_second.abs() < 0.01);
    }

    #[test]
    fn blank_frames_are_quality_outcome() {
        let frame = GrayFrame {
            seconds: 0.0,
            width: 64,
            height: 64,
            pixels: vec![0; 4096],
        };
        assert!(matches!(
            track(&frame, &frame).unwrap(),
            TrackingOutcome::Insufficient(_)
        ));
    }
}
