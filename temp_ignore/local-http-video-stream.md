# Revised Implementation Plan: Local HTTP Range Server + Native `<video>` Preview

## 1. Decision Summary

The local HTTP range server plus native `<video>` tag remains the correct primary architecture for OVRLEY's current use case.

The app needs local random-access playback of one imported video file at a time. The video is only a preview background for the transparent overlay editor. It is not edited, transformed, segmented, or exported through this preview path.

Therefore the target architecture is:

```txt
Imported local video file
  -> existing Rust/Tauri import + probe command
  -> local loopback HTTP server with byte-range support
  -> unique preview URL per import
  -> native React <video> element
  -> existing canvas overlay/editor logic
```

This plan intentionally avoids HLS, segmentation, transcoding, or duplicated media caches in the primary path.

## 2. Important Changes From the Previous Plan

The critique identifies several valid issues. The implementation plan is revised accordingly.

### 2.1 Server belongs in the Tauri app crate, not `ovrley_core`

The local HTTP server should live in the Tauri app crate:

```txt
src-tauri/src/video_server.rs
```

not in:

```txt
src-tauri/ovrley_core/src/video_server.rs
```

Reason: the server is application infrastructure. It needs to be initialized from Tauri lifecycle code, exposed through Tauri managed state, and coordinated with app commands. `ovrley_core` should remain a clean reusable library crate without Tauri lifecycle dependencies.

### 2.2 Use the existing video probe as the starting point

The codebase already has `video_probe.rs`. This plan does not treat video probing as greenfield work.

The initial implementation should reuse the existing probe and later enrich it with additional fields only if needed:

- codec name
- codec profile
- pixel format
- bit depth
- chroma format if available
- audio presence
- rotation metadata
- container format
- keyframe/GOP estimate, optional later

### 2.3 Collapse the previous nine-phase plan to three phases

The revised plan uses three independently testable phases:

```txt
Phase A — Core Rust server infrastructure
Phase B — Frontend switchover
Phase C — Diagnostics, probing enrichment, and health warnings
```

This avoids over-engineering and makes it easier to compare the new preview path against the existing `convertFileSrc()` path before switching the default.

### 2.4 Change `<video preload>` early

The frontend should change:

```jsx
preload = "auto";
```

to:

```jsx
preload = "metadata";
```

as part of Phase B, not as late polish.

For 10-60 minute high-bitrate files, `metadata` is the safer default. The browser can still request ranges as needed for playback and seeking, but it is less likely to aggressively buffer from the beginning of a huge file.

### 2.5 Use `404` for unavailable preview files

Do not distinguish stale import IDs, cleared imports, missing current file, and unknown IDs with separate `410 Gone` logic.

For this local single-client app, this is sufficient:

```txt
404 Not Found
```

Use the frontend media error handler to show a user-facing message.

### 2.6 Multipart range handling can be pragmatic

Browsers normally use single byte ranges for `<video>` playback.

If a multipart range request appears, the server can either:

1. serve the first valid range as `206 Partial Content`, or
2. ignore the `Range` header and return `200 OK` with the full file streamed from disk.

Do not implement full `multipart/byteranges` support in the first version.

Recommended first implementation:

```txt
If Range contains comma-separated ranges:
  parse and serve the first valid range only
```

This keeps playback robust without adding unnecessary multipart response complexity.

### 2.7 Metadata timeout should not be too aggressive

Do not fail metadata loading after only 15 seconds for large files on slow disks.

Use softer thresholds:

```txt
8-10 seconds: show non-blocking "Loading video metadata..." message
30-45 seconds: show stronger warning and allow fallback/manual retry
```

The first import of a 50 GB file with the `moov` box at the end can be slow on HDDs.

## 3. Target Architecture

### 3.1 Backend components

```txt
src-tauri/src/lib.rs
src-tauri/src/video_server.rs
src-tauri/src/commands/... or existing command module
src-tauri/ovrley_core/src/video_probe.rs
```

Responsibilities:

| Area                            | Responsibility                                                             |
| ------------------------------- | -------------------------------------------------------------------------- |
| `src-tauri/src/video_server.rs` | Own local HTTP server, shared state, range parsing, streaming responses    |
| `src-tauri/src/lib.rs`          | Initialize server in `.setup()`, register managed state, register commands |
| Tauri commands                  | Import preview video, clear preview video, get preview URL/state           |
| Existing `video_probe.rs`       | Probe imported video metadata, later enrich fields if needed               |
| React frontend                  | Request preview URL and use it as `<video src>`                            |

