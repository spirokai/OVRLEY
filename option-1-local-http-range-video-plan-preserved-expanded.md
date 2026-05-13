# Option 1 Implementation Plan: Local HTTP Range Server for Scalable Video Preview

## 1. Goal

Replace the current Tauri `convertFileSrc()`-based video preview path with a local Rust HTTP server that serves the imported source video through proper HTTP byte-range requests.

The video remains an unmodified background preview for the transparent overlay editor. No video editing, transcoding, segmentation, HLS generation, or duplicated media cache should be introduced in this phase.

The primary objective is to make preview playback and seeking more scalable for large local action-camera files, including 4K/60 footage ranging from 10-20 minutes routinely and up to 60 minutes or approximately 50 GB in extreme cases.

## 2. Scope

### In scope

- Start a local loopback-only HTTP server from the Rust/Tauri backend.
- Serve exactly one imported video file at a time.
- Serve the original file directly from disk.
- Implement correct HTTP `Range` request support.
- Generate a unique video URL per import to avoid stale browser/WebView metadata or cache reuse.
- Keep the existing React `<video>` element as the playback surface.
- Preserve the current overlay editor and player synchronization logic as much as possible.
- Add import probing and basic playback-health detection.
- Add fallback plumbing for future proxy generation, but do not implement proxy generation in this phase unless explicitly chosen later.
- Add defensive behavior for unsupported codecs, deleted files, invalid ranges, and rapid re-imports.

### Out of scope

- HLS.js integration.
- MPEG-TS or fMP4 segmentation.
- Background transcoding.
- Low-resolution proxy generation.
- Multi-video timelines.
- Video editing operations.
- Export changes.
- Serving files over the network.
- Remote streaming.

## 3. Core Architecture

Current path:

```txt
Imported local file
  -> Tauri convertFileSrc()
  -> React <video>
  -> Canvas/editor background
```

Target path:

```txt
Imported local file
  -> Rust import/probe command
  -> local HTTP range server on 127.0.0.1
  -> unique local URL per import
  -> React <video>
  -> Canvas/editor background
```

The exported final video should continue to use the original file path directly through the existing backend/FFmpeg pipeline. The local HTTP server is only for interactive preview playback in the editor.

## 4. Design Principles

1. Do not transform media unless absolutely necessary.
2. Use native browser/WebView decoding first.
3. Serve bytes efficiently; never load the full video into memory.
4. Treat browser-decodability as conditional, not guaranteed.
5. Make each import URL unique.
6. Keep frontend playback semantics based on the normal `<video>` API.
7. Keep fallback paths explicit, but do not prematurely implement HLS.
8. Prefer predictable failure states over silent broken playback.

## 5. Important Revisions to the Original Option 1

### 5.1 Do not reuse a single `/video` URL

The server must not always expose the imported video as only:

```txt
http://127.0.0.1:<port>/video
```

Instead, every import should create a unique import identifier:

```txt
http://127.0.0.1:<port>/video/<import_id>
```

or:

```txt
http://127.0.0.1:<port>/video?token=<import_id>
```

Recommended route:

```txt
GET /video/<import_id>
```

Reason: The video element, WebView, and browser cache may retain metadata, duration, seek ranges, or codec state if the URL stays identical across imports. A unique URL forces the frontend media element to treat each imported file as a new media resource.

### 5.2 Implement full practical Range support

The backend must correctly support the range forms normally used by browsers:

```txt
Range: bytes=0-
Range: bytes=123456-
Range: bytes=123456-789999
Range: bytes=-500000
```

The server must return:

```txt
206 Partial Content
Accept-Ranges: bytes
Content-Type: video/mp4 or detected content type
Content-Length: <range_length>
Content-Range: bytes <start>-<end>/<file_size>
```

For invalid or unsatisfiable ranges, return:

```txt
416 Range Not Satisfiable
Content-Range: bytes */<file_size>
```

### 5.3 Stream file chunks instead of buffering ranges

The backend must never allocate an entire requested range into memory.

Correct behavior:

```txt
open file
seek to byte start
read fixed-size chunks
stream chunks into response
stop after requested range length
```

Suggested internal chunk size:

```txt
64 KiB to 1 MiB
```

The exact size can be adjusted after profiling.

### 5.4 Add preflight probing

On import, the backend should probe the media file and return a compact metadata object to the frontend. This should not block the HTTP server design, but it should become part of the import lifecycle.

Probe fields:

- path
- file size
- container format
- duration
- width
- height
- nominal fps
- codec name
- codec profile
- pixel format
- bit depth if available
- audio streams present/absent
- rotation metadata if available
- estimated keyframe interval if available
- whether the file looks likely to be browser-playable
- warning flags

The probing step allows the UI to warn the user before playback fails, and it creates a clean handoff point for future proxy fallback.

### 5.5 Add playback-health detection

The frontend should monitor whether the native video path actually works.

Useful signals:

- `loadedmetadata`
- `canplay`
- `canplaythrough` if useful, but do not rely on it exclusively
- `error`
- `stalled`
- `waiting`
- `seeking`
- `seeked`
- seek latency
- metadata load timeout
- first-frame timeout
- repeated stalls

This phase does not need to solve all failures. It should reliably detect and surface them.

### 5.6 Fallback should be proxy-oriented later, not HLS-first

