---
name: showcase-avif
description: Encode an animated AVIF showcase clip from an OVRLEY screen recording (OBS .mp4) or a transparent OVRLEY overlay export (ProRes 4444 .mov with alpha) — crop a widget region, composite on black, or append it beside an existing showcase AVIF. Use when the user asks for a showcase/sample AVIF, a README animation, or to add a widget to an existing showcase.
---

Turns a source in `~/Documents/OVRLEY/` into a crisp animated AVIF for `docs/`. Two source kinds, each with its own recipe:

- **OBS screen recording** (`.mp4`, h264, already lossy) → [Screen recording recipe](#screen-recording-recipe-obs-mp4).
- **OVRLEY overlay export** (`.mov`, `prores` with `yuva444p12le`, transparent background, usually 3840×2160) → [ProRes 4444 recipe](#prores-4444-recipe-transparent-overlay-export).

Run commands in the Bash tool from the repo root; ffmpeg is `vendor/ffmpeg/bin/ffmpeg`, ffprobe is `vendor/ffmpeg/bin/ffprobe`. Scratch frames go in the session scratchpad.

## Steps

1. **Probe** every input: `ffprobe -v error -show_entries stream=codec_name,width,height,r_frame_rate,pix_fmt:format=duration -of compact <file>`. Done when you know each input's size, fps, pix_fmt and duration (which picks the recipe), and the clip window (`-ss`/`-to`/`-t`) fits inside the source.
2. **Locate the crop.**
   - Screen recording: grab one frame (`-ss <t> -frames:v 1 frame.png`) and read it to estimate the widget box. Then render a contact sheet over the whole clip window — `-ss A -to B -i src -vf "fps=1/5,crop=W:H:X:Y,tile=10x1" -frames:v 1 sheet.png` — and read it. Done when the widget sits fully inside the crop with margin in **every** tile (values, labels and indicators change width over time).
   - ProRes with alpha: measure the content box from the alpha channel over the whole clip — `ffmpeg -i src.mov -vf "fps=1,alphaextract,bbox=min_val=8" -f null - 2>&1 | grep -o "x1:.*y2:[0-9]*" | sort | uniq -c`. Done when you have the union box. Then pick a crop with the target aspect (match the existing `docs/` AVIFs, e.g. 16:9 for 1920×1080) centred on that box, with ~120 px (at 4K) padding on the tighter axis.
3. **Encode** with the matching recipe below. Done when ffmpeg exits cleanly.
4. **Verify**: `ffprobe -v error -count_frames -select_streams v:1 -show_entries stream=width,height,pix_fmt,nb_read_frames,avg_frame_rate -of compact out.avif`, then extract a mid-clip frame (`-ss <t> -i out.avif -map 0:v:1 -frames:v 1`) and read it. Done when frame count = duration × fps, pix_fmt is `yuv444p`, and the frame shows the widget sharp with no seam or level mismatch at any join. Motion artifacts (banding, shimmer during turns) don't show in a single frame — the user judges those in playback.

Encodes of 4K sources take several minutes: run ffmpeg with `run_in_background` plus `-nostats -progress <scratch>/x.log`, and stream `frame=` counts to the user with a Monitor loop. The AVIF muxer writes the file only at the end, so `total_size` stays 0 until it finishes. The Bash shell is zsh: pass optional argument groups as arrays (`out=(-f null /dev/null); ... "${out[@]}"`), since an unquoted string variable is not word-split.

## Screen recording recipe (OBS .mp4)

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

## ProRes 4444 recipe (transparent overlay export)

Composite on black, crop to the padded box, downscale to the target width:

```
ffmpeg -y -t 35 -i src.mov -an -filter_complex \
  "color=black:s=3840x2160:r=30[bg];[bg][0:v]overlay=shortest=1:format=auto,\
   crop=W:H:X:Y,scale=1920:1080:flags=lanczos+accurate_rnd+full_chroma_int,format=yuv444p" \
  -c:v libaom-av1 -crf 37 -b:v 0 -cpu-used 3 -row-mt 1 -tiles 2x2 -still-picture 0 \
  -color_primaries bt709 -color_trc bt709 -colorspace bt709 docs/out.avif
```

- Set the `color` source size and rate to the source's own. The source is lossless and clean: **no `hqdn3d`, no `unsharp`.** Lanczos downscaling of 4K already gives crisp edges, while sharpening makes rotating map labels shimmer and spends bits on fine detail.
- **Use CRF, not a bitrate target.** Moving 3D map views (rotating chase camera, turns) need many more bits than static stretches. Bitrate-targeted VBR (1- or 2-pass, the pass count makes no difference) starves the turns and shows horizontal banding on the distant map. CRF gives every frame the same quality.
- Stay 8-bit `yuv444p`. A 10-bit encode at the same size looked worse.
- Always encode straight from the `.mov`. Re-encoding from an earlier AVIF stacks a second lossy step and visibly adds artifacts.
- Size guide for the six-map 1920×1080 grid: CRF 22 ≈ 10.7 MB / 40 s, CRF 32 ≈ 4.3 MB / 40 s, CRF 35 ≈ 3.3 MB / 35 s, CRF 37 ≈ 2.9 MB / 35 s. Each CRF step saves only ~8 % on this content, so to meet a size budget, trim the clip (`-t`) before raising CRF past ~37. A 35 s encode at `-cpu-used 3` takes about 5 minutes.

## Animated AVIF quirk

An animated AVIF holds two video streams: `v:0` is a single still cover frame and `v:1` is the animation. Always decode with `-map 0:v:1` (or `[0:v:1]` in a filtergraph); otherwise ffmpeg reads the 1-frame cover and seeks fail silently.
