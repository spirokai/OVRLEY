# What manual video-to-data sync workflows exist in the industry?

Status: closed
Labels: wayfinder:research

## Question

What manual video-to-data (or video-to-audio/telemetry) sync workflows are used in video editing, action-cam, and telemetry-overlay tools? Which are one-point, two-point, or multi-point? Which are suited to correcting a pure time offset (not stretch) across a large range? Surface concrete examples from tools like GoPro Quik, Garmin VIRB Edit, Dashware, RaceRender, Final Cut Pro, Premiere, DaVinci Resolve, etc., and extract the pattern names and tradeoffs.

## Blocking

None.

## Comments

## Resolution

Research complete. Key findings:

- The industry uses roughly three sync strategies: **automatic metadata alignment** (one implicit point), **single-event sync wizards** (one explicit point), and **numeric/slider offsets** (continuous adjustment).
- **Two-point or multi-point sync** is largely an audio/video multicam concept (markers, waveforms, timecode) and is not common for GPS/speed/elemetry overlays.
- Concrete examples:
  - **RaceRender 3**: Data Sync Wizard (single event) + numeric "Starting Position" / "Offset Within Project".
  - **Dashware / Garmin VIRB Edit**: dual-timeline scrub with a "Sync with Video" checkbox; VIRB also offers map-based G-Metrix Sync.
  - **GoPro Telemetry Overlay**: automatic metadata sync, single-event "Starts/Ends Now", and a fine-grained offset slider.
  - **GPStitch**: auto metadata/timezone detection with a fallback manual offset in seconds.
  - **Premiere / Final Cut / DaVinci Resolve**: timecode, markers, and audio waveform sync — powerful but overkill and not data-source friendly.
- **Recommendation for OVRLEY**: default to automatic metadata sync, offer a one-point "sync data start to current frame" fallback, and expose a numeric offset slider with live gauge/map preview and clear early/late labels.

Full findings are recorded in [`01-industry-workflows-findings.md`](../01-industry-workflows-findings.md).