If native local HTTP playback fails, the future fallback should usually be a low-resolution preview proxy rather than HLS segmentation of the full source.

Recommended future fallback hierarchy:

```txt
Path A: Native local HTTP Range playback of original file
Path B: Low-resolution preview proxy MP4
Path C: Specialized fMP4/HLS path only if a specific future requirement appears
```

This implementation plan focuses on Path A while preparing the control flow for Path B.

## 6. Proposed File/Module Changes

Exact file names can be adjusted to match the current repository layout.

### Rust/Tauri backend

| File | Change |
|---|---|
| `src-tauri/Cargo.toml` | Add local HTTP dependency if needed. |
| `src-tauri/ovrley_core/Cargo.toml` | Add dependency if server lives in core crate. |
| `src-tauri/ovrley_core/src/video_server.rs` | New local HTTP range server. |
| `src-tauri/ovrley_core/src/video_import.rs` | Import state, import IDs, metadata objects. |
| `src-tauri/ovrley_core/src/video_probe.rs` | Probe source video metadata. |
| `src-tauri/ovrley_core/src/lib.rs` | Register modules. |
| `src-tauri/src/lib.rs` | Initialize server during app setup; manage server state. |
| `src-tauri/ovrley_core/src/commands/mod.rs` | Add commands for import, clear, URL retrieval, health/debug info. |

### React frontend

| File | Change |
|---|---|
| `app/src/api/backend.js` or equivalent | Add wrappers for video import/clear/get-preview-url. |
| `app/src/hooks/useVideoPreview.js` | Replace `convertFileSrc()` with backend preview URL. |
| `app/src/components/overlay-editor/OverlayCanvas.jsx` | Continue using `<video>`, but consume new source URL. |
| `app/src/store/slices/createVideoImportSlice.js` | Store import ID, preview URL, metadata, warnings, playback state. |
| Optional: `app/src/hooks/useVideoPlaybackHealth.js` | Centralize media error/stall/seek-latency detection. |

## 7. Dependency Choice

The original proposal used `tiny_http`. That can work, but the dependency should be selected based on the current backend runtime.

### Acceptable options

#### Option A: `tiny_http`

Pros:

- Small.
- Simple.
- No async runtime required.
- Good enough for one local video stream.

Cons:

- More manual response streaming work.
- Less ergonomic routing.

#### Option B: `axum` or `hyper`

Pros:

- More robust HTTP primitives.
- Easier streaming-body abstraction.
- Better long-term extensibility.

Cons:

- Larger dependency footprint.
- Requires async runtime integration.

#### Recommended decision

If the current Tauri backend does not already use an async web stack, use a small synchronous server. If the app already has Tokio/hyper-related dependencies, use `axum` or `hyper` directly.

The important requirement is not the crate. The important requirement is correct range behavior and streaming from disk.

## 8. Backend Data Model

### 8.1 Video import state

The server should maintain thread-safe state for the current import.

Conceptual structure:

```rust
struct VideoImportState {
    current: Option<CurrentVideoImport>,
}

struct CurrentVideoImport {
    import_id: String,
    path: PathBuf,
    file_size: u64,
    content_type: String,
    metadata: VideoProbeMetadata,
    created_at_ms: u128,
}
```

The `import_id` should be unique per import, even if the user imports the same file twice.

Suggested ID generation:

```txt
UUID v4
```

or:

```txt
timestamp + random suffix
```

### 8.2 Video metadata object

Frontend-facing shape:

```ts
interface VideoProbeMetadata {
  fileSizeBytes: number;
  container?: string;
  durationSeconds?: number;
  width?: number;
  height?: number;
  fps?: number;
  codecName?: string;
  codecProfile?: string;
  pixelFormat?: string;
  bitDepth?: number;
  hasAudio?: boolean;
  rotationDegrees?: number;
  estimatedKeyframeIntervalSeconds?: number;
  likelyBrowserPlayable: boolean;
  warnings: VideoImportWarning[];
}
```

Example warning values:

```ts
type VideoImportWarning =
  | 'unsupported_codec_possible'
  | 'hevc_support_platform_dependent'
  | 'ten_bit_video_possible_issue'
  | 'four_two_two_video_possible_issue'
  | 'very_high_bitrate'
  | 'sparse_keyframes_possible_slow_seek'
  | 'variable_frame_rate_possible_sync_issue'
  | 'rotation_metadata_present'
  | 'probe_incomplete';
```

## 9. Backend Commands

### 9.1 `backend_import_preview_video`

Purpose:

- Accept selected source path.
- Probe file.
- Register file as current server target.
- Generate unique import ID.
- Return preview URL and metadata.

Input:

```ts
{
  path: string
}
```

Output:

```ts
{
  importId: string;
  previewUrl: string;
  metadata: VideoProbeMetadata;
}
```

Behavior:

1. Validate that path exists.
2. Validate that path points to a file.
3. Open file once to confirm readable permissions.
4. Get file size.
5. Probe video metadata.
6. Determine content type.
7. Generate unique import ID.
8. Store import state.
9. Return unique preview URL.

Preview URL example:

```txt
http://127.0.0.1:49152/video/e5bcb78b-1aa3-4271-9e7b-b5ddda5a8977
```

### 9.2 `backend_clear_preview_video`

Purpose:

- Clear current imported preview video.

Input:

```ts
{}
```

Output:

```ts
{
  ok: true
}
```

Behavior:

