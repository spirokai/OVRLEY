# Visual activity/video alignment implementation plan

Status: proposed implementation; no feature code implemented.

Based on [motion-analysis.md](motion-analysis.md), the current OVRLEY source, and Gyroflow revision [`d918ab3594e539f25a67a9f1d2b8e042798f61f7`](https://github.com/gyroflow/gyroflow/commit/d918ab3594e539f25a67a9f1d2b8e042798f61f7), inspected on 2026-09-05. Numerical tuning values below are starting hypotheses unless explicitly identified as release criteria.

## 1. Outcome and scope

Implement a local Rust pipeline that locates a video within an activity by comparing image-derived motion with activity-derived motion. The reference case is a 60-second video somewhere in a six-hour activity; the implementation must also handle shorter and longer clips, activity rates from 1–40 Hz, irregular timestamps, and channels with different effective update rates.

The output is a ranked set of candidate offsets with evidence and uncertainty, or an explicit no-match result. A toolbar Sync Doctor drawer runs analysis, shows cancellable progress, and lets the user apply a candidate by clicking it. Running analysis alone never changes alignment.

This is a fallback when timestamp/telemetry alignment is unavailable or unhelpful. Do not require absolute video time, camera IMU, lens calibration, or a particular sport. Lack of those inputs must be represented explicitly. Accepting arbitrary sports does not mean every recording is visually identifiable: head movement, stabilization, scenery, or a straight steady journey can make alignment impossible.

Initial alignment model: `activity_time = video_time + offset_seconds`, with both times measured from their source starts. Sampling-rate differences do not imply playback-rate differences. Edited montages, speed ramps, and drift that prevents one offset fitting the selected video are no-match cases in this version; fitting clock scale or piecewise alignment is a separate feature requiring changes to preview and rendering.

Exclude bitrate/frame-size fingerprints, semantic scene recognition, map services, audio matching, dense optical flow, and arbitrary dynamic time warping.

The first implementation consumes the existing `ParsedActivity`. Do not implement `sync_observations`, change format parsers/finalization to capture extra observations, or require activity re-import for this version. Chapter 12 describes an optional follow-up only if testing the first implementation demonstrates that its existing activity inputs are insufficient.

## 2. Gyroflow reuse decisions

Extract useful source into OVRLEY rather than depending on the complete Gyroflow core. Keep the numerical tracking code as close to the pinned source as practical, with a small documented patch surface. OVRLEY owns the extracted interfaces directly; do not construct fake `StabilizationManager`, `ComputeParams`, or `TimeIMU` objects to satisfy upstream APIs.

Paths in this table are relative to Gyroflow's [`src/core/synchronization/`](https://github.com/gyroflow/gyroflow/tree/d918ab3594e539f25a67a9f1d2b8e042798f61f7/src/core/synchronization).

| Upstream file | Inspected behavior | Implementation decision |
| --- | --- | --- |
| [`optical_flow/opencv_pyrlk.rs`](https://github.com/gyroflow/gyroflow/blob/d918ab3594e539f25a67a9f1d2b8e042798f61f7/src/core/synchronization/optical_flow/opencv_pyrlk.rs) | Good-features detection, pyramidal LK, status/bounds filtering, timestamp-keyed pair cache. | Directly extract detection/tracking and filtering. Start with upstream numerical settings. Replace whole-run cache ownership with bounded frame-pair ownership; return typed errors and quality outcomes. Add forward/backward consistency and spatial coverage checks. |
| `optical_flow/mod.rs` | Separates feature detection, correspondences, and frame cleanup; dispatches three implementations. | Preserve separation and correspondence model. Ship one concrete PyrLK implementation; do not copy numeric method fallback or unused algorithm dispatch/dependencies. |
| [`estimate_pose/find_homography.rs`](https://github.com/gyroflow/gyroflow/blob/d918ab3594e539f25a67a9f1d2b8e042798f61f7/src/core/synchronization/estimate_pose/find_homography.rs) | Undistorts points, robustly fits a homography, decomposes it to rotation. | Extract robust homography estimation for the initial image-space motion model. Replace calibration-dependent preparation with normalized image coordinates and retuned residual thresholds. Use the transform's displacement field; do not call its decomposition physical yaw without calibration. |
| [`estimate_pose/find_essential_mat.rs`](https://github.com/gyroflow/gyroflow/blob/d918ab3594e539f25a67a9f1d2b8e042798f61f7/src/core/synchronization/estimate_pose/find_essential_mat.rs) | Undistorts to calibrated coordinates, estimates essential matrix and recovers rotation with inlier checks. | Retain as the first optional calibrated extension if fixtures demonstrate benefit. Extract the estimator and matrix conversion, with an explicit calibration input; never invent focal length to enable it. |
| `estimate_pose/mod.rs` | Pose interface takes stabilization parameters and returns rotation. | Adapt the separation, but define an OVRLEY motion estimate with proxies and fit quality. No stabilization context in its signature. |
| `estimate_pose/eight_point.rs` | Uses rust-cv, ARR-SAC and calibrated bearings. | Inspected; exclude from initial implementation because OpenCV already covers the required fitting and this adds a second CV dependency stack. |
| `estimate_pose/almeida.rs` | Rotation-only optimizer with a camera model and stabilization transforms; itself credits ofps. | Inspected; defer. Its camera-model dependency and rotation-only output do not solve uncalibrated activity matching. Preserve original provenance if later extracted. |
| `optical_flow/opencv_dis.rs`, `optical_flow/akaze.rs` | Dense DIS and descriptor matching alternatives. | Inspected; exclude from production initially. Revisit only for demonstrated PyrLK failure cases. |
| [`autosync.rs`](https://github.com/gyroflow/gyroflow/blob/d918ab3594e539f25a67a9f1d2b8e042798f61f7/src/core/synchronization/autosync.rs) | Selects timestamp ranges, feeds frames, coordinates estimation, progress, cancellation and cleanup; depends on gyro/stabilization state. | Adapt range selection and staged orchestration. Implement OVRLEY job ownership and joined shutdown rather than transplanting its manager, global estimator caches, or completion polling. |
| [`find_offset/visual_features.rs`](https://github.com/gyroflow/gyroflow/blob/d918ab3594e539f25a67a9f1d2b8e042798f61f7/src/core/synchronization/find_offset/visual_features.rs) | Scores point displacement after gyro-informed undistortion; searches around an initial offset with millisecond refinement. | Reference only. The cost function requires gyro and cannot locate a clip in GPS data. |
| [`find_offset/essential_matrix.rs`](https://github.com/gyroflow/gyroflow/blob/d918ab3594e539f25a67a9f1d2b8e042798f61f7/src/core/synchronization/find_offset/essential_matrix.rs) | Filters estimated and recorded gyro, then minimizes axis-weighted differences around an initial offset. | Reuse the coarse/refine concept, not its cost, fixed filtering frequencies, gyro nearest-sample lookup, or precision assumptions. Implement a new whole-activity matcher. |

The inspected files carry GPL-3.0-or-later headers; OVRLEY's root package already declares that license. Retain copyright/SPDX headers and record upstream revision, copied symbols, modifications, and transitive notices in `synchronization/UPSTREAM.md`. Verify distribution notices as part of dependency integration. The aim is maximum useful source reuse without importing unrelated stabilization machinery.

## 3. Existing OVRLEY seams and required changes

| Current owner | Integration |
| --- | --- |
| `src-tauri/ovrley_core/src/activity/schema.rs` | Consume the existing `ParsedActivity` contract without adding fields. Canonical times are `sample_elapsed_seconds`; use `course`, `speed`, and `heading` in their existing units. |
| `activity/finalize.rs` and `activity/finalize/gap.rs` | Reuse their existing output. No new import pass, observation capture, or finalization changes in the first implementation. Test the effect of smoothing and synthetic idle insertion on matching. |
| `media/video_probe.rs`, `media/source_video_metadata.rs` | Reuse video probing, orientation and stream information. Analysis additionally needs actual presentation timestamps. Container creation time is not needed. |
| `encode/ffmpeg/binary.rs` | Reuse binary resolution and Windows subprocess configuration for the analysis decoder. |
| `commands/mod.rs`, `src-tauri/src/tauri_commands.rs`, `src-tauri/src/lib.rs` | Add core job commands, thin Tauri handlers, registration and managed job state. Existing command bodies live in `tauri_commands.rs`, not solely `lib.rs`. |
| `app/src/api/backend.js` | Own IPC calls and result contract validation. |
| `app/src/store/slices/createVideoImportSlice.js` | Apply accepted offsets through existing `setVideoSyncOffset(seconds)`. |
| `app/src/features/toolbar/`, `createLayoutSlice`, selector hooks | Add one canonical drawer tool and presentational Sync Doctor content; hook owns job lifecycle and candidate application. |

Offset sign is already established by preview: `video_time = selected_activity_second - videoSyncOffsetSeconds`. A clip starting 7,200 seconds into an activity must set `videoSyncOffsetSeconds = 7200`. A match at video second 20 and activity second 7,220 yields the same value, not 7,220. Rendering already passes this through `scene.composite_sync_offset`.

### Fingerprint the existing parsed activity

`activity_fingerprint.rs` consumes the full existing parsed activity and produces temporary matching features. It does not reopen the activity file, rerun a format parser, repeat activity finalization, or mutate the parsed activity. Speed trends, circular heading changes, event detection and matching-rate aggregation are fingerprint computations on this shared input, independent of file format.

The shared finalizer's `build_course_series` preserves missing coordinates; it does not interpolate GPS into every activity row. Frame-aligned GPS interpolation happens later for rendering, and those dense render arrays are not the fingerprint input. For 40 Hz rows with GPS present once per second, select valid `course` pairs with their corresponding `sample_elapsed_seconds` to recover the available GPS cadence. The MP4 path anchors its activity timeline to GPS timestamps when GPS exists, selecting IMU/camera values onto that timeline.

Use existing speed and heading as the initial metric inputs. Their values can be derived, carried forward or smoothed, and stationary gaps can contain synthetic samples. Treat these as processed signals, not independent raw measurements at every row. Existing metadata such as `inserted_idle_sample_count` identifies that insertion occurred but does not locate every synthetic interval. Do not pretend unavailable per-sample provenance exists. Calibrate stop evidence and confidence against these limitations, using other feature families and disjoint sections to corroborate a match.

If a source already repeats GPS coordinates on faster rows, identical values alone cannot reveal the actual receiver update rate. Estimate effective support conservatively from available timing, valid GPS and signal bandwidth, and evaluate that case explicitly. Occasional nulls or smoothing are not assumed to make matching unusable; establish their effect through manual testing of real recordings before expanding the activity contract.

Existing saved activities remain eligible based on the data they already contain; no new field or re-import is required. Insufficient usable motion may produce no-match, just as for a newly imported activity.

Malformed user analysis settings fail at ingress. Fingerprint preparation handles visible external activity anomalies once with explicit masks/reasons: unavailable channels, duplicate/non-monotonic times, GPS jumps and gaps are not fabricated into usable motion. Split on observable unsupported spans or clock discontinuities, without claiming to recover discontinuities already removed upstream. Consumers receive prepared features and do not repeat coercion or normalization.

## 4. Backend structure and ownership

```text
ovrley_core/src/synchronization/
    mod.rs                    public analysis entry point
    types.rs                  validated requests, fingerprints, outcomes
    config.rs                 versioned algorithm settings and validation
    decode.rs                 ffmpeg frames + presentation timestamps
    optical_flow/
        mod.rs                frame-pair types and quality
        opencv_pyrlk.rs        extracted tracking implementation
    motion_estimation/
        mod.rs                motion/quality contract
        homography.rs         extracted fit + image-space proxies
    video_fingerprint.rs      intervals -> visual channels
    activity_fingerprint.rs   existing ParsedActivity -> activity channels
    resample.rs               time-weighted filtering/binning and masks
    matcher.rs                whole-activity search + candidate refinement
    confidence.rs             uniqueness, validation and acceptance
    autosync.rs               one job owns pipeline and cancellation
    UPSTREAM.md               extraction provenance
```

Keep pure matching and confidence independent of OpenCV, Tauri, rendering, and decoding. One job owns the decoder child, bounded frame queue, workers and compact results. Keep only frames needed for current pairs and boundary overlap between chunks. Join workers and kill/reap the child on cancellation, errors and shutdown; a cancellation flag alone cannot unblock a pipe read. Drain stdout/stderr concurrently so diagnostic output cannot stall decoding. OpenCV errors are errors; insufficient trackable scene content is a quality outcome.

Use `opencv` with default features disabled and only the required modules, initially `imgproc`, `video`, and `calib3d` plus their required dependencies. Do not bring in rust-cv, Qt, or Gyroflow's decoder. Rust OpenCV bindings require native OpenCV and binding-generation tooling; pin an OpenCV/crate/toolchain combination proven on the actual repository toolchain, rather than assuming the newest crate works. See the [opencv-rust installation guidance](https://github.com/twistedfall/opencv-rust#quickstart).

Dependency integration includes Windows native DLL discovery/packaging, macOS library relocation/signing where applicable, CI installation/cache, and portable archive inclusion in `scripts/package-portable.mjs`. A development-machine-only installation is not completion. Builds remain subject to the repository's explicit user-permission requirement; this planning task does not authorize one.

## 5. Image analysis

### Decode and track

Use the bundled FFmpeg to decode the original source stream, downscale to roughly 640 pixels on the long edge, correct display rotation, and output grayscale. Start at a target analysis rate of 10 fps, bounded by actual source frame availability. Evaluate 20–40 fps or native rate in candidate refinement when both activity evidence and video support it. Fast motion may require closer frame pairs even when the resulting fingerprint is only 1 Hz.

Preserve actual frame presentation time, not `frame_index / nominal_fps`. An initial subprocess implementation can pair selected raw frames with post-filter `showinfo` frame ordinals/PTS from the same FFmpeg invocation, parsed at the decoder boundary. Use frame selection without synthesized duplicate frames. Explicitly reconcile stream start/edit-list time with browser media time; validate non-zero starts and VFR against preview. Rawvideo alone has no timestamps. Missing/mismatched metadata or short frame reads are decoder errors, not guessed timestamps. Pin and test the selected FFmpeg command/parser combination.

For window seeks, preserve original video-relative PTS and decode/discard preroll; never reset each window to zero. Scene cuts, duplicate frames, and long time gaps break tracks. A missed frame does not become a stationary observation.

Extract PyrLK detection/tracking, then require adequate inlier count, spatial distribution, and forward/backward consistency. Retain quality metrics and invalidate intervals dominated by blur, occlusion, moving foreground, or unstable fitting. Start with upstream settings and tune from manual runs on real footage in the integrated app. Release frames immediately after their pair estimates are reduced.

### Motion representation

Fit a robust global homography to normalized image coordinates using the extracted OpenCV fitting code. Assess residuals, inlier coverage and transform conditioning. Sample its predicted displacement on a fixed normalized grid; derive horizontal flow, vertical flow, image roll, expansion/contraction and residual motion magnitude. Divide displacement/increments by the actual pair duration and timestamp them at the interval midpoint. Model support remains the full pair interval.

Horizontal flow is a turning proxy, not geographic yaw; image roll is not heading change. Expansion and residual flow are relative motion proxies, not metric speed or acceleration. Different depth, side-looking mounts, digital zoom and camera panning confound them. A homography is a global approximation and must be rejected when background support is poor. OpenCV's [calibration and reconstruction documentation](https://docs.opencv.org/4.13.0/d9/d0c/group__calib3d.html) distinguishes image transforms from calibrated pose recovery.

Initial feature families:

| Visual evidence | Activity comparison | Restriction |
| --- | --- | --- |
| Horizontal flow/turning envelope; calibrated yaw only if available later | Signed heading change and absolute turn rate | Evaluate a small fixed set of clip-wide polarity hypotheses; never choose signs independently per event. |
| Expansion/residual flow trend | Speed trend and speed change | Weak supporting evidence; normalize within comparable windows and reject textureless/foreground-dominated intervals. |
| Sustained low translational flow plus transitions | Stop/start patterns in parsed speed and their duration | Parsed stops may include synthetic idle data; require corroboration. Lack of trackable texture is unknown, not a stop. Camera rotation at rest may remain nonzero. |

Turn magnitude and signed turn rate form one family. Speed and its derivative form one family. Several correlated transforms of the same signal do not count as independent corroboration. Camera orientation assumptions are hypotheses tested consistently across the clip; unsupported mounts should reduce acceptance, not trigger unlimited hypothesis search.

## 6. Sampling and fingerprint construction

Represent fingerprints in elapsed seconds with per-channel valid duration, quality and estimated effective sample count. Estimates describe support available in the parsed signals, not guaranteed raw sensor provenance. Never align vectors by index or use total sample count divided by total duration as the sole sampling-rate estimate.

1. Split channels into contiguous supported spans using available parsed timestamps and valid values. Measure positive timestamp differences locally and account for visible timestamp quantization. Do not bridge observable long gaps or clock discontinuities. Already filled idle intervals cannot always be distinguished from measured stops; evaluate their impact during manual testing.
2. Derive heading changes with circular differences, expressed in radians per second over the actual interval. Mask heading where GPS displacement is insufficient relative to position noise. Derive speed changes with time-aware local fits; do not amplify 40 Hz quantization by naively differentiating each row. Use physical units and time-based windows, not sport-specific sample counts.
3. Respect effective bandwidth: a 40 Hz activity with visibly sparse 1 Hz GPS contributes approximately 1 Hz GPS-derived heading evidence. Use valid coordinate timestamps and existing metric coverage metadata where available; when cadence/provenance is uncertain, estimate support conservatively and report the limitation. Repeated values must not silently become independent evidence, but must not automatically be classified as held samples either.
4. Build a common coarse level at approximately 1 Hz (or slower for locally poorer support), with additional 2- and 4-second summaries for broad patterns. These are search levels; retain the original parsed series for refinement.
5. Apply a time-aware low-pass filter before decimation, targeting a cutoff below the destination Nyquist frequency, then aggregate by supported duration. Use symmetric offline filtering with explicit edge masks; never filter across gaps. Aggregate interval quantities by overlap duration, not by number of samples. Keep stop/turn event timing separately so brief distinctive events are not lost in averages.
6. For retained candidate regions, build finer shared levels capped by `min(activity_channel_effective_rate, visual_effective_rate)`. Re-analyze video windows at a higher rate if necessary. A 40 Hz channel and 10 Hz video may refine at up to 10 Hz without a re-decode; a 1 Hz channel must never become 10 Hz primary evidence.

Uniform bins summarize the existing parsed signals with support estimates; they do not create new measurements. Empty or insufficiently supported bins remain invalid. Fine offset evaluation may interpolate already band-limited signals within valid spans after localization, but does not increase their effective sample count or justify frame-accurate confidence.

Example: a six-hour 40 Hz source has about 864,000 rows. The initial one-second search still covers about 21,600 positions, while fine local matching retains the genuine high-rate information. Mixed 1/5/40 Hz sections refine at their local supported rates; one global rate choice must not force the whole recording to the fastest or slowest section.

## 7. Whole-activity matching

### Search domain and coverage

Search all eligible elapsed activity time regardless of current trim or timestamp-derived alignment. For a fully contained clip of duration `V` in activity duration `A`, starts are `[0, A - V]`. Also evaluate partial overlaps, including negative offsets and video longer than the activity, within `(-V, A)`, but require enough overlapping valid evidence. Support duration and overlap fraction enter acceptance; a tiny overlap cannot beat a well-supported full match by having fewer disagreements.

Initially require at least 20 seconds of valid paired evidence and two informative non-overlapping subsegments for an applicable result; adjust these starting thresholds after manual testing of the integrated feature with real recordings. For a 60-second fully overlapping clip, target at least 70% supported duration. Low-overlap or very short clips can return insufficient evidence without error. These are evidence requirements, not hard-coded input duration limits.

### Candidate generation

At each coarse offset, calculate masked, quality-weighted normalized cross-correlation for compatible continuous channels. Compute means and variances on the same paired support. A near-constant channel has no correlation evidence; never divide by a small variance and manufacture a high score. Match stop/start and turn events by temporal proximity, direction where supported, ordering and duration, including unmatched-event penalties.

For continuous channel `k`, use weighted covariance divided by paired weighted standard deviations. Combine channel scores into fixed feature families, then compute:

`candidate_score = weighted_family_similarity - missing_support_penalty - event_disagreement_penalty`

Define weights and minimum support before the search; do not tune them separately to rescue each candidate. Document exact normalization and penalty functions in code alongside deterministic tests. A candidate missing a contradictory feature cannot earn the same evidence score as one supported by that feature.

Retain, initially, the best 20 distinct candidate regions from the union of full-clip, individual-family, and informative-window searches. Merge neighboring positions from one peak using its width and support bandwidth; second-best means another plausible region, not the adjacent offset bin. Preserve competing repeated laps. A first-pass top-K is a computational shortlist, not evidence that discarded candidates are impossible.

Avoid losing short high-rate turns in coarse pruning: include native-timed event votes and overlapping informative windows in candidate generation. If coarse features are flat but native event evidence is rich, perform the next supported full-activity level before pruning. When a real recording fails manual alignment checks, inspect whether the correct region survived candidate generation; do not accept a sharp refined peak when the coarse stage demonstrably cannot distinguish the alternatives.

### Refinement and independent checks

Refine every retained region using matched-bandwidth features and finer offset steps. Recompute comparable evidence for all contenders, not only the initial winner. Offset grid spacing is numerical resolution; report a separate uncertainty interval determined by temporal support, peak breadth and disagreement.

For a 60-second clip, independently search three roughly 20-second sections across the whole activity. For shorter clips, use two sections if they contain enough evidence; for long videos, select informative sections distributed through the clip, retaining their original times. A section beginning at video time `u` and matching activity time `m` votes for `offset = m - u`.

Use some sections to nominate candidates and held-out sections to confirm them. At least two informative disjoint sections must agree within support-derived tolerance. Strong contradictory sections veto acceptance; uninformative ones do not count as agreement. Temporal disagreement increasing through a long clip suggests drift or edits and returns a reasoned no-match under the single-offset model.

Analyze long videos in bounded chunks, retaining compact fingerprints. Full coarse matching can use masked FFT correlation/prefix statistics once direct matching is too expensive; keep the direct matcher as a correctness oracle. Partial-overlap normalization must be identical. Ordinary 60-second versus six-hour matching at 1 Hz is approximately 1.3 million bin comparisons per channel, so start with the simpler direct implementation. Add FFT dependencies only if profiling a slow real-world run identifies matching as the bottleneck.

## 8. Confidence and rejection

Keep similarity, confidence and temporal uncertainty distinct. Raw correlation is not a probability of correct alignment.

An applicable candidate must satisfy these evidence gates, with thresholds tuned through manual testing:

- Enough valid duration, distinctive variation/events and background tracking quality.
- Agreement from at least two feature families for the initial conservative release, plus independent temporal confirmation. This deliberately abstains on clips with only one usable family.
- Separation from the best other distinct region, accounting for comparable support.
- A localized peak with acceptable timing uncertainty; the true candidate can be at an activity boundary and must not be rejected solely for being there.
- No strong held-out contradiction or evidence of an incompatible time mapping.
- A heuristic confidence threshold accounting for the entire search procedure, including activity length, rate, number of windows, polarity hypotheses, and refinements.

During manual testing, try matching and unrelated real video/activity pairs, including repeated laps and low-motion clips. Check whether the full search proposes a convincing wrong result or correctly reports ambiguity. Nearby samples, repeated laps, and derived feature families are correlated; raw sample count must not inflate confidence.

The first implementation uses an explicitly heuristic confidence/evidence score with qualitative labels. It is not a statistically calibrated probability of correct alignment, and the UI must not present it as one. Candidate buttons show this score, estimated timing uncertainty, overlap, and concise evidence/rejection details. No synthetic surrogate generation, calibration corpus, or statistical benchmark is required before integration or manual acceptance.

Use explicit outcomes: `matched`, `no_match`, `cancelled`, or job error. `no_match` carries reasons such as insufficient motion, insufficient source observations, ambiguous repeated patterns, contradictory segments, or unsupported time mapping. It may carry ranked diagnostics for manual inspection, but no applicable candidate. Do not return offset zero as a substitute for no-match.

## 9. IPC and Sync Doctor UI

Add start/cancel/status commands and a progress event with one canonical serialized schema. Start receives imported video identity/path, activity identity/revision, the existing parsed activity, and validated options; returns a backend-generated job ID. Progress/status include job ID, input revisions, stage, progress, and a terminal outcome. Subscribe before starting and reconcile with status so very fast completion cannot be missed.

Keep serialized field names consistent with existing backend API conventions; no hand-written alias objects in components. Candidate data includes a stable backend candidate ID, `offset_seconds`, heuristic confidence score, `uncertainty_seconds`, overlap/support duration, family evidence and applicability. Map the domain result to the existing offset action once in the owning hook.

Add a Sync Doctor tool to `createLayoutSlice`, toolbar definitions, drawer routing and translations. `useVisualSync` owns request state, cancellation, subscriptions and candidate selection; selectors expose store data. Components render the button, loader overlay, current stage, cancel control, candidates and no-match guidance. The drawer can close while job state remains owned above drawer mount lifetime; changing/removing either input cancels and invalidates the job.

States: idle → analyzing → matching → completed/no-match/error, with cancellation from either work stage. Ignore progress/results from stale jobs or changed activity/video revisions. Applying a candidate checks that its input revisions still match and calls `setVideoSyncOffset`; preview/export keep using the existing offset. Candidate selection is the explicit application action. No-match leaves manual alignment available.

Multiple plausible candidates can be shown for inspection, but ambiguous candidates must be labeled and cannot masquerade as confident one-click alignment. This reconciles the requested candidate list with the requirement to reject unreliable synchronization.

Cache compact fingerprints in memory initially, keyed by source identity/change revision, analysis version, frame rate/resolution and calibration settings. Activity fingerprints depend on parsed activity revision and fingerprint processing version. Bound cache size. Do not persist extracted images or introduce an unbounded whole-video frame cache.

## 10. Delivery sequence and manual acceptance

| Phase | Deliverable | Verification / exit condition |
| --- | --- | --- |
| 1. Native integration and Gyroflow extraction | Minimal OpenCV configuration, extracted PyrLK/homography, provenance notices, timestamp-aware decoder with cancellation. | Decoder and tracking are wired into the analysis pipeline with bounded memory and child cleanup. |
| 2. Parsed-activity multirate fingerprints | Consume existing `sample_elapsed_seconds`, `course`, `speed` and `heading`; implement support estimates, feature computation, anti-aliasing and common-rate levels. No new activity fields or parser/finalizer changes. | Existing imported and saved activities can feed matching without re-import. |
| 3. Whole-activity matcher and confidence | Exhaustive coarse search, event/window nomination, distinct peak grouping, refinement, partial overlap, long-video chunking, independent sections and heuristic confidence gates. | Pipeline returns candidates with evidence or an explicit no-match outcome. |
| 4. Commands and UI | Managed analysis jobs, Sync Doctor drawer, progress/cancel, candidate application and stale-result protection. | Complete feature is available for the user's manual testing in OVRLEY. |
| 5. Manual acceptance and tuning | Test the final integration with real videos and activities; fix observed issues and adjust algorithm settings. | User can inspect/apply useful matches, verify alignment in preview/export, and exercise cancellation and no-match behavior. Record specific failures that could justify chapter 12. |
| 6. Distribution integration | Cross-platform dependency packaging and usage documentation. | When builds are authorized, verify the packaged app works without a developer OpenCV installation. |

Start directly with implementation. Phases 1–4 deliver the complete integration for manual testing; no preliminary feasibility study, standalone benchmark harness, synthetic dataset or calibration stage is required. Keep implementation checks focused on concrete correctness and lifecycle behavior, then evaluate matching quality in the actual application.

### Manual testing in the integrated app

Use the user's available real recordings; these are practical scenarios to try, not a requirement to assemble an exhaustive corpus before delivery.

- Import an activity and a video with recognizable turns or stops. Run Sync Doctor, apply a candidate, and scrub several separated events to verify the alignment holds through the clip. Check export uses the same offset.
- Try a short clip within a long activity, preferably the one-minute/six-hour case when available. Also try longer clips and matches near activity boundaries, including partial overlaps.
- Try real activities with different rates across 1–40 Hz and sparse GPS where available. Note whether matching or refinement changes unexpectedly; do not generate synthetic rate variants as an acceptance prerequisite.
- Try repeated laps, steady straight travel and an unrelated activity. Ambiguity or insufficient evidence should produce understandable results rather than a misleading confident match.
- Cancel analysis, rerun it, close/reopen the drawer, replace the activity/video and attempt to use stale results. Verify the loader clears and previous jobs cannot apply offsets to new inputs.
- Open an existing saved project and confirm visual matching works without new activity fields or re-import.

For a failure, record the real input pair, expected approximate offset, returned candidates and visible symptom. Use existing diagnostics to identify whether the problem is decoding, image motion, fingerprinting, pruning or confidence. Tune against these concrete observations. Manual acceptance does not establish a statistical false-match rate or calibrated timing interval, so do not claim either.

Observe responsiveness, analysis duration and cancellation during these runs. Investigate performance if the integrated feature is too slow or consumes excessive memory; there is no separate synthetic performance benchmark or percentile report requirement. Never promise sub-frame accuracy for 1 Hz source data.

During implementation, run relevant existing regressions and lint, with small focused checks for offset arithmetic, input contracts or job cleanup where useful. Do not build a synthetic matching-quality benchmark suite. No build or dependency installation has been performed for this document.

## 11. Decisions settled and remaining empirical work

Settled: Rust/OpenCV sparse tracking; direct Gyroflow extraction including homography fitting from `estimate_pose`; uncalibrated image-space proxies as the default; existing `ParsedActivity` as the initial activity input; whole-activity multiresolution search; one constant offset; explicit abstention; existing preview/render offset integration. `sync_observations` is explicitly outside the first implementation.

Implementation and manual tuning decisions: frame rate and fitting thresholds, useful feature families, minimum supported duration, candidate budget and heuristic confidence thresholds. Keep settings in the algorithm configuration and adjust them based on observed behavior in the integrated app. Resolve OpenCV/toolchain versions during native integration. No preliminary research phase or statistical acceptance target is required.

## 12. Optional follow-up: `sync_observations` only if the tested first implementation is insufficient

**Deferred: do not implement `sync_observations` during the first implementation.** This chapter is a possible subsequent improvement, not a delivery phase, prerequisite, or instruction to add schema scaffolding now. Complete and test matching against the existing parsed activity first. If it meets the acceptance criteria, this chapter requires no work.

### Evidence required before adding it

Consider this extension only when reproducible failures show that information lost during activity processing materially harms matching: for example, heading smoothing shifts a distinctive turn, filled idle intervals cause false acceptance, or channel timing lost during alignment prevents useful refinement. Distinguish those failures from poor optical flow, camera movement unrelated to travel, weak scene evidence and matcher defects; extra activity observations do not solve those problems.

Reproduce a failure found during manual testing with the same real video/activity pair. Compare the existing parsed signals with a diagnostic capture of the relevant pre-processing values, keeping video analysis and matching settings fixed. Verify the practical alignment improvement in the app and recheck other available recordings. No synthetic benchmark or held-out corpus is required. A diagnostic experiment after first-version testing does not itself require a production schema change. Record which information is needed, the observed benefit and the smallest change that preserves it; a synthetic-sample marker or one pre-smoothing channel may be sufficient instead of a complete new field.

### Proposed contents if the extension is justified

`ParsedActivity.sync_observations` would contain normalized source observations used only for fingerprint matching. It would not duplicate all activity metrics or contain finished fingerprints. Each channel has its own elapsed timestamps on the existing activity time origin, so a 1 Hz GPS channel can coexist with 40 Hz direct speed.

| Channel | Contents | Units |
| --- | --- | --- |
| `course` | Valid source latitude/longitude pairs and their available timestamps | Decimal degrees |
| `speed` | Directly supplied speed values and their available timestamps | m/s |
| `heading` | Directly supplied heading values and their available timestamps | Degrees |

Illustrative shape (the arrays in each channel must have equal length):

```json
{
  "sync_observations": {
    "course": {
      "sample_elapsed_seconds": [0, 1, 2],
      "values": [[52.0, 13.0], [52.00005, 13.0], [52.0001, 13.00002]]
    },
    "speed": {
      "sample_elapsed_seconds": [0, 0.025, 0.05],
      "values": [5.1, 5.2, 5.2]
    },
    "heading": {
      "sample_elapsed_seconds": [],
      "values": []
    }
  }
}
```

Empty arrays mean that channel has no usable source observations. Missing samples are omitted while their time gaps remain; no synthetic idle values or new interpolation are inserted into these arrays. GPS-derived speed/turns remain fingerprint computations using timestamped coordinates and shared mathematical utilities. Additional support intervals or provenance should be added only where needed by the demonstrated failure and actually known upstream. Source-repeated coordinates still cannot reveal a hidden receiver update rate; this field cannot restore information absent from the file or already discarded before capture.

### Integration if adopted later

Populate the justified data during the existing import/finalization pass, after format normalization and before the specific smoothing/filling operation shown to harm matching. Reuse `RawActivity`/`ActivityColumns` and existing time/unit normalization; do not reopen files, implement parallel format parsers or run a second finalization pipeline. Preserve the same elapsed-time origin as `ParsedActivity`, including when leading samples lack GPS. Audit both row-based and columnar paths for the chosen capture point; touch format-specific extraction only where the proven information loss occurs there.

Keep the normal parsed series and rendering behavior intact. Define serialization and absence semantics at the canonical schema owner. Activities saved before the extension continue using the first implementation's parsed-data path; absence is documented optionality, not malformed data. Re-import may be offered to obtain enhanced observations, but is not required to run baseline matching. Present malformed observation data fails validation at ingress rather than silently switching paths. Select the activity feature source once in fingerprint preparation, with one internal fingerprint contract and no consumer-level adapters.

Before shipping the extension, verify save/load, affected import paths, timestamp origin, missing channels, sparse versus repeated GPS, exclusion of synthetic values and cache invalidation. Manually confirm the matching improvement on the recordings that motivated the extension. Revisit heuristic confidence thresholds if the new input materially changes scoring. These obligations apply only if this optional extension is adopted after testing the first implementation.
