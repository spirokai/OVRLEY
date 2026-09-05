# OVRLEY Visual Auto-Sync â€” Research Brief

## Goal

Add a fallback synchronization method for videos that contain **no usable timestamps or telemetry**.

Typical difficult case:

- video clip: ~1 minute
- activity recording: potentially ~6 hours
- activity sampling: often only ~1 Hz GPS/FIT data
- sport/activity type: arbitrary

The system must identify **where the short video belongs within the long activity**, and only accept the match when confidence is high.

IMPORTANT: Activity sampling is variable and anywhere between 1-40 Hz. This must be taken into account when designing the fingerprint matching algorithm.

## Core strategy

Use **visual camera motion as a fingerprint** and match it against motion derived from the activity.

```text
Video
â†’ decode/downscale/grayscale
â†’ detect trackable image points
â†’ optical flow between frames
â†’ estimate global camera motion
â†’ derive visual motion time series
â†’ aggregate to ~1 Hz
                            â†“
Activity â†’ speed / heading change / acceleration / stops
                            â†“
        search entire activity for best temporal match
                            â†“
             offset + confidence / no-match
```

The video side should produce **visual motion proxies**, not pretend to directly calculate GPS heading or physical acceleration.

Useful video-derived signals may include:

- camera rotation / yaw
- overall motion magnitude
- translation / expansion patterns
- changes in those values

The activity side can provide:

- heading change / turn rate
- speed change / acceleration
- stop/start patterns
- other motion channels when available

Because activity data may only be 1 Hz, **do not upsample it for the primary fingerprint search**. Analyze video at a useful internal rate (for example several fps), then aggregate visual features to approximately the activity sampling rate.

Interpolation can be considered later for fine alignment after the correct region has been found.

## Matching and confidence

This is effectively **short-template vs long-signal matching**, not merely offset correction.

For every candidate position in the full activity, calculate similarity between the video fingerprint and activity fingerprint.

Confidence must account for ambiguity because a 60-second clip may have many plausible locations in a 6-hour activity.

Important confidence criteria:

- best candidate significantly better than second-best candidate
- multiple independent features agree on the same offset
- different subsegments of the video (for example 3 Ã— 20 s) independently resolve to the same region
- correlation peak is sufficiently sharp / unique
- reject synchronization entirely when the video contains insufficient distinctive motion

OVRLEY should prefer **â€œno confident visual matchâ€** over returning an unreliable offset.

## Gyroflow as architectural reference

Gyroflow repository:

**https://github.com/gyroflow/gyroflow**

Study Gyroflow's current Rust synchronization implementation and **mimic its architecture**, while implementing OVRLEY's own activity-matching stage.

Main directory:

`src/core/synchronization/`

Repository path:

**https://github.com/gyroflow/gyroflow/tree/master/src/core/synchronization**

Gyroflow already separates synchronization into optical flow, pose estimation, offset finding and autosync orchestration. We want to reuse as much of the code structure and architecture as possible, but replace the gyro-specific offset-finding code with a new activity-matching implementation.

## Highest-priority Gyroflow files

### `src/core/synchronization/optical_flow/opencv_pyrlk.rs`

Primary reference for the initial OVRLEY implementation.

Gyroflow uses the Rust `opencv` crate to:

1. select good trackable points using OpenCV `good_features_to_track`
2. track those points into the next frame using pyramidal Lucasâ€“Kanade (`calc_optical_flow_pyr_lk`)
3. discard failed / out-of-frame correspondences

Direct link:

**https://github.com/gyroflow/gyroflow/blob/master/src/core/synchronization/optical_flow/opencv_pyrlk.rs**

OVRLEY should initially follow this sparse optical-flow approach rather than dense optical flow.

### `src/core/synchronization/optical_flow/mod.rs`

Study the abstraction Gyroflow uses around optical-flow implementations and frame-to-frame point correspondences.

Also inspect:

- `opencv_dis.rs`
- `akaze.rs`

Directory:

**https://github.com/gyroflow/gyroflow/tree/master/src/core/synchronization/optical_flow**

### `src/core/synchronization/estimate_pose/`

Study how Gyroflow converts tracked point pairs into estimated camera rotation / pose.

This is the layer between raw optical flow and the motion signal used for synchronization.

Inspect:

- `mod.rs`
- `find_essential_mat.rs`
- `find_homography.rs`
- `eight_point.rs`
- `almeida.rs`

Directory:

**https://github.com/gyroflow/gyroflow/tree/master/src/core/synchronization/estimate_pose**

The research should determine which pose / motion representation is actually useful for GPS matching rather than blindly reproducing Gyroflow's gyro-specific requirements.

### `src/core/synchronization/autosync.rs`

Study this for overall orchestration:

- selecting frames / windows
- generating optical-flow information
- performing sync searches
- coordinating the synchronization pipeline

Direct link:

**https://github.com/gyroflow/gyroflow/blob/master/src/core/synchronization/autosync.rs**

The architecture is relevant even though OVRLEY's target signal is activity / GPS rather than gyro data.

## Offset-finding code to study, but not directly reproduce

Directory:

`src/core/synchronization/find_offset/`

**https://github.com/gyroflow/gyroflow/tree/master/src/core/synchronization/find_offset**

Especially inspect:

- `visual_features.rs`
- `essential_matrix.rs`

These demonstrate how Gyroflow searches for synchronization using visual motion information, but they are designed around matching video motion against IMU / gyro data.

OVRLEY needs a new matcher designed for:

**visual motion â†” low-rate GPS/activity motion over potentially very large search windows**

## Proposed OVRLEY architecture

Conceptually mirror Gyroflow:

```text
visual_sync/
    optical_flow/
        opencv_pyrlk.rs
    motion_estimation/
        ...
    video_fingerprint.rs
    activity_fingerprint.rs
    matcher.rs
    confidence.rs
    mod.rs
```

Use **opencv-rust (`opencv` crate)** for feature detection / tracking rather than `rust-cv`.

The first implementation should focus on:

**sparse PyrLK optical flow â†’ global camera-motion estimation â†’ ~1 Hz visual fingerprint â†’ whole-activity search against GPS turn-rate / speed-change fingerprints â†’ multi-signal confidence score**

Bitrate / frame-size analysis should not be part of the initial implementation; it is much less directly related to physical motion and is strongly affected by scene complexity and encoder behavior.