1. Clear current import state.
2. Future requests to the previous import URL should return `404` or `410`.
3. Frontend should remove the video `src` and call `load()` on the video element.

### 9.3 `backend_get_preview_video_state`

Purpose:

- Debugging and diagnostics.

Output:

```ts
{
  hasVideo: boolean;
  importId?: string;
  path?: string;
  fileSizeBytes?: number;
  previewUrl?: string;
}
```

This is useful during development and should either be kept internal or only exposed through debug tooling.

## 10. HTTP Server Behavior

### 10.1 Bind address

The server must bind only to loopback:

```txt
127.0.0.1
```

Do not bind to:

```txt
0.0.0.0
```

Use a random available port:

```txt
bind 127.0.0.1:0
```

Store the assigned port in Tauri state.

### 10.2 Server lifetime

The server should start once during app startup and remain alive for the app lifetime.

The imported file path should change, not the server itself.

### 10.3 Routes

Required route:

```txt
GET /video/<import_id>
```

Optional debug routes:

```txt
GET /health
GET /debug/video-state
```

Do not expose filesystem paths in ordinary HTTP responses unless in development/debug mode.

### 10.4 Request validation

For `GET /video/<import_id>`:

1. Check that there is a current video.
2. Check that `<import_id>` matches the current import ID.
3. Check that the file still exists.
4. Check that the file is readable.
5. Read file size from metadata or refresh from filesystem.
6. Parse `Range` header if present.
7. Serve range or full response.

If no current video:

```txt
404 Not Found
```

If import ID is stale:

```txt
410 Gone
```

or:

```txt
404 Not Found
```

Prefer `410 Gone` during development because it makes stale URL problems easier to diagnose. Use `404` in production if you prefer not to reveal state.

## 11. HTTP Range Semantics

### 11.1 No Range header

If the browser requests the file without a Range header, there are two acceptable behaviors.

#### Recommended behavior

Return `200 OK` with headers and stream the file body:

```txt
200 OK
Accept-Ranges: bytes
Content-Type: video/mp4
Content-Length: <file_size>
```

The body must still be streamed from disk. Do not buffer the whole file.

#### Alternative behavior

Return `206 Partial Content` for a conservative initial chunk even without a Range header.

This is less standard and not recommended unless testing proves the WebView behaves better with it.

### 11.2 Range from start to EOF

Input:

```txt
Range: bytes=0-
```

Response:

```txt
206 Partial Content
Content-Range: bytes 0-<file_size - 1>/<file_size>
Content-Length: <file_size>
```

### 11.3 Range from offset to EOF

Input:

```txt
Range: bytes=123456-
```

Response:

```txt
206 Partial Content
Content-Range: bytes 123456-<file_size - 1>/<file_size>
Content-Length: <file_size - 123456>
```

### 11.4 Explicit start and end range

Input:

```txt
Range: bytes=123456-789999
```

Response:

```txt
206 Partial Content
Content-Range: bytes 123456-789999/<file_size>
Content-Length: 666544
```

### 11.5 Suffix range

Input:

```txt
Range: bytes=-500000
```

Response:

```txt
206 Partial Content
Content-Range: bytes <file_size - 500000>-<file_size - 1>/<file_size>
Content-Length: 500000
```

If suffix length is larger than the file size, serve the whole file as a range:

```txt
Content-Range: bytes 0-<file_size - 1>/<file_size>
```

### 11.6 Invalid range

Input examples:

```txt
Range: bytes=abc-def
Range: bytes=999999999999-
Range: bytes=500-100
```

Response:

```txt
416 Range Not Satisfiable
Content-Range: bytes */<file_size>
```

### 11.7 Multiple ranges

Browsers rarely need multipart byte ranges for video playback in this context.

If the request is:

```txt
Range: bytes=0-99,200-299
```

Initial implementation may reject it with:

```txt
416 Range Not Satisfiable
```

or respond with:

```txt
501 Not Implemented
```

Recommended: reject multipart ranges explicitly and log it. Add multipart support only if real WebView testing shows it is needed.

## 12. Content Type Detection

Minimum implementation:

```txt
.mp4  -> video/mp4
.mov  -> video/quicktime
.m4v  -> video/x-m4v or video/mp4
```

Because action cameras and editors often produce MP4/MOV files, this is sufficient for the first version.

Do not rely entirely on extension for playback compatibility. The probe metadata should remain the more important source for codec warnings.

## 13. Security Requirements

Although this is local-only, still treat it as a real HTTP server.

Requirements:

1. Bind only to `127.0.0.1`.
2. Serve only the currently imported file.
3. Do not expose arbitrary filesystem paths through route parameters.
4. Do not implement a route like `/file?path=...`.
5. Use opaque import IDs.
6. Reject stale import IDs.
7. Do not allow directory traversal.
8. Consider adding a per-app-session secret token if additional protection is desired.

Optional hardened URL:

```txt
http://127.0.0.1:<port>/video/<import_id>?session=<session_token>
```

This is probably not necessary for a local desktop app, but it is simple to add if desired.

## 14. Frontend Integration

### 14.1 Replace `convertFileSrc()` usage

Current conceptual logic:

```js
const videoSrc = importedVideoPath ? convertFileSrc(importedVideoPath) : null;
```

Target conceptual logic:

