// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright © 2021-2022 Adrian <adrian.eddy at gmail>
// Adapted from estimate_pose/find_homography.rs; see ../UPSTREAM.md.

use super::{MotionEstimate, MotionOutcome, QualityReason};
use crate::synchronization::{
    optical_flow::{coverage, Point, TrackedPairs},
    AnalysisResult,
};
use opencv::{
    core::{Mat, Point2f},
    prelude::*,
};

pub(crate) fn estimate(tracks: TrackedPairs, seconds: f64) -> AnalysisResult<MotionOutcome> {
    // Fit with both axes measured in image-width units so rotation and uniform
    // scale retain their geometry on non-square frames.
    let aspect = tracks.height_over_width;
    let (pts1, pts2): (Vec<Point2f>, Vec<Point2f>) = tracks
        .pairs
        .iter()
        .map(|pair| {
            (
                Point2f::new(pair.before.x as f32, (pair.before.y * aspect) as f32),
                Point2f::new(pair.after.x as f32, (pair.after.y * aspect) as f32),
            )
        })
        .unzip();
    let a1_pts = Mat::from_slice(&pts1)?;
    let a2_pts = Mat::from_slice(&pts2)?;
    let mut inliers = Mat::default();
    // Normalized image coordinates: 0.01 is ~6.4 horizontal pixels at 640 width.
    // Full affine also permits shear and nonuniform scale.
    let affine = opencv::calib3d::estimate_affine_partial_2d(
        &a1_pts,
        &a2_pts,
        &mut inliers,
        opencv::calib3d::RANSAC,
        0.01,
        2000,
        0.999,
        10,
    )?;
    if affine.empty() {
        return Ok(MotionOutcome::Unavailable(QualityReason::NoGlobalModel));
    }
    let mut transform = [[0.0; 3]; 2];
    for (row, values) in transform.iter_mut().enumerate() {
        for (col, value) in values.iter_mut().enumerate() {
            *value = *affine.at_2d::<f64>(row as i32, col as i32)?;
        }
    }
    // Convert the fitted transform back to the canonical width/height-normalized
    // coordinates used by coverage, residuals and the existing motion outputs.
    transform[0][1] *= aspect;
    transform[1][0] /= aspect;
    transform[1][2] /= aspect;
    if !well_conditioned(&transform) {
        return Ok(MotionOutcome::Unavailable(
            QualityReason::IllConditionedTransform,
        ));
    }
    let mut supported = Vec::new();
    let mut residual = 0.0;
    for (i, pair) in tracks.pairs.iter().enumerate() {
        if *inliers.at::<u8>(i as i32)? == 0 {
            continue;
        }
        let predicted = project(&transform, pair.before);
        residual += (predicted.x - pair.after.x).powi(2) + (predicted.y - pair.after.y).powi(2);
        supported.push(pair.before);
    }
    let count = supported.len();
    let inlier_fraction = count as f64 / tracks.pairs.len() as f64;
    let inlier_coverage = coverage(supported.into_iter());
    // Gently relaxed PoorFit thresholds to retain more motion observations.
    if count < 12 || inlier_fraction < 0.55 || inlier_coverage < 0.375 {
        return Ok(MotionOutcome::Unavailable(QualityReason::PoorFit));
    }
    let rms_residual = (residual / count as f64).sqrt();
    let mut horizontal = 0.0;
    let mut vertical = 0.0;
    let mut roll = 0.0;
    let mut expansion = 0.0;
    let mut radius = 0.0;
    for y in [-0.4, -0.2, 0.0, 0.2, 0.4] {
        for x in [-0.4, -0.2, 0.0, 0.2, 0.4] {
            let predicted = project(&transform, Point { x, y });
            let dx = predicted.x - x;
            let dy = predicted.y - y;
            horizontal += dx;
            vertical += dy;
            roll += x * dy - y * dx;
            expansion += x * dx + y * dy;
            radius += x * x + y * y;
        }
    }
    Ok(MotionOutcome::Estimated(MotionEstimate {
        horizontal_per_second: horizontal / (25.0 * seconds),
        vertical_per_second: vertical / (25.0 * seconds),
        roll_per_second: roll / (radius * seconds),
        expansion_per_second: expansion / (radius * seconds),
        residual_per_second: rms_residual / seconds,
        inliers: count,
        inlier_fraction,
        inlier_coverage,
        rms_residual,
        tracking: tracks.quality,
    }))
}

fn project(transform: &[[f64; 3]; 2], p: Point) -> Point {
    Point {
        x: transform[0][0] * p.x + transform[0][1] * p.y + transform[0][2],
        y: transform[1][0] * p.x + transform[1][1] * p.y + transform[1][2],
    }
}

fn well_conditioned(transform: &[[f64; 3]; 2]) -> bool {
    if transform.iter().flatten().any(|v| !v.is_finite()) {
        return false;
    }
    let determinant = transform[0][0] * transform[1][1] - transform[0][1] * transform[1][0];
    // Include the fixed homogeneous row's unit entry in the norm, preserving
    // the previous conditioning and area-scale limits without projective poles.
    let norm = (1.0 + transform.iter().flatten().map(|v| v * v).sum::<f64>()).sqrt();
    determinant.abs() / norm.powi(3) >= 1e-5 && determinant > 0.05 && determinant < 20.0
}
