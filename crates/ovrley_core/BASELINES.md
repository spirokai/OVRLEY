# Render Baseline Suite

Fixture-driven integration tests that render frames, transparent overlays,
and composite videos through `ovrley_core` and compare decoded output
against committed pixel baselines.

Three ignored tests live in `tests/it/render_baseline_suite.rs`:

- **`rendered_frames_match_baselines`** — preview PNGs from activity/config fixtures
- **`transparent_videos_match_baselines`** — short transparent videos, frame-decoded comparison
- **`composite_videos_match_baselines`** — short composite MP4s against a source video fixture

## Use Cases

- Regression detection for render, encode, and composite output changes
- A check, run by hand, that a commit leaves the output pixel-identical. No CI job runs it.
- Baseline recording/updating after intentional output changes

## Running

From the repository root (bash or zsh):

```bash
# Record/update baselines (after intentional output changes)
OVRLEY_RECORD_BASELINES=1 cargo test -p ovrley_core --test it render_baseline_suite:: -- --ignored --nocapture

# Compare against committed baselines
cargo test -p ovrley_core --test it render_baseline_suite:: -- --ignored --nocapture
```

In PowerShell set `$env:OVRLEY_RECORD_BASELINES='1'` first, and `'0'` to compare again.

### Flag explanations

| Flag                        | Meaning                                                                                                                                                                                                                                                                              |
| --------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `--ignored`                 | The suite is ignored by default. Without this flag none of it runs. |
| `--nocapture`               | Cargo test flag passed after `--`. Without it, `println!` output is hidden unless the test fails. The suite uses `println!` to report per-case results and progress — `--nocapture` lets you see those live.                                                                  |
| `OVRLEY_RECORD_BASELINES=1` | Environment variable that switches the suite from compare mode to record mode. When set, the suite overwrites the committed baseline PNGs with newly rendered output instead of comparing against them. Use this after intentionally changing render output (format, codec, layout). |

## Fixtures

- The cases, with their activity, config and baseline paths: `tests/fixtures/render-baseline-suite.json`
- Baselines: `tests/fixtures/baselines/` (`frame/`, `transparent/`, `composite/`)
- Source video of the composite case: `tests/fixtures/video/test-4k.mp4`. It is not committed; put one there.

## Failure Artifacts

On mismatch, the suite writes `actual.png`, `baseline.png`, and `diff.png`
to `target/render-baseline-suite/{kind}/{case_name}/failures/`.