```js
const result = await backendImportPreviewVideo(importedVideoPath);
setVideoImport({
  importId: result.importId,
  previewUrl: result.previewUrl,
  metadata: result.metadata,
});
```

Then:

```jsx
<video
  ref={videoRef}
  src={previewUrl ?? undefined}
  muted
  playsInline
  preload="metadata"
/>
```

### 14.2 Force media element reset on import change

When `previewUrl` changes:

1. Pause current video.
2. Remove old `src` if needed.
3. Assign new `src`.
4. Call `video.load()`.
5. Wait for `loadedmetadata`.
6. Restore desired player state if appropriate.

React keying can help:

```jsx
<video key={importId} src={previewUrl} ... />
```

This forces a clean media element lifecycle per import.

### 14.3 Preserve existing sync model

The existing logic based on:

```js
videoRef.current.currentTime
videoRef.current.play()
videoRef.current.pause()
timeupdate
seeking
seeked
```

should remain valid.

Do not introduce HLS-specific abstractions or segment state.

### 14.4 Handle metadata and warnings in UI

If probing returns warnings, the UI can show a non-blocking message such as:

```txt
This file may not play smoothly in the preview because it uses HEVC/10-bit encoding. If playback fails, OVRLEY will offer to create a preview proxy.
```

For this phase, the fallback action can be disabled or marked as future work.

## 15. Playback Health Detection

Create a small hook or internal utility to track native playback health.

Suggested state:

```ts
interface VideoPlaybackHealth {
  metadataLoaded: boolean;
  firstFrameReady: boolean;
  canPlay: boolean;
  lastError?: string;
  stalledCount: number;
  waitingCount: number;
  lastSeekStartedAt?: number;
  lastSeekLatencyMs?: number;
  slowSeekCount: number;
  failed: boolean;
  failureReason?: string;
}
```

### 15.1 Metadata timeout

On new import:

- Start a timer when `src` is assigned.
- If `loadedmetadata` does not fire within a threshold, mark metadata load as slow or failed.

Suggested thresholds:

```txt
Warning: 5 seconds
Failure: 15 seconds
```

These should be configurable constants.

### 15.2 First-frame timeout

Track whether the video reaches a useful ready state:

- `loadeddata`
- `canplay`
- successful `requestVideoFrameCallback` if available

Suggested thresholds:

```txt
Warning: 5 seconds after metadata
Failure: 15 seconds after metadata
```

### 15.3 Seek latency tracking

On user seek:

1. Record timestamp on `seeking`.
2. Resolve on `seeked` or first frame after seek.
3. Store latency.
4. If latency repeatedly exceeds threshold, mark as degraded.

Suggested thresholds:

```txt
Good: < 300 ms
Acceptable: 300-1000 ms
Slow: > 1000 ms
Problematic: repeated > 2000 ms
```

### 15.4 Media errors

Map native `HTMLMediaElement.error.code` values:

```txt
1 MEDIA_ERR_ABORTED
2 MEDIA_ERR_NETWORK
3 MEDIA_ERR_DECODE
4 MEDIA_ERR_SRC_NOT_SUPPORTED
```

Likely handling:

- `MEDIA_ERR_DECODE`: unsupported codec or decoder failure; future proxy fallback.
- `MEDIA_ERR_SRC_NOT_SUPPORTED`: unsupported format; future proxy fallback.
- `MEDIA_ERR_NETWORK`: local server issue, stale import ID, file deleted, or range bug.

## 16. Backend Probe Details

### 16.1 Probe method

Use the existing FFmpeg/ffprobe infrastructure if already bundled.

Recommended command shape:

```txt
ffprobe \
  -v error \
  -print_format json \
  -show_format \
  -show_streams \
  <input>
```

Optional keyframe estimate command:

```txt
ffprobe \
  -v error \
  -select_streams v:0 \
  -skip_frame nokey \
  -show_frames \
  -show_entries frame=pkt_pts_time,best_effort_timestamp_time \
  -of json \
  <input>
```

The keyframe scan can be expensive on very large files. It should be optional, sampled, cached, or skipped initially.

### 16.2 Likely browser-playable heuristic

This should be conservative and used for warnings only.

Likely OK:

```txt
H.264 AVC, 8-bit, yuv420p, MP4/MOV
```

Potentially platform-dependent:

```txt
HEVC/H.265, 8-bit, yuv420p/yuv420p10le
```

Likely problematic:

```txt
10-bit H.264
HEVC 4:2:2
H.264/H.265 4:2:2
ProRes
DNxHR
AV1 if WebView support is uncertain
unusual pixel formats
```

Do not block playback solely based on heuristic. Let the native video element try unless the format is known impossible.

## 17. Logging and Diagnostics

### 17.1 Backend logs

Log at debug level:

- server start address and port
- import ID creation
- imported file size
- content type
- probe warnings
- request method/path
- Range header
- resolved range start/end
- response status
- stale import ID requests
- invalid range requests
- file-open failures

Avoid logging full user file paths in production logs unless the app already does this and the user has opted into diagnostics.

### 17.2 Frontend logs

In development mode, log:

- assigned preview URL
- metadata load duration
- first-frame duration
- media error code/message
- seek latency
- repeated stalls

## 18. Phase-by-Phase Implementation Plan

---

# Phase 1 — Backend Server Skeleton

## Goal

Start a local HTTP server once during app startup and expose a minimal health route.

## Backend tasks

