# Visual synchronization matching

`matcher::match_fingerprints` is the single active matcher for direct analysis,
managed editor jobs, and the `match_motion` command-line tool. All return the
canonical `MatchResult`. Managed jobs report cancellation as a separate terminal
kind, not a synthetic match result. Matching gates are owned by the backend;
the start request contains analysis settings but no obsolete confidence settings.

The matcher currently measures signed-turn correlation only. Speed features remain
in the fingerprint for subsequent speed-matching work. Raw-threshold turn events,
turn magnitude, speed proxy and stop events do not currently affect scoring.
The drawer displays accepted and rejected candidates, correlation, observed
duration, per-section agreement and rejection reasons. Only accepted candidates
from the current inputs can be applied, by their canonical offset in seconds.

## Observation accounting

- Four-second bins average actual feature intervals by observed duration.
- At least half of a bin must be observed; no missing run may exceed two seconds.
- Missing time contributes neither values nor observed duration. No interpolation
  or full-neighborhood filter is applied.
- Explicit timestamp/geometry discontinuities, invalid heading and GPS jumps
  prevent a bin from crossing the discontinuity. A `PoorFit` hole may be included
  within the missing-duration bound; it is not silently converted to zero motion.
- Bin pairing measures the exact intersection of observed time ranges. Source
  means are estimated independently within each bin; covariance weights use
  intersected duration times the smaller source quality.
- `video_observed_seconds` measures native accepted turning intervals;
  `video_binned_observed_seconds` measures those retained in eligible bins.
- Candidate `observed_seconds` and `observed_fraction` measure paired observations
  against full clip duration. `retained_video_observation_fraction` measures how
  much eligible video evidence found activity support. `correlation` measures
  agreement separately; none of these values is a probability.

## Search and acceptance gates

Search covers full-clip placements in the activity at one-second steps. It uses
one clip-wide sign. The first two nonoverlapping thirds nominate candidates by
absolute correlation; the last third does not nominate or rank candidates.
The trailing partial four-second bin is omitted, but full clip duration remains
the coverage denominator.

Nomination requires 80 observed seconds, 20% of nomination time, and 80% of
available binned video observations. Candidate regions are separated by 30 seconds.
Acceptance requires:

- At least 120 paired observed seconds, 20% of full clip duration, and 80% of
  available binned video observations.
- Whole-clip correlation at least 0.6 after choosing the nomination polarity.
- Nomination correlation margin at least 0.08 over the strongest other region.
- All three sections have correlation at least 0.5, at least 20 observed seconds,
  20% of section duration, and 80% of available section observations.
- Each section's independent whole-activity best offset, with the same polarity,
  agrees within eight seconds. The final section is held-out verification.

These gates are deliberately explicit experimental constants. Short clips,
partial activity overlap and subsecond refinement are not currently supported.
One positive recording and constructed negatives do not calibrate general
false-positive rates. A cut that appears only as `PoorFit` cannot be identified
from the current dump; only explicit discontinuities and gap lengths are known.

## Verification with the local Pragelpass recording

From the repository root in PowerShell:

```powershell
. ./scripts/setup-motion-native.ps1
cargo test --manifest-path src-tauri/ovrley_core/Cargo.toml --lib synchronization::matcher -- --include-ignored
```

The ignored local-data test reads:

- `debug/activities/Pragelpass_nearly_killed_me-parse-debug.json`
- `target/debug/motion-analysis/Pragelpass.motion.jsonl`

It checks the known offset range 17236–17252 and repeats the search after removing
heading/GPS observations in activity time 17100–18300, covering the true match
with a buffer. It writes both reports to
`target/debug/motion-analysis/matcher-regression.json` before asserting results.
No video decoding or application build is needed.

Portable synthetic tests cover fragmented known matches, exact observed-time
intersection, excessive holes, flat/sparse signals, periodic ambiguity,
contradictory held-out data and cancellation.
