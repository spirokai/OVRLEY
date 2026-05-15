# Create a detailed plan to implment the follow feature in this project

## 1. Feature Description

User will be able to import a .mp4 file and then render and composite the overlay on top of the video.
This feature is intended to be used for creating youtube videos of race telemetry. This will function will exist in ADDITION to the current transparent video export feature.

[IMPORTANT!] The app must continue to produce transparent video exports in full scope as now. The code responsible for transparent export is sacred and must not be touched. The code produced by this plan should be placed in separate files - do not modify existing files unless absolutely necessary - always prompt for permission with explanation.

## 2. Current Limitation

Cyclemetry currently exports each overlay as a separate transparent video.
The user then has to take these videos and manually composite them on top of their base video using a video editor.
This is a time consuming and tedious process.

## 3. Feature Details

- A button to import the mp4 file in the AppHeader
- Once loaded, the video will replace the black/white/checkerboard canvas and serve as the background
- We need a script that will try to reconstruct what the real time was at the start of the uploaded clip. The video might be a direct export from an action cam (GoPro, DJI, Insta360, etc.) or it could be a trimmed and processed export from a video editing software such as Premiere or DaVinci. Create a plan to implement robust time detection strategy for all these cases.
- The app will then try to immediately sync the video with the widget preview timeline using timestamps of video creation and timestamps from .fit/.gpx parsed activity
- The sync will not always be perfect, so we will need a way to adjust the sync in the SidebarSettingsTab under new "sync" section. There should be a simple input field that allows setting offset in seconds or as time e.g. 4:53.
- If time of creation of the video is outside the activity range, or video creation date cannot be determined, there must be a warning displayed in the "sync" section.
- Overlay player will (on top of the widget preview) also play the imported video.
- Overlay player slider will highlight the section where video clip for the track exists. It should display the total duration of the video.
- If imported video exists, the custom range in SidebarSettingsTab should not be visible - it assumes the user will export only the relevant section of the video.
- When user clicks Render button, the codec dropdown should contain two groups of codecs: transparent and mp4. Transparent will be disabled,with a small excalamation mark saying 'video imported', mp4 will be disabled if no video has been imported, with a small exclamation mark saying 'video required' The codec options include h264/h265/hevc/h264_qsv/hevc_qsv/h264_vaapi also with gpu-accelerated (nvgpu, nnvgpu) and videotoolbox options - the tool must remain compatible with windows, intel, linux, macOS, and it should work on both nvidia and intel/amd GPUs. The available codecs should be detected based on the hardware - e.g. do not allow intel-specific codecs for macOS, or gpu acceleration if no dedicated GPU exists. Consult the available codecs in the installed ffmpeg .
- The render modal will also contain a slider with bitrate setting 20-100Mbps if a video import exists. The slider should have a recommended default value. I am not sure how to handle this since custom resolutions also exists, but as a guideline the recommendation should be: 60Mbps for H264, and 40Mbps for H265 for 4K video, and 10Mbps for H264, and 8Mbps for H265 for 1080p video; both at 30fps. At 60 fps, the value should be 50% higher. The recommended values should not be exactly calculated, more like there should be some binning based on resolution and fps, and user can override them.
- The compositing will be done using ffmpeg. Current encoding is happening in encode/ffmpeg.rs and video.rs. The frames are rendered by skia-rust and then passed into the encoder. The current pipeline must be COMPLETELY preserved for transparent overlays. Parallel pipeline must be built for video compositing on top of the mp4. Consult what approach to use for compositing - both quality and performance are crucial here. I assume we should use filter_complex but please consult ffmpeg documentation to find the best way to do it.
- Ensure this encoding still produces debug jsons with timings equivalent to those found in target/debug_render/phase_6 with individual steps and totals.

- Here are some builtin profiles used by gopro-dashboard-overlay and they should be a good starting point:

```json
builtin_profiles = {
"nvgpu": {
"input": ["-hwaccel", "nvdec"],
"output": ["-vcodec", "h264_nvenc", "-rc:v", "cbr", "-b:v", "25M", "-bf:v", "3", "-profile:v", "high", "-spatial-aq", "true", "-movflags", "faststart"]
},
"nnvgpu": {
"input": ["-hwaccel", "cuda", "-hwaccel_output_format", "cuda"],
"filter": "[0:v]scale_cuda=format=yuv420p[mp4_stream];[1:v]format=yuva420p,hwupload[overlay_stream];[mp4_stream][overlay_stream]overlay_cuda",
"output": ["-vcodec", "h264_nvenc", "-rc:v", "cbr", "-b:v", "25M", "-bf:v", "3", "-profile:v", "main", "-spatial-aq", "true", "-movflags", "faststart"]
},
"mac_hevc": {
"input": ["-hwaccel", "videotoolbox"],
"output": ["-vcodec", "hevc_videotoolbox", "-q:v", "60"]
},
"mac": {
"input": ["-hwaccel", "videotoolbox"],
"output": ["-vcodec", "h264_videotoolbox", "-q:v", "60"]
},
"qsv": {
"input": ["-init_hw_device", "qsv=hw", "-hwaccel", "qsv", "-hwaccel_output_format", "qsv"],
"filter": "[0:v]hwupload=extra_hw_frames=64[main_hw];[1:v]hwupload=extra_hw_frames=64,format=qsv[overlay_hw];[main_hw][overlay_hw]overlay_qsv=x=0:y=0,hwdownload,format=nv12",
"output": ["-vcodec", "hevc_qsv", "-global_quality", "25", "-c:a", "copy"]
}
}
```

## 4. Contingency plan

In case of any ambiguities, ask clarifying questions. This is strictly a planning phase and we need to get the details right before proceeding to implementation.

## 5. Scope of the plan

Create detailed plan in phases that can be tested independently. Outline the deliverables, and which manual tests must be performed.