1. Choose HTTP crate.
2. Create `video_server.rs`.
3. Bind server to `127.0.0.1:0`.
4. Store the assigned port.
5. Run server loop on a dedicated thread or async task.
6. Add route:

```txt
GET /health
```

7. Return:

```json
{ "ok": true }
```

8. Store server handle in Tauri managed state.
9. Ensure clean shutdown when app exits.

## Frontend tasks

None required, except optional development call to verify health.

## Deliverables

- Local HTTP server starts with the app.
- Server listens only on `127.0.0.1`.
- Server port is available through backend state.
- `/health` responds successfully.

## Manual tests

1. Start the app.
2. Confirm server starts without blocking the UI.
3. Confirm the server binds to `127.0.0.1`, not `0.0.0.0`.
4. Request `/health` manually during development and confirm `200 OK`.
5. Quit the app and confirm the server thread/task exits.
6. Restart the app multiple times and confirm no port collision occurs.

## Failure checks

- If the server fails to start, the app should not crash without explanation.
- The error should be visible in backend logs.
- The old `convertFileSrc()` path may remain temporarily available as a development fallback.

---

# Phase 2 — Import State and Unique Preview URLs

## Goal

Add backend state for one current video and return a unique preview URL per import.

## Backend tasks

1. Create `VideoImportState`.
2. Create `CurrentVideoImport`.
3. Implement import ID generation.
4. Add command:

```txt
backend_import_preview_video(path)
```

5. Validate that the path exists.
6. Validate that the path is a file.
7. Validate that the file can be opened for reading.
8. Get file size.
9. Infer basic content type from extension.
10. Store import state.
11. Return:

```json
{
  "importId": "...",
  "previewUrl": "http://127.0.0.1:<port>/video/<import_id>",
  "metadata": null
}
```

Metadata can be filled in Phase 5.

12. Add command:

```txt
backend_clear_preview_video()
```

13. Add optional command:

```txt
backend_get_preview_video_state()
```

## Frontend tasks

1. Add backend API wrapper for `backend_import_preview_video`.
2. Add backend API wrapper for `backend_clear_preview_video`.
3. Store `importId` and `previewUrl` in video import state.
4. Do not yet wire the URL to `<video>` unless Phase 3 is ready.

## Deliverables

- Importing a file creates a unique import ID.
- Importing the same file twice creates two different URLs.
- Clearing the video removes current import state.

## Manual tests

1. Import a small MP4.
2. Confirm the returned `previewUrl` contains a unique import ID.
3. Import the same file again.
4. Confirm the second `previewUrl` is different.
5. Clear the import.
6. Confirm backend state reports no current video.
7. Import a nonexistent path through a development command and confirm a clean error.
8. Import a directory path and confirm a clean error.

## Failure checks

- Invalid file path should not update current import state.
- Failed import should not leave stale state behind.
- Rapid import calls should leave only the latest file as current.

---

# Phase 3 — HTTP Video Route Without Range Optimization

## Goal

Serve the current imported file through `GET /video/<import_id>` with basic streaming, before adding full Range logic.

## Backend tasks

1. Add route:

```txt
GET /video/<import_id>
```

2. Validate import ID.
3. Open the current file.
4. Return `404` or `410` for stale/missing import IDs.
5. Return `200 OK` with:

```txt
Accept-Ranges: bytes
Content-Type: <content_type>
Content-Length: <file_size>
```

6. Stream file body in chunks.
7. Do not buffer the full file.

## Frontend tasks

1. Temporarily assign `previewUrl` to the `<video>` element.
2. Add `key={importId}` to force clean media lifecycle.
3. Use `preload="metadata"`.
4. Keep old `convertFileSrc()` behind a temporary feature flag if desired.

## Deliverables

- The video element can load a small MP4 from the local server.
- Server streams bytes from disk.
- The frontend no longer needs `convertFileSrc()` for the tested path.

## Manual tests

1. Import a small MP4.
2. Confirm video metadata loads.
3. Confirm video can play.
4. Confirm the server logs the request.
5. Clear video and confirm the video disappears.
6. Import another video and confirm the old one is not shown.
7. Refresh/reopen the editor view and confirm state behaves as expected.

## Failure checks

- Requesting an old `previewUrl` after re-import should not serve the new file accidentally.
- Requesting `/video/random-id` should return `404` or `410`.
- Deleting the source file while loaded should cause a clean video error, not a backend panic.

---

# Phase 4 — Full HTTP Range Support

## Goal

Implement correct byte-range support for efficient metadata reads and seeking in large files.

## Backend tasks

1. Parse the `Range` header.
2. Support:

```txt
bytes=0-
bytes=N-
bytes=N-M
bytes=-N
```

3. Reject malformed ranges.
4. Reject multipart ranges initially.
5. Return `206 Partial Content` for valid ranges.
6. Return `416 Range Not Satisfiable` for invalid/unsatisfiable ranges.
7. Add correct headers:

```txt
Accept-Ranges: bytes
Content-Type
Content-Length
Content-Range
```

8. Stream only the requested byte range.
9. Add unit tests for range parsing.
10. Add integration/dev tests for actual HTTP responses.

## Frontend tasks

No major changes.

## Deliverables

- Browser/WebView can request only needed byte ranges.
- Large files do not need to be read from the beginning to seek or parse tail metadata.
- Range parser has unit coverage.

## Manual tests