### 3.2 Frontend components

Expected integration points:

```txt
app/src/hooks/useVideoImport.js
app/src/hooks/useVideoPreview.js
app/src/components/overlay-editor/OverlayCanvas.jsx
app/src/store/slices/createVideoImportSlice.js
app/src/api/backend.js
```

Exact file names may differ, but the important integration points are:

- import flow
- preview URL state
- video element rendering
- playback/seek synchronization
- error/warning UI

## 4. Backend State Model

### 4.1 Shared state requirements

The same video state must be visible to:

1. Tauri command handlers, which update the current imported video.
2. The HTTP server thread, which serves the current video file.

Use shared state:

```rust
Arc<Mutex<VideoServerInner>>
```

The Tauri-managed handle owns this shared state and the server thread receives a clone.

### 4.2 Recommended structs

```rust
// src-tauri/src/video_server.rs

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

pub struct VideoServerHandle {
    inner: Arc<Mutex<VideoServerInner>>,
}

struct VideoServerInner {
    port: Option<u16>,
    current: Option<CurrentVideo>,
    shutdown: bool,
}

#[derive(Clone)]
struct CurrentVideo {
    import_id: String,
    path: PathBuf,
    file_size: u64,
    content_type: String,
}
```

Optional additions:

```rust
created_at_ms: u64,
original_display_name: String,
probe_summary: Option<VideoProbeSummary>,
```

Keep the HTTP-serving state small. Do not store large buffers or decoded metadata in this shared state.

### 4.3 State ownership pattern

```txt
VideoServerHandle
  owns Arc<Mutex<VideoServerInner>>
  is registered with Tauri .manage(...)
  is used by Tauri commands
  is cloned into server thread
```

The command handler updates `current`. The server thread reads `current` on each request.

The server should clone the `CurrentVideo` out of the mutex quickly, then release the lock before opening and streaming the file.

Correct pattern:

```rust
let current = {
    let guard = inner.lock().map_err(...)?;
    guard.current.clone()
};

// Mutex is released here.
// Now open the file and stream bytes without holding the lock.
```

Do not hold the mutex while reading from disk or writing the HTTP response.

## 5. Phase A — Core Rust Server Infrastructure

### Goal

Add the local HTTP range server behind the backend API while preserving the existing preview path until the frontend is switched over.

### Estimated scope

```txt
~200-350 lines Rust
```

### Deliverables

- `tiny_http` dependency added to the Tauri app crate.
- New `src-tauri/src/video_server.rs` module.
- Server starts on `127.0.0.1:<random_port>`.
- Server is initialized from Tauri `.setup()`.
- Server handle is registered as Tauri managed state.
- Server serves only the currently imported video.
- Server uses unique import IDs in URLs.
- Server supports practical HTTP byte ranges.
- Tauri commands are added for import, clear, and URL retrieval.

### 5.1 Add dependency

Add to the Tauri app crate, not necessarily to `ovrley_core`:

```toml
# src-tauri/Cargo.toml
[dependencies]
tiny_http = "0.12"
uuid = { version = "1", features = ["v4"] }
```

If `uuid` already exists, reuse the existing dependency.

### 5.2 Create `video_server.rs`

Location:

```txt
src-tauri/src/video_server.rs
```

Core responsibilities:

```rust
impl VideoServerHandle {
    pub fn new() -> Self;
    pub fn start(&self) -> Result<(), String>;
    pub fn set_video(&self, path: PathBuf, content_type: String) -> Result<String, String>;
    pub fn clear_video(&self) -> Result<(), String>;
    pub fn current_url(&self) -> Option<String>;
    pub fn current_state(&self) -> Option<PreviewVideoState>;
}
```

`set_video` should:

1. validate the file exists
2. read file size from metadata
3. generate a new `import_id`
4. store `CurrentVideo`
5. return the preview URL

Example URL:

```txt
http://127.0.0.1:<port>/video/<import_id>
```

### 5.3 Start server in Tauri setup

In `src-tauri/src/lib.rs`:

