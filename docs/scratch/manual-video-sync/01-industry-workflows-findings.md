# Industry Workflows for Manual Video-to-Data Sync

**Scope:** Workflows for correcting a pure time offset between a video clip and a telemetry/GPS/activity data file, not for correcting time-stretch/drift. Focus on action-cam, telemetry-overlay, and general video-editing tools.

## Summary

Most tools approach the problem in one of three ways:

1. **Metadata / automatic alignment** (one implicit point): use file creation time, embedded timecode, GPS timestamps, or audio LTC.
2. **Single-event sync** (one point): the user identifies one matching moment in the video and in the data, and the tool computes the offset.
3. **Numeric / slider offset** (continuous adjustment): the user directly changes an offset value while watching a live preview.

Two-point or multi-point sync exists, but it is mostly found in audio/video multicam workflows (clapper boards, waveform matching) and is rarely used for GPS/speed/elemetry overlays. For a layman user aligning a short action-cam clip to a GPS track, **single-event sync combined with a numeric offset slider and live preview is the most robust pattern**.

## Patterns Found

### 1. Automatic metadata sync
- **Tools:** GoPro Telemetry Overlay, GPStitch, GoPro Quik (performance stickers), Garmin VIRB Edit (auto attempt), DaVinci Resolve / Premiere / Final Cut Pro (timecode).
- **How the user marks sync:** No manual marking; the tool compares the video's embedded `creation_time`, file timestamp, timecode track, or GPS timestamps to the data file's time range.
- **Points:** One implicit point (the start time).
- **Visual aids:** Overlaid gauges, map path, timeline cursors, sometimes a warning banner when timestamps do not overlap.
- **Tradeoffs:**
  - *Fast:* usually one click or automatic on import.
  - *Precision:* depends entirely on camera clock accuracy and timezone handling; often off by seconds to hours.
  - *Learning curve:* low for the happy path, but debugging timezone/clock errors is confusing for non-technical users.

### 2. Single-event sync wizard
- **Tools:** RaceRender 3 (Data Sync Wizard), GoPro Telemetry Overlay ("Starts/Ends Now", "Sync to In/Out Point"), Garmin VIRB Edit (G-Metrix Sync).
- **How the user marks sync:** The user scrubs the video to a recognizable event (start moving, cross a junction, brake light, start/finish line, data begins), selects the matching data event, and clicks a sync button.
- **Points:** One point.
- **Visual aids:** Main video preview + live gauges (speedometer, G-force, track map) that update as the offset changes.
- **Tradeoffs:**
  - *Speed:* fast when a clear event exists; slower if the user has to hunt for one.
  - *Precision:* frame-ish; fine-tuning may still be needed.
  - *Learning curve:* low-to-medium; the user must understand what "event" to pick.

### 3. Dual-timeline scrub + "Sync with Video" checkbox
- **Tools:** Dashware, Garmin VIRB Edit (G-Metrix Sync).
- **How the user marks sync:** Two timelines are shown side-by-side (video on the left, data on the right). The user scrubs each to the same event, then ticks a "Sync with Video" checkbox.
- **Points:** Effectively one point, but both timelines are manipulated independently.
- **Visual aids:** Dual playheads, frame-step buttons, live gauge preview, sometimes a map.
- **Tradeoffs:**
  - *Speed:* slower because the user coordinates two timelines.
  - *Precision:* moderate; frame-step controls help.
  - *Learning curve:* medium; the split control can be fiddly and the relationship between the two timelines is not always obvious.

### 4. Numeric / slider offset
- **Tools:** GoPro Telemetry Overlay (offset slider + seconds/ms fields), RaceRender 3 ("Starting Position in Input File" / "Offset Within Project"), GPStitch (manual offset in seconds), Garmin VIRB Edit XML workarounds.
- **How the user marks sync:** The user types or drags an offset value and watches the preview until gauges align.
- **Points:** None explicitly; continuous adjustment.
- **Visual aids:** Live preview with overlaid gauges, before/after timestamp display, sometimes a duplicate gauge for A/B comparison.
- **Tradeoffs:**
  - *Speed:* fast if the approximate offset is already known; slow if the user has to search a wide range.
  - *Precision:* high, often down to milliseconds or frames.
  - *Learning curve:* low for simple cases; harder when the offset is large or the sign is unclear.