Use curl or an equivalent local request tool during development.

### Test 1: first bytes

Request:

```txt
Range: bytes=0-99
```

Expected:

```txt
206 Partial Content
Content-Length: 100
Content-Range: bytes 0-99/<file_size>
```

### Test 2: from offset to EOF

Request:

```txt
Range: bytes=1000-
```

Expected:

```txt
206 Partial Content
Content-Range: bytes 1000-<file_size - 1>/<file_size>
```

### Test 3: suffix range

Request:

```txt
Range: bytes=-500000
```

Expected:

```txt
206 Partial Content
Content-Length: 500000
Content-Range: bytes <file_size - 500000>-<file_size - 1>/<file_size>
```

### Test 4: invalid range

Request:

```txt
Range: bytes=500-100
```

Expected:

```txt
416 Range Not Satisfiable
Content-Range: bytes */<file_size>
```

### Test 5: too-large range start

Request:

```txt
Range: bytes=<file_size + 1000>-
```

Expected:

```txt
416 Range Not Satisfiable
```

### Test 6: video playback

1. Import a large MP4.
2. Open dev logs.
3. Confirm the video element issues Range requests.
4. Seek near the end of the file.
5. Confirm the backend serves a tail or mid-file byte range instead of streaming from byte 0.

## Failure checks

- No request should allocate memory proportional to video size.
- Invalid ranges should not panic.
- Stale import IDs should not access the current file.
- Multiple simultaneous requests from the video element should be handled correctly.

---

# Phase 5 — Media Probing and Import Warnings

## Goal

Probe imported video files and classify likely playback risks before or during native playback.

## Backend tasks

1. Create `video_probe.rs`.
2. Use existing FFmpeg/ffprobe discovery logic if available.
3. Run metadata probe on import.
4. Parse JSON output.
5. Extract video stream metadata.
6. Extract format metadata.
7. Detect audio stream presence.
8. Detect rotation metadata if present.
9. Compute nominal fps from `avg_frame_rate` or `r_frame_rate`.
10. Populate `VideoProbeMetadata`.
11. Add warning flags.
12. Return metadata from `backend_import_preview_video`.
13. If probe fails, do not block import; return `probe_incomplete` warning.

## Frontend tasks

1. Store metadata and warnings.
2. Optionally show non-blocking warnings.
3. Keep playback attempt active even if warning exists.
4. Reserve UI space/logic for future proxy fallback.

## Deliverables

- Import returns useful metadata.
- UI can distinguish “file imported but possibly problematic” from “file failed.”
- Unsupported/fragile codec cases are visible to the user.

## Manual tests

1. Import standard H.264 8-bit MP4.
2. Confirm metadata looks normal and warnings are empty or minimal.
3. Import HEVC file.
4. Confirm HEVC warning appears if platform support is uncertain.
5. Import 10-bit file if available.
6. Confirm 10-bit warning appears.
7. Import MOV file.
8. Confirm content type and metadata are still handled.
9. Temporarily break ffprobe path and confirm import still proceeds with `probe_incomplete` warning.

## Failure checks

- Probe failure should not crash import.
- Probe should have a timeout.
- Probe should not block the UI indefinitely.
- Very large files should not trigger a full keyframe scan by default.

---

# Phase 6 — Frontend Playback Health Detection

## Goal

Detect when native playback is failing, degraded, or too slow, so the app can later offer proxy fallback.

## Frontend tasks

1. Create `useVideoPlaybackHealth` or equivalent.
2. Track metadata load timing.
3. Track first-frame readiness.
4. Track native media errors.
5. Track stalled/waiting events.
6. Track seek latency.
7. Track repeated slow seeks.
8. Expose health state to UI/store.
9. Add development logging.
10. Add user-facing degraded-playback message only when needed.

## Backend tasks

None required, unless adding debug event reporting.

## Deliverables

- Playback failure is observable.
- Slow metadata loading is observable.
- Slow seeking is observable.
- Decode/source-not-supported errors are distinguishable.

## Manual tests

1. Import known-good MP4.
2. Confirm `loadedmetadata` and `canplay` are detected.
3. Seek repeatedly and confirm latency is recorded.
4. Import unsupported or renamed invalid file.
5. Confirm media error is captured and shown cleanly.
6. Delete source file during playback.
7. Confirm frontend receives a failure rather than hanging silently.
8. Simulate slow server response in development and confirm timeout behavior.

## Failure checks

- Health detection should not interfere with normal playback.
- Slow seek warning should not appear after a single minor delay.
- Warnings should reset on new import.

---

# Phase 7 — Remove or Downgrade `convertFileSrc()` Path

## Goal

Make local HTTP Range playback the default preview path.

## Backend tasks

1. Ensure server starts reliably.
2. Ensure import command returns preview URL reliably.
3. Ensure errors are surfaced through existing error handling.

## Frontend tasks

1. Remove direct `convertFileSrc()` usage from the main video preview path.
2. Optionally keep it behind a development-only fallback flag.
3. Update import flow to always request backend preview URL.
4. Ensure clear/re-import behavior resets the media element.
5. Confirm overlay synchronization still uses the same player API.

## Deliverables

- Main preview path uses local HTTP server.
- Existing playback controls still work.
- Existing overlay timeline sync still works.

## Manual tests