```rust
mod video_server;

pub fn run() {
    let video_server = video_server::VideoServerHandle::new();

    tauri::Builder::default()
        .manage(video_server.clone())
        .setup(|app| {
            let server = app.state::<video_server::VideoServerHandle>();
            server.start()?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            backend_import_preview_video,
            backend_clear_preview_video,
            backend_get_preview_url,
            backend_get_video_state,
            // existing commands...
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

Adjust the exact error conversion to match the existing project style.

### 5.4 Add Tauri commands

Recommended commands:

```rust
#[tauri::command]
async fn backend_import_preview_video(
    state: tauri::State<'_, VideoServerHandle>,
    path: String,
) -> Result<ImportPreviewVideoResponse, String>;

#[tauri::command]
async fn backend_clear_preview_video(
    state: tauri::State<'_, VideoServerHandle>,
) -> Result<(), String>;

#[tauri::command]
async fn backend_get_preview_url(
    state: tauri::State<'_, VideoServerHandle>,
) -> Result<Option<String>, String>;

#[tauri::command]
async fn backend_get_video_state(
    state: tauri::State<'_, VideoServerHandle>,
) -> Result<Option<PreviewVideoState>, String>;
```

Recommended response shape:

```rust
#[derive(serde::Serialize)]
struct ImportPreviewVideoResponse {
    import_id: String,
    preview_url: String,
    metadata: ExistingVideoProbeResponse,
    warnings: Vec<String>,
}
```

`metadata` should initially reuse the existing probe response.

### 5.5 Import command flow

`backend_import_preview_video(path)` should:

```txt
1. Validate path exists and is a file.
2. Run existing video probe.
3. Determine content type from extension/container.
4. Register file with VideoServerHandle::set_video(...).
5. Return import_id, preview_url, metadata, warnings.
```

Recommended content type mapping:

```txt
.mp4  -> video/mp4
.mov  -> video/quicktime
.m4v  -> video/mp4
.webm -> video/webm
fallback -> application/octet-stream
```

For expected GoPro/Insta360/DJI/Premiere/DaVinci files, `.mp4` and `.mov` are the important paths.

### 5.6 HTTP route behavior

Supported route:

```txt
GET /video/<import_id>
```

Optional route for quick health checks:

```txt
GET /health
```

Do not serve arbitrary filesystem paths.

### 5.7 Request validation

For `GET /video/<import_id>`:

```txt
If no current video -> 404
If import_id does not match current import -> 404
If source file no longer exists -> 404
If method is not GET/HEAD -> 405
If Range is valid -> 206
If Range is absent -> 200
If Range is unsatisfiable -> 416
```

`HEAD` support is useful but not mandatory. If implemented, return the same headers as `GET` without a body.

### 5.8 Required response headers

For full response:

```txt
HTTP/1.1 200 OK
Content-Type: video/mp4
Content-Length: <file_size>
Accept-Ranges: bytes
Cache-Control: no-store
```

For range response:

```txt
HTTP/1.1 206 Partial Content
Content-Type: video/mp4
Content-Length: <range_length>
Content-Range: bytes <start>-<end>/<file_size>
Accept-Ranges: bytes
Cache-Control: no-store
```

For unsatisfiable range:

```txt
HTTP/1.1 416 Range Not Satisfiable
Content-Range: bytes */<file_size>
Accept-Ranges: bytes
Cache-Control: no-store
```

### 5.9 Range parsing requirements

Support these forms:

```txt
Range: bytes=0-
Range: bytes=123456-
Range: bytes=123456-789999
Range: bytes=-500000
```

Rules:

```txt
bytes=0-                  -> start 0, end file_size - 1
bytes=123456-             -> start 123456, end file_size - 1
bytes=123456-789999       -> start 123456, end min(789999, file_size - 1)
bytes=-500000             -> last 500000 bytes of file
```

Invalid or unsatisfiable examples:

```txt
bytes=999999999999-       -> 416 if start >= file_size
bytes=500-400             -> 416
bytes=-0                  -> 416 or ignore Range and return 200
```

Multipart first version:

```txt
bytes=0-99,200-299
```

Recommended handling:

```txt
Use the first valid range only: bytes=0-99
```

Do not implement full multipart body formatting in this phase.

### 5.10 Stream from disk in chunks

Do not use `read_to_end()`.

Use:

```txt
File::open
seek to start
read fixed-size chunks
write chunks to response body
stop after requested byte count
```

Recommended chunk size:

```txt
256 KiB - 1 MiB
```

The server must not hold the shared state mutex while streaming the file.

### 5.11 Server shutdown

The first implementation can use app-lifetime server behavior:

```txt
Server starts once.
Server runs until app exits.
Current video can be replaced or cleared.
```

A clean shutdown signal is preferred but not essential for MVP if the process exits normally.

If implemented, store a shutdown flag in `VideoServerInner` and have the server loop periodically check it using a non-blocking or timeout-based receive pattern.

### Phase A manual tests

#### A1. Server starts

- Launch app.
- Confirm no crash during `.setup()`.
- Confirm server binds to `127.0.0.1` on a random port.
- Confirm repeated launches do not fail due to port collision.

#### A2. Import registers video

- Import a small MP4.
- Confirm `backend_import_preview_video` returns:
  - `import_id`
  - `preview_url`
  - metadata from existing probe
- Confirm preview URL has unique import ID.

#### A3. URL uniqueness

- Import file A.
- Note URL A.
- Import file B.
- Note URL B.
- Confirm URL A !== URL B.

#### A4. Curl full response

Run:

```bash
curl -I "http://127.0.0.1:<port>/video/<import_id>"
```

Expected:

```txt
200 OK or 206 depending on method/server behavior
Accept-Ranges: bytes
Content-Type: video/mp4 or video/quicktime
Content-Length: correct file size
```

#### A5. Curl range response

Run:

```bash
curl -v -H "Range: bytes=0-999" "http://127.0.0.1:<port>/video/<import_id>" -o /tmp/range.bin
```

Expected:

```txt
206 Partial Content
Content-Range: bytes 0-999/<file_size>
Content-Length: 1000
```

`range.bin` should be exactly 1000 bytes.

#### A6. Curl suffix range

Run:

```bash
curl -v -H "Range: bytes=-500000" "http://127.0.0.1:<port>/video/<import_id>" -o /tmp/tail.bin
```

Expected:

```txt
206 Partial Content
Content-Length: 500000
```

unless file is smaller than 500000 bytes, in which case the full file length is expected.

#### A7. Unsatisfiable range

Run:

```bash
curl -v -H "Range: bytes=999999999999999-" "http://127.0.0.1:<port>/video/<import_id>"
```

Expected:

```txt
416 Range Not Satisfiable
Content-Range: bytes */<file_size>
```

#### A8. Stale import ID

- Import file A.
- Save URL A.
- Import file B.
- Request URL A.

Expected:

```txt
404 Not Found
```

#### A9. Cleared video

- Import a file.
- Clear preview video.
- Request previous URL.

Expected:

```txt
404 Not Found
```

#### A10. Deleted source file

- Import a temporary test video.
- Delete or rename the file externally.
- Request the preview URL.

Expected:

```txt
404 Not Found
```

## 6. Phase B — Frontend Switchover

### Goal

Use the local HTTP preview URL in the existing native `<video>` element while keeping the existing playback and overlay synchronization logic as unchanged as possible.

### Estimated scope

```txt
~50-120 lines JS/TS/JSX
```

### Deliverables

- Frontend import flow calls `backend_import_preview_video`.
- Store holds `importId` and `previewUrl`.
- `<video>` uses `previewUrl` instead of `convertFileSrc(path)`.
- `<video>` has `key={importId}` to force media element reset per import.
- `<video preload>` is changed to `metadata`.
- Existing play/pause/currentTime synchronization still works.
- Existing export path remains unchanged.
- Optional feature flag keeps old `convertFileSrc()` path available during testing.

### 6.1 Backend API wrapper

Add wrapper functions in the existing backend API module:

```js
export async function importPreviewVideo(path) {
  return invoke("backend_import_preview_video", { path });
}