### 5. Map-based sync
- **Tools:** Garmin VIRB Edit, GoPro Telemetry Overlay (Sync Assistant map gauges).
- **How the user marks sync:** The user scrubs the video to a landmark (intersection, turn, bridge) and matches it to the same location on the GPS track map.
- **Points:** One point.
- **Visual aids:** Side-by-side or overlaid map and video; full-activity map + zoomed current-position map.
- **Tradeoffs:**
  - *Speed:* moderate; requires a distinctive landmark.
  - *Precision:* depends on GPS accuracy and map detail; can be off on straight roads or poor signal.
  - *Learning curve:* low conceptually, but GPS noise can make it frustrating.

### 6. Marker / In-Out / two-point sync (video-audio world)
- **Tools:** Adobe Premiere Pro (Clip > Synchronize: Clip Start/End, Timecode, Clip Marker), Final Cut Pro (multicam angle editor, markers, "Sync to Monitoring Angle"), DaVinci Resolve (Auto-Align Clips Based on Timecode, Audio Offset column).
- **How the user marks sync:** The user sets markers, in/out points, or relies on timecode/audio waveforms on two or more clips.
- **Points:** One or two points (start/end, head/tail slate).
- **Visual aids:** Waveforms, timecode displays, two-up viewer, angle editor, audio offset columns.
- **Tradeoffs:**
  - *Speed:* fast with timecode or clear slates; slow with manual markers.
  - *Precision:* very high, often sample-accurate for audio.
  - *Learning curve:* high for casual users; these tools are not designed around telemetry data, and importing a GPX/FIT file as a sync target is not supported directly.

### 7. Audio waveform sync
- **Tools:** Premiere Pro, Final Cut Pro, DaVinci Resolve.
- **How the user marks sync:** The tool analyzes audio waveforms across sources and aligns them; users can fine-tune by nudging clips sub-frame.
- **Points:** Many implicit points across the waveform.
- **Visual aids:** Waveform overlays, two-up viewer, audio offset columns.
- **Tradeoffs:**
  - *Speed:* automatic batch syncing, but analysis can be slow.
  - *Precision:* very high.
  - **Not applicable to GPS/speed data**, only useful if the user is syncing an external audio recorder to the action-cam clip.

## Recommendation for OVRLEY

For a layman user aligning a short action-cam clip to a GPS/speed/elevation activity where only an offset needs correction, the best workflow combines:

1. **Automatic metadata sync as the default** (`creation_time`, file timestamp, or GPS timestamp) so most users never see the sync UI.
2. **A single-event "sync here" fallback** when metadata is unreliable: pause the video at a clear event (start moving, cross a line, start/stop recording), click "Set data start to current frame", and let the tool compute the offset.
3. **A numeric offset slider with live preview** for fine-tuning: show the current offset in seconds and milliseconds, update overlaid gauges and a map in real time, and include small arrow buttons for frame-level nudging.
4. **Clear sign/direction labeling** (e.g., "data is X seconds early/late") so users do not have to reason about positive vs. negative offsets.

This pairing gives the speed of automatic sync, the intuitiveness of a one-point wizard, and the precision of direct numeric control, while avoiding the complexity of professional multicam tools or fiddly dual-timeline scrubbing.

## Sources Consulted

- RaceRender 3 documentation: Synchronization Tool, Input File Configuration, Data Sync Wizard.
- Dashware tutorials and wikis (rocketwiki.danno.org, smtperformances.com, mavicpilots.com).
- GoPro Telemetry Overlay instruction manual and YouTube synchronization tutorial.
- Garmin VIRB Edit forums and support articles (G-Metrix Sync, Activity Start Time, XML workarounds).
- GPStitch README and issue tracker (auto/manual/timezone sync modes).
- Apple Final Cut Pro support: multicam clip sync, angle editor, audio sync.
- Adobe Premiere Pro help: Synchronize clips, Merge Clips, timecode workflows.
- DaVinci Resolve documentation and tutorials: timecode, audio offset, auto-align clips.
- GoPro Quik / GPS Quick Fix documentation and community FAQ.