1. Import video.
2. Play/pause from primitive player controls.
3. Scrub timeline.
4. Confirm video follows timeline.
5. Confirm overlay canvas remains visually synchronized.
6. Clear video.
7. Re-import another video.
8. Confirm no stale frame/metadata from previous video appears.

## Failure checks

- Re-import should not reuse stale duration.
- Re-import should not show previous video first frame.
- Clearing should not leave audio/video playing.

---

# Phase 8 — Large-File Validation

## Goal

Validate the architecture with realistic action-camera footage.

## Test assets

Use files representing:

1. GoPro H.264 4K/60.
2. GoPro HEVC 4K/60.
3. DJI H.264/H.265 4K/60.
4. Insta360 exported flat MP4.
5. DaVinci Resolve H.264 export.
6. DaVinci Resolve H.265 export.
7. File with moov atom at end.
8. File with faststart/moov atom at beginning.
9. 10-20 minute file.
10. 60 minute or very large stress-test file if available.

## Manual tests

For each file:

1. Import file.
2. Measure time to `loadedmetadata`.
3. Measure time to first visible frame.
4. Play for 30 seconds.
5. Seek to 25% duration.
6. Seek to 50% duration.
7. Seek to 90% duration.
8. Seek backward to 10% duration.
9. Pause/play repeatedly.
10. Scrub quickly across the timeline.
11. Confirm overlay remains responsive.
12. Confirm memory usage remains stable.
13. Confirm no full-file memory spike occurs.
14. Confirm server logs show byte ranges rather than full linear reads.

## Performance targets

Initial suggested targets:

```txt
Metadata load on SSD: usually < 2 seconds for normal files
Metadata load on HDD / moov-at-end large file: may be several seconds
Seek response: ideally < 300 ms, acceptable < 1000 ms
Repeated seek > 2000 ms: degraded playback warning
Memory: no growth proportional to video size
Disk: no duplicated video files
```

These are not strict correctness requirements. They are validation targets.

## Failure checks

- HEVC unsupported cases should fail cleanly.
- 10-bit unsupported cases should fail cleanly.
- Very slow HDD metadata load should show a waiting/loading state.
- The app should remain responsive while metadata loads.

---

# Phase 9 — Future Fallback Hook, No Full Implementation Yet

## Goal

Prepare a clean control-flow point for future proxy generation without implementing it prematurely.

## Backend tasks

1. Define a placeholder command shape:

```txt
backend_create_preview_proxy(importId)
```

Do not implement unless needed.

2. Define future proxy metadata shape.
3. Ensure import state can later store `playbackMode`:

```ts
type PlaybackMode = 'native_original' | 'proxy_preview';
```

## Frontend tasks

1. Add UI branch for degraded native playback.
2. Placeholder message:

```txt
Preview playback is not smooth for this file. A future version can create a lightweight preview proxy.
```

3. Do not show a non-functional button unless the product decision is to expose it as disabled/coming soon.

## Deliverables

- The codebase has a clear future insertion point for proxy fallback.
- HLS is not introduced accidentally as the default fallback.

## Manual tests

1. Force a simulated playback failure.
2. Confirm the UI enters degraded/fallback-available state.
3. Confirm normal files do not show fallback messaging.

---

## 19. Acceptance Criteria

The implementation is considered successful when:

1. The app starts a loopback-only local HTTP server.
2. Importing a video returns a unique preview URL.
3. The React `<video>` element plays from that URL.
4. The server supports correct `Range` requests.
5. Seeking uses byte ranges rather than linear full-file reads.
6. The original video file is not duplicated.
7. The original video file is not transcoded.
8. Memory usage does not scale with total video file size.
9. Clearing and re-importing videos does not reuse stale metadata.
10. Import probing returns useful metadata and warnings.
11. Unsupported or problematic videos fail visibly and cleanly.
12. Existing overlay/player sync remains intact.
13. Export behavior remains unchanged.

## 20. Risks and Mitigations

| Risk | Impact | Mitigation |
|---|---:|---|
| Browser/WebView cannot decode HEVC or 10-bit source | Preview fails | Detect via probe and media errors; future proxy fallback. |
| Moov atom at end of huge file causes slow metadata load | Slow import/preview start | Range support allows tail read; show loading state and timeout warning. |
| Sparse keyframes cause slow seeking | Poor scrub experience | Detect slow seeks; future proxy fallback or keyframe-aware seek snapping. |
| Incorrect Range implementation | Playback/seeking broken | Unit-test range parser; manually test with curl; test real video element. |
| Stale URL/cache reuse | Wrong duration or old frame shown | Unique import ID per import; key video element by import ID. |
| File deleted during playback | Playback error | Return clean 404; frontend handles media error. |
| Server accidentally exposes filesystem | Security risk | Serve only current file by opaque import ID; no arbitrary path route. |
| Backend server thread panic | Preview unavailable | Catch/log errors; do not crash app if possible. |
| Multiple rapid imports | Race/stale state | Atomic state swap; stale import IDs return 404/410. |

## 21. Recommended First Implementation Slice

If you want the smallest useful slice, implement this first:

1. Server startup on `127.0.0.1:0`.
2. Import state with unique `import_id`.
3. `GET /video/<import_id>`.
4. Correct single-range support.
5. Frontend `<video key={importId} src={previewUrl}>`.
6. Basic media error logging.

This gives you the core architecture without overcommitting to probing, warnings, or future fallback logic.