export async function clearPreviewVideo() {
  return invoke("backend_clear_preview_video");
}

export async function getPreviewUrl() {
  return invoke("backend_get_preview_url");
}

export async function getVideoState() {
  return invoke("backend_get_video_state");
}
```

Adjust argument casing to match the project’s existing Tauri invoke conventions.

### 6.2 Update import flow

Where the current flow does something like:

```txt
select path
-> extractVideoMetadata(path)
-> store importedVideoPath
-> use convertFileSrc(importedVideoPath)
```

change to:

```txt
select path
-> backend_import_preview_video(path)
-> store metadata, importId, previewUrl, originalPath
-> video element uses previewUrl
```

The original path should remain stored for export/backend processing if already needed elsewhere.

### 6.3 Store shape

Recommended state additions:

```ts
type VideoImportState = {
  importedVideoPath: string | null;
  previewUrl: string | null;
  importId: string | null;
  metadata: VideoMetadata | null;
  previewWarnings: string[];
  previewError: string | null;
};
```

Do not remove the original path if export or sync code depends on it.

### 6.4 Update `<video>` element

Before:

```jsx
<video ref={videoRef} src={convertFileSrc(importedVideoPath)} preload="auto" muted playsInline />
```

After:

```jsx
<video key={importId ?? "no-video"} ref={videoRef} src={previewUrl ?? undefined} preload="metadata" muted playsInline />
```

`key={importId}` is important. It forces React to create a fresh media element for each imported file and avoids stale browser/WebView decoder state.

### 6.5 Preserve existing player logic

The following should continue to work through the native video API:

```txt
videoRef.current.play()
videoRef.current.pause()
videoRef.current.currentTime = t
videoRef.current.duration
loadedmetadata
seeked
seeking
timeupdate
error
```

No HLS.js-specific event layer is needed.

### 6.6 Optional feature flag during rollout

Add a temporary app setting or dev constant:

```ts
const USE_LOCAL_HTTP_VIDEO_PREVIEW = true;
```

Comparison paths:

```txt
true  -> previewUrl from local HTTP server
false -> convertFileSrc(importedVideoPath)
```

Remove this once real-file testing is complete.

### Phase B manual tests

#### B1. Small MP4 playback

- Import a small MP4.
- Confirm preview appears as canvas background.
- Confirm metadata loads.
- Confirm play/pause works.

#### B2. Seeking

- Import a 4K MP4.
- Seek to 10%, 50%, 90% of duration.
- Confirm video seeks without full-file loading.
- Confirm overlay/editor time sync remains correct.

#### B3. Re-import lifecycle

- Import video A.
- Play it.
- Import video B.
- Confirm video B appears.
- Confirm video A does not flash or remain cached.
- Confirm `key={importId}` causes clean reset.

#### B4. Clear lifecycle

- Import video.
- Clear video.
- Confirm video disappears.
- Confirm old preview URL returns 404.
- Confirm no React error loop occurs.

#### B5. Large file smoke test

- Import a large action-cam file.
- Confirm UI remains responsive.
- Confirm app memory does not grow anywhere near file size.
- Confirm playback starts after metadata is loaded.

#### B6. Export unaffected

- Import video.
- Preview using new local HTTP URL.
- Export using existing backend path.
- Confirm export still reads from original file path and is not dependent on HTTP server.

#### B7. Fallback path comparison

If feature flag exists:

- Test same file with local HTTP preview.
- Test same file with old `convertFileSrc()` path.
- Compare metadata load time and seek responsiveness.

## 7. Phase C — Probe Enrichment, Health Detection, and Warnings

### Goal

Add diagnostics and user-visible warnings without blocking the default path.

This phase should not prevent playback unless playback actually fails.

### Estimated scope

```txt
~80-160 lines Rust
~80-180 lines JS/TS/JSX
```

### Deliverables

- Existing video probe is enriched, not replaced.
- Frontend tracks basic playback health.
- Slow metadata loading shows a non-blocking warning.
- Seek latency can be measured.
- Native media errors are mapped to useful messages.
- UI can recommend proxy fallback in the future.

### 7.1 Enrich existing `video_probe.rs`

Add fields if available:

```rust
codec_name: Option<String>,
codec_long_name: Option<String>,
codec_profile: Option<String>,
pix_fmt: Option<String>,
bits_per_raw_sample: Option<u32>,
has_audio: bool,
container_format: Option<String>,
rotation_degrees: Option<i32>,
```

Do not block import if some fields are missing.

### 7.2 Non-blocking browser-decodability warnings

Use conservative warnings such as:

```txt
HEVC/H.265 playback depends on the OS/WebView codec stack.
10-bit or 4:2:2 footage may not play reliably in the native video element.
Very high-bitrate exports may decode slowly.
```

Warnings should not prevent trying playback.

### 7.3 Metadata loading health

Frontend logic:

```txt
on src set:
  start metadata timer

