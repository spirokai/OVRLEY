---
name: showcase-avif
description: Encode an animated AVIF showcase clip from an OVRLEY screen recording — crop a widget region, or append it beside an existing showcase AVIF. Use when the user asks for a showcase/sample AVIF, a README animation, or to add a widget to an existing showcase.
---

Turns a screen recording (usually OBS `.mp4` in `~/Documents/OVRLEY/`) into a crisp animated AVIF for `docs/`. Run commands in the Bash tool from the repo root; ffmpeg is `vendor/ffmpeg/bin/ffmpeg`, ffprobe is `vendor/ffmpeg/bin/ffprobe`. Scratch frames go in the session scratchpad.

## Steps

1. **Probe** every input: `ffprobe -v error -show_entries stream=codec_name,width,height,r_frame_rate,pix_fmt:format=duration -of compact <file>`. Done when you know each input's size, fps and duration, and the clip window (`-ss`/`-to`) fits inside the source.
2. **Locate the crop.** Grab one frame (`-ss <t> -frames:v 1 frame.png`) and read it to estimate the widget box. Then render a contact sheet over the whole clip window — `-ss A -to B -i src -vf "fps=1/5,crop=W:H:X:Y,tile=10x1" -frames:v 1 sheet.png` — and read it. Done when the widget sits fully inside the crop with margin in **every** tile (values, labels and indicators change width over time).
3. **Encode** with the recipe below. Done when ffmpeg exits cleanly.
4. **Verify**: `ffprobe -v error -count_frames -select_streams v:1 -show_entries stream=width,height,pix_fmt,nb_read_frames,avg_frame_rate -of compact out.avif`, then extract a mid-clip frame (`-ss <t> -i out.avif -map 0:v:1 -frames:v 1`) and read it. Done when frame count = duration × fps, pix_fmt is `yuv444p`, and the frame shows the widget sharp with no seam or level mismatch at any join.

## Recipe

Single clip, native scale:

```
ffmpeg -y -ss A -to B -i src.mp4 -an \
  -vf "crop=W:H:X:Y,hqdn3d=1:1:0:0,unsharp=5:5:0.6:5:5:0,format=yuv444p" \
  -c:v libaom-av1 -crf 22 -b:v 0 -cpu-used 4 -row-mt 1 -tiles 2x2 -still-picture 0 \
  -color_primaries bt709 -color_trc bt709 -colorspace bt709 docs/out.avif
```

- `hqdn3d=1:1:0:0` removes the recording's spatial compression noise and leaves temporal detail alone; `unsharp 0.6` restores edge crispness on text and ticks. Both go **after** the crop.
- Keep native scale. When a wider variant is wanted (e.g. 1920), insert `scale=1920:-2:flags=lanczos+accurate_rnd+full_chroma_int` after `hqdn3d` and raise unsharp to `0.7`.
- A 50 s, ~1200×670 clip encodes in about 1 minute and comes out around 1 MB.

Append a new widget beside an existing showcase AVIF:

```
ffmpeg -y -i docs/existing.avif -ss A -to B -i src.mp4 -an -filter_complex \
  "[0:v:1]fps=30,format=yuv444p,setpts=PTS-STARTPTS[a];\
   [1:v]fps=30,crop=W:H:X:Y,hqdn3d=1:1:0:0,unsharp=5:5:0.6:5:5:0,format=yuv444p,setpts=PTS-STARTPTS[b];\
   [a][b]hstack=inputs=2:shortest=1[v]" \
  -map "[v]" <same encoder flags as above> docs/new.avif
```

- The existing AVIF was already denoised and sharpened, so it passes through untouched; only the new clip gets the filters.
- `hstack` needs equal heights: set the crop `H` to the existing showcase height (`vstack` needs equal widths). Center the widget vertically in that band, and leave about 50 px of black on the far side to match the existing margins.
- Match the clip window length to the existing showcase duration so both loop together; `fps=30` normalises OBS variable frame rate.

## Animated AVIF quirk

An animated AVIF holds two video streams: `v:0` is a single still cover frame and `v:1` is the animation. Always decode with `-map 0:v:1` (or `[0:v:1]` in a filtergraph); otherwise ffmpeg reads the 1-frame cover and seeks fail silently.
