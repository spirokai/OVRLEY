# Render Baseline Suite

Fixture-driven integration tests that render frames, transparent overlays,
and composite videos through `ovrley_core` and compare decoded output
against committed pixel baselines.

Three test functions live in `render_baseline_suite.rs`:

- **`rendered_frames_match_baselines`** — preview PNGs from activity/config fixtures
- **`transparent_videos_match_baselines`** — short transparent videos, frame-decoded comparison
- **`composite_videos_match_baselines`** — short composite MP4s against a source video fixture

## Use Cases

- Regression detection for render, encode, and composite output changes
- CI gate to verify pixel-identical output across commits
- Baseline recording/updating after intentional output changes

## Running

In powershell from src-tauri/ovrley_core:

```bash
# Record/update baselines (after intentional output changes)
$env:OVRLEY_RECORD_BASELINES='1'; cargo test -p ovrley_core --test render_baseline_suite -- --ignored --nocapture

# Compare against committed baselines (normal CI run)
$env:OVRLEY_RECORD_BASELINES='0'; cargo test -p ovrley_core --test render_baseline_suite -- --ignored --nocapture
```

### Flag explanations

| Flag                        | Meaning                                                                                                                                                                                                                                                                              |
| --------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `--nocapture`               | Cargo test flag passed after `--`. Without it, `println!` output is hidden unless the test fails. The suite uses `println!` to report per-case parity results and progress — `--nocapture` lets you see those live.                                                                  |
| `OVRLEY_RECORD_BASELINES=1` | Environment variable that switches the suite from compare mode to record mode. When set, the suite overwrites the committed baseline PNGs with newly rendered output instead of comparing against them. Use this after intentionally changing render output (format, codec, layout). |

## Fixtures

- Activity JSONs and config JSONs under `tests/fixtures/activity` and `tests/fixtures/config` (manifests)
- Baseline PNGs under `tests/fixtures/render-baseline-suite.json` (manifest)
- Source video fixture: `tests/fixtures/video/test-1080p.mp4` (download-only)

## Failure Artifacts

On mismatch, the suite writes `actual.png`, `baseline.png`, and `diff.png`
to `target/render-baseline-suite/{kind}/{case_name}/failures/`.