on loadedmetadata:
  clear timer

after 8-10s:
  show "Loading video metadata..." soft message

after 30-45s:
  show stronger warning:
  "This file is taking unusually long to load. It may be on a slow drive or use metadata stored at the end of the file."
```

Do not auto-fail too early.

### 7.4 Seek latency measurement

When the user seeks:

```txt
record seek start timestamp on seeking
record seek complete timestamp on seeked
latency = seeked - seeking
```

Use this only for diagnostics/warnings initially.

Possible warning:

```txt
Seeking is slow for this file. A preview proxy may improve responsiveness.
```

### 7.5 Native video error mapping

Map `HTMLMediaElement.error.code`:

```txt
1 MEDIA_ERR_ABORTED
2 MEDIA_ERR_NETWORK
3 MEDIA_ERR_DECODE
4 MEDIA_ERR_SRC_NOT_SUPPORTED
```

Suggested messages:

```txt
MEDIA_ERR_DECODE:
  "The video could not be decoded by the system video player. This may happen with some HEVC, 10-bit, or 4:2:2 files."

MEDIA_ERR_SRC_NOT_SUPPORTED:
  "This video format is not supported by the system video player."

MEDIA_ERR_NETWORK:
  "The local preview server could not read the video file. The file may have been moved, deleted, or become unavailable."