## 22. Final Recommendation

Implement Option 1 as the default preview architecture.

The correct mental model is not HLS-style streaming. The correct mental model is:

```txt
local random-access file serving + native decoder + byte-range seeking
```

For the current OVRLEY use case — one local source video, no editing, preview-only background, large action-camera files — this is the simplest and most robust primary path.

Do not introduce HLS unless a later, concrete requirement appears that native local range playback plus optional proxy preview cannot satisfy.


---

# Additional Implementation Clarifications (Incorporated Revisions)

## Dedicated HTTP Server Thread

The `tiny_http` request loop MUST run on a dedicated `std::thread`.

`.setup()` runs on the main thread and must return quickly. The blocking HTTP receive loop must never execute directly inside `.setup()`.

Recommended implementation pattern:

```rust
pub fn start(&self) -> Result<(), String> {
    let server = tiny_http::Server::http("127.0.0.1:0")?;

    let port = server.server_addr().to_ip()?.port();

    {
        let mut inner = self.inner.lock().unwrap();
        inner.port = Some(port);
    }

    let inner = self.inner.clone();

    std::thread::spawn(move || {
        loop {
            match server.recv_timeout(
                std::time::Duration::from_millis(200)
            ) {
                Ok(Some(req)) => {
                    handle_request(&inner, req);
                }

                Ok(None) => {}

                Err(_) => break,
            }

            let should_shutdown = {
                inner.lock().unwrap().shutdown
            };

            if should_shutdown {
                break;
            }
        }
    });

    Ok(())
}
```

This ensures:
- `.setup()` returns immediately
- the app startup does not block
- the server remains responsive
- graceful shutdown becomes possible

---

## Streaming Response Implementation

The server MUST stream file ranges directly from disk.

Do NOT:
- allocate entire byte ranges into RAM
- use `read_to_end()`
- build temporary buffers for large requests

Correct implementation pattern:

```rust
let mut file = std::fs::File::open(&path)?;

file.seek(SeekFrom::Start(range_start))?;

let limited = file.take(range_length);

let response = Response::new(
    StatusCode(206),
    headers,
    Box::new(limited),
    Some(range_length),
    None,
);
```

This guarantees:
- constant memory usage
- scalability to 50 GB+ files
- no accidental huge allocations
- stable seek behavior on large files

---

## tiny_http Version Guidance

Preferred dependency:

```toml
tiny_http = "0.12"
```

If unavailable on crates.io:

```toml
tiny_http = "0.11"
```

The API differences are negligible for this implementation.

---

## Removal of backend_get_preview_url()

The original command set included:

```txt
backend_get_preview_url()
```

This command should be removed from the core implementation because:

- `backend_import_preview_video()` already returns the preview URL
- the frontend stores the URL in Zustand state
- the server does not persist across restarts
- an additional getter introduces unnecessary complexity

Recommended command set:

```txt
backend_import_preview_video(path)
backend_clear_preview_video()
backend_get_video_state() // optional diagnostics
```

---

## State Sharing Clarification

The server state is shared between:
- Tauri command handlers
- the dedicated HTTP server thread

using:

```rust
Arc<Mutex<VideoServerState>>
```

Important implementation rule:

DO NOT hold the mutex during:
- file opening
- file seeking
- file streaming
- slow I/O operations

Correct pattern:

```rust
let current_video = {
    let inner = state.lock().unwrap();
    inner.current_video.clone()
};
```

Then:
- release the lock
- perform file I/O
- stream the response

This avoids:
- UI stalls
- blocked command handlers
- deadlocks on slow disks
- poor seek responsiveness

---

## preload Behavior Clarification

`<video>` must use:

```jsx
preload="metadata"
```

NOT:

```jsx
preload="auto"
```

Reason:
- prevents aggressive buffering
- avoids unnecessary reads on huge files
- especially important for 50 GB sources

This change should occur early in the frontend switchover phase.

---

## Existing Probe Reuse

The implementation must extend the existing:

```txt
video_probe.rs
```

instead of introducing a parallel probe system.

The current probe already extracts:
- duration
- FPS
- resolution
- creation time

The implementation should only enrich it with:
- codec
- pixel format
- bit depth
- audio presence
- container
- estimated keyframe interval

---

## Multipart Range Requests

Multipart range requests do NOT need full implementation.

If encountered:
- either serve the first range
- or return a normal `200 OK`

Do NOT hard-fail playback with `416`.

Browsers rarely use multipart ranges for native `<video>` playback.

---

## Simplified Invalid-State Handling

All invalid video states should return:

```txt
404 Not Found
```

This includes:
- no current video
- stale import ID
- invalid import ID
- missing source file

Using `410 Gone` adds unnecessary complexity for a single-user local desktop app.

---

## Metadata Timeout Guidance

Avoid overly aggressive metadata timeout thresholds.

Recommended thresholds:

```txt
Warning: ~10-15s
Failure: ~30s
```

Large HDD-hosted files with moov-at-end may legitimately require significant initial metadata load time.

---

## HEVC / H.265 Clarification

HEVC/H.265 should be treated as:

```txt
supported-but-not-guaranteed
```

Playback policy:

1. Always attempt native playback first:

```txt
HTTP server → native <video>
```

2. If playback fails:
- show warning
- optionally generate preview proxy in a future implementation

Do NOT pre-transcode all HEVC sources during import.