```

### 7.6 Backend diagnostics command

`backend_get_video_state()` should return:

```ts
{
  importId: string;
  previewUrl: string;
  pathExists: boolean;
  fileSize: number;
  contentType: string;
}
```

Do not expose filesystem paths to the frontend unless the app already stores and displays them elsewhere. If the path is already part of the import state, keep behavior consistent with the current app.

### Phase C manual tests

#### C1. Metadata warning

- Use a large file on a slower drive if available.
- Confirm soft loading message appears after threshold.
- Confirm it disappears on `loadedmetadata`.

#### C2. Unsupported/awkward codec

- Test HEVC file.
- Test 10-bit HEVC file if available.
- Test 4:2:2 export if available.
- Confirm app tries playback.
- Confirm useful warning or error if playback fails.

#### C3. Seek latency diagnostics

- Seek repeatedly across a long file.
- Confirm seek latency is measured.
- Confirm no warning appears for normal seek latency.
- Confirm warning appears only for repeated slow seeks.

#### C4. Deleted file during playback

- Import video.
- Delete/rename source file externally.
- Seek or reload video.
- Confirm local server returns 404.
- Confirm frontend shows useful message rather than crashing.

#### C5. Rapid re-imports

- Import several videos quickly.
- Confirm only latest `importId` is valid.
- Confirm stale URLs return 404.
- Confirm no mixed metadata or stale duration appears.

## 8. Security and Safety Requirements

### 8.1 Bind only to loopback

The server must bind only to:

```txt
127.0.0.1
```

Do not bind to:

```txt
0.0.0.0
```

### 8.2 Do not expose arbitrary file paths

Only serve the current registered video file.

Do not implement routes such as:

```txt
/file?path=C:\...
```

or:

```txt
/video/C:/Users/...
```

The route should only contain an opaque import ID:

```txt
/video/<import_id>
```

### 8.3 Do not keep old imports accessible

Because only one video can be imported at a time, old import IDs should become invalid immediately after re-import.

Expected behavior:

```txt
current import ID -> serves video
old import ID     -> 404
```

### 8.4 Cache behavior

Use:

```txt
Cache-Control: no-store
```

The unique import URL already prevents most stale metadata problems. `no-store` adds another defensive layer.

## 9. Performance Expectations

### Expected benefits

Compared with `convertFileSrc()`, the local HTTP range server should improve behavior when the WebView/video engine needs byte-range access to large files, especially when metadata or seek tables are located near the end of the file.

Expected properties:

```txt
No transcoding
No media duplication
No HLS cache
No full-file memory load
Native hardware decoding where available
Existing <video> control model preserved
```

### Expected memory profile

Approximate:

```txt
HTTP server overhead: low single-digit MB to ~10 MB
Compressed video buffer: controlled by browser/WebView
Decoded frames: GPU/system memory, depends on resolution and decoder
File itself: not loaded into RAM
```

### Expected disk usage

```txt
Additional media disk usage: 0 GB
```

Only tiny metadata/state is added.

## 10. Known Limitations

This architecture does not guarantee playback for every imported file.

Possible problematic files:

```txt
HEVC/H.265 unsupported by OS/WebView
10-bit HEVC
4:2:2 exports
unusual MOV variants
very high-bitrate intermediate files
corrupt files
files with very sparse keyframes
files on very slow external drives
```

These should eventually be handled by a preview proxy fallback, not by making HLS the primary path.

## 11. Future Fallback Path: Preview Proxy, Not HLS by Default

If native local HTTP playback fails or performs poorly, the next fallback should be low-resolution proxy generation:

```txt
Original source video
  -> ffmpeg proxy generation
  -> 720p or 1080p H.264 faststart MP4
  -> native <video>
```

This solves the actual hard cases:

- unsupported codec
- excessive bitrate
- 10-bit/4:2:2 decode problems
- poor playback performance
- slow seek behavior

HLS should remain a later specialized option only if proxy MP4 does not satisfy a clearly identified requirement.

## 12. Updated Code Length Estimate

### Minimal usable implementation

```txt
Rust:       250-400 LOC
Frontend:    50-120 LOC
Total:      300-520 LOC
```

### Production-ready implementation with diagnostics

```txt
Rust:       400-650 LOC
Frontend:   150-300 LOC
Total:      550-950 LOC
```

The main code cost is not the HTTP server itself. The real complexity is:

```txt
correct Range handling
shared Tauri/server state
import lifecycle
cache-busting import IDs
frontend media lifecycle reset
diagnostic warnings
robust error handling
```

## 13. Other implementation details:

- 1. tiny_http server loop needs to run on a dedicated std::thread
     The plan shows server.start() in Tauri's .setup() hook but doesn't show the thread spawn. .setup() runs on the main thread and must return quickly. The actual loop needs:

```rust
pub fn start(&self) -> Result<(), String> {
    let server = tiny_http::Server::http("127.0.0.1:0")?;
    let port = server.server_addr().to_ip()?.port();
    let inner = self.inner.clone();
    // Update port in shared state
    self.inner.lock().unwrap().port = Some(port);
    std::thread::spawn(move || {
        loop {
            match server.recv_timeout(Duration::from_millis(200)) {
                Ok(Some(req)) => handle_request(&inner, req),
                Ok(None) => {}
                Err(_) => break,
            }
            if inner.lock().unwrap().shutdown {
                break;
            }
        }
    });
    Ok(())
}
```

- 2. tiny_http response streaming. This avoids buffering the range. The plan should specify this approach.

```rust
tiny_http::Response::new() accepts Box<dyn Read + Send>, so streaming a partial range is:
let file = std::fs::File::open(&path)?;
file.seek(SeekFrom::Start(range_start))?;
let limited = file.take(range_length);
let response = Response::new(206, headers, Box::new(limited), Some(range_length), None);
```

- 3. One minor version risk
     tiny_http 0.12 may not exist yet on crates.io. If it doesn't, 0.11 is the fallback. The API is essentially the same.
- 4. backend_get_preview_url is redundant
     The import command (backend_import_preview_video) already returns the URL. The separate getter adds complexity without a clear use case — the frontend stores it in Zustand state. If the frontend loses that state, the server state is also lost (the server doesn't persist across restarts). Consider dropping this command unless there's a specific need, or defer it to Phase C as a diagnostic.

## 14. Final Revised Phase Checklist

### Phase A — Core infrastructure

- [ ] Add `tiny_http` to Tauri app crate.
- [ ] Add `uuid` if needed.
- [ ] Create `src-tauri/src/video_server.rs`.
- [ ] Define `VideoServerHandle`, `VideoServerInner`, `CurrentVideo`.
- [ ] Start server in Tauri `.setup()`.
- [ ] Register `VideoServerHandle` as managed state.
- [ ] Implement `GET /video/<import_id>`.
- [ ] Implement practical Range support.
- [ ] Stream from disk in chunks.
- [ ] Add import, clear, get URL, get state commands.
- [ ] Reuse existing `video_probe.rs` in import command.
- [ ] Test with `curl`.

### Phase B — Frontend switchover

- [ ] Add frontend backend API wrappers.
- [ ] Update import flow to call `backend_import_preview_video`.
- [ ] Store `importId` and `previewUrl`.
- [ ] Use `previewUrl` as video `src`.
- [ ] Add `key={importId}` to `<video>`.
- [ ] Change `preload="auto"` to `preload="metadata"`.
- [ ] Preserve existing playback/sync logic.
- [ ] Confirm export still uses original source path.
- [ ] Optionally retain feature flag for comparison.

### Phase C — Diagnostics and warnings

- [ ] Enrich existing probe with codec/pixel format fields.
- [ ] Add metadata load warning timer.
- [ ] Add seek latency measurement.
- [ ] Map native media errors to user-friendly messages.
- [ ] Add diagnostics command/state.
- [ ] Prepare UI path for future preview proxy fallback.
