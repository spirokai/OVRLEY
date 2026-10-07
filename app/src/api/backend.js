/**
 * Implements API helpers for backend.
 */

import { rasterResourceIds } from '@/lib/widget/raster-resources'

/**
 * Shared Tauri runtime detection.
 * Returns true when running inside a Tauri desktop shell (IPC available).
 * @returns {boolean}
 */
export function hasTauriRuntime() {
  return typeof window !== 'undefined' && typeof window.__TAURI_INTERNALS__ !== 'undefined'
}

/**
 * Returns invoke.
 * @returns {Promise<*>} Promise resolving to the operation result.
 */
async function getInvoke() {
  if (!hasTauriRuntime()) return null
  const { invoke } = await import('@tauri-apps/api/core')
  return invoke
}

/**
 * Returns invoke or throws when desktop runtime is unavailable.
 * @returns {Promise<*>} Promise resolving to the Tauri invoke helper.
 */
async function requireInvoke() {
  const invoke = await getInvoke()
  if (!invoke) {
    throw new Error('OVRLEY desktop runtime is required.')
  }
  return invoke
}

/**
 * Normalizes values thrown by the Tauri bridge to standard Error instances.
 *
 * @param {*} error - Rejection value from the invoke bridge.
 * @param {string} fallbackMessage - Message used when no better detail exists.
 * @returns {Error} Normalized Error instance.
 */
function normalizeBackendError(error, fallbackMessage = 'Unknown backend error') {
  if (error instanceof Error) {
    return error
  }

  if (typeof error === 'string' && error.trim()) {
    return new Error(error)
  }

  if (error && typeof error === 'object' && typeof error.message === 'string' && error.message.trim()) {
    const normalized = new Error(error.message)
    Object.assign(normalized, error)
    return normalized
  }

  return new Error(fallbackMessage)
}

/**
 * Invokes a Tauri command and normalizes bridge errors.
 *
 * @param {string} tauriCmd - Tauri command name to invoke.
 * @param {object} tauriArgs - Argument payload passed to the Tauri command.
 * @returns {Promise<*>} Promise resolving to the raw command result.
 */
async function invokeCommand(tauriCmd, tauriArgs = {}) {
  const invoke = await requireInvoke()

  try {
    return await invoke(tauriCmd, tauriArgs)
  } catch (error) {
    console.error(`[Backend] Tauri bridge error for ${tauriCmd}:`, error)
    throw normalizeBackendError(error)
  }
}

/**
 * Serializes data to JSON while safely handling cyclic references.
 *
 * @param {*} obj - Value for obj.
 * @returns {*} Result produced by the helper.
 */
function safeJsonStringify(obj) {
  try {
    return JSON.stringify(obj)
  } catch {
    // Only use cycle breaking if normal stringify fails
    const seen = new WeakSet()
    return JSON.stringify(obj, (key, value) => {
      if (typeof value === 'object' && value !== null) {
        if (seen.has(value)) {
          return // Circular reference found
        }
        seen.add(value)
      }
      return value
    })
  }
}

/**
 * Handles api call.
 *
 * @param {*} tauriCmd - Tauri command name to invoke.
 * @param {*} tauriArgs - Argument payload passed to the Tauri command.
 * @returns {Promise<*>} Promise resolving to the operation result.
 */
async function apiCall(tauriCmd, tauriArgs) {
  // In Tauri, call Rust directly. No sidecar/socket probing remains.
  const result = await invokeCommand(tauriCmd, tauriArgs)
  return JSON.parse(result)
}

/**
 * Handles health check.
 * @returns {Promise<*>} Promise resolving to the operation result.
 */
export async function healthCheck() {
  return apiCall('backend_health', {})
}

/**
 * Handles socket ready.
 * @returns {Promise<*>} Promise resolving to the operation result.
 */
export async function socketReady() {
  return Boolean(await getInvoke())
}

/**
 * Renders video.
 *
 * @param {*} config - Overlay template configuration data.
 * @param {*} parsedActivity - Normalized activity payload used by the app.
 * @returns {Promise<*>} Promise resolving to the operation result.
 */
export async function renderVideo(config, parsedActivity, { outputPath, overwrite = false } = {}) {
  const safeConfig = safeJsonStringify(config)
  const safeParsedActivity = safeJsonStringify(parsedActivity)
  return apiCall('backend_render', {
    configJson: safeConfig,
    parsedActivityJson: safeParsedActivity,
    outputPath,
    overwrite,
  })
}

/**
 * Requests a fresh Rust-owned render output suggestion.
 * @param {'transparent'|'composite'} outputKind - Output container kind.
 * @param {string|undefined} rememberedDirectory - Optional accepted directory.
 * @returns {Promise<string>} Absolute suggested output path.
 */
export async function suggestRenderOutputPath(outputKind, rememberedDirectory) {
  return invokeCommand('backend_suggest_output_path', {
    outputKind,
    rememberedDirectory: rememberedDirectory ?? null,
  })
}

/**
 * Finalizes extracted raw activity samples through the Rust backend pipeline.
 *
 * @param {object} rawActivity - RawActivity contract produced by format parsers.
 * @returns {Promise<object>} Promise resolving to parsed activity and debug payload.
 */
export async function finalizeActivity(rawActivity) {
  return apiCall('backend_finalize_activity', {
    rawActivityJson: safeJsonStringify(rawActivity),
  })
}

/**
 * Parses a native CSV activity path through the Rust columnar pipeline.
 *
 * @param {string} path - Absolute path returned by the native file picker.
 * @returns {Promise<object>} Promise resolving to the native finalized activity response.
 */
export async function parseCsvActivity(path) {
  return invokeCommand('backend_parse_csv_activity', { path })
}

/**
 * Parses a native VBO activity path through the Rust columnar pipeline.
 *
 * @param {string} path - Absolute path returned by the native file picker.
 * @returns {Promise<object>} Promise resolving to the native finalized activity response.
 */
export async function parseVboActivity(path) {
  return invokeCommand('backend_parse_vbo_activity', { path })
}

/**
 * Renders a transparent PNG for a single preview second.
 *
 * @param {*} config - Overlay template configuration data.
 * @param {*} parsedActivity - Normalized activity payload used by the app.
 * @param {number} second - Timeline second to render.
 * @returns {Promise<object>} Promise resolving to the generated preview metadata.
 */
export async function renderPreviewFrame(config, parsedActivity, second) {
  const safeConfig = safeJsonStringify(config)
  const safeParsedActivity = safeJsonStringify(parsedActivity)
  return apiCall('backend_render_preview_frame', {
    configJson: safeConfig,
    parsedActivityJson: safeParsedActivity,
    second,
  })
}

/**
 * Returns platform info.
 * @returns {Promise<object>} Promise resolving to the operation result.
 */
export async function getPlatformInfo() {
  const payload = await invokeCommand('backend_current_os')
  return typeof payload === 'string' ? JSON.parse(payload) : payload
}

/**
 * Returns the Rust-owned loopback URL template for cached OpenFreeMap styles.
 * The null browser result is intentional: the map service only exists in the
 * desktop runtime.
 *
 * @returns {Promise<string|null>} MapLibre style URL template, or null outside Tauri.
 */
export async function getMapStyleUrlTemplate() {
  if (!hasTauriRuntime()) return null

  const urlTemplate = await invokeCommand('backend_get_map_style_url_template')
  if (typeof urlTemplate !== 'string' || !/^http:\/\/127\.0\.0\.1:\d+\/styles\/\{style\}$/.test(urlTemplate)) {
    throw new Error('Invalid map style URL template returned by backend')
  }
  return urlTemplate
}

/**
 * Returns the Rust-owned application distribution kind.
 * @returns {Promise<'installed'|'portable'>} Promise resolving to the canonical distribution kind.
 */
export async function getDistributionKind() {
  const distributionKind = await invokeCommand('backend_distribution_kind', {})
  if (distributionKind !== 'installed' && distributionKind !== 'portable') {
    throw new Error(`Invalid distribution kind returned by backend: ${String(distributionKind)}`)
  }
  return distributionKind
}

/**
 * Opens the official Windows HEVC playback support page.
 * @returns {Promise<void>} Promise resolving when the system opener has been invoked.
 */
export async function openHevcSupport() {
  await invokeCommand('backend_open_hevc_support')
}

/**
 * Lists available fonts.
 * @returns {Promise<object>} Canonical bundled and system family catalog.
 */
export async function listAvailableFonts() {
  // Font discovery is Rust-owned and unavailable in frontend-only development.
  if (!hasTauriRuntime()) return { recommendedFonts: [], systemFonts: [] }
  return apiCall('backend_list_system_fonts', {})
}

/** @param {string} fontId Canonical font ID. @returns {Promise<object>} Resolved capabilities. */
export function getFontCapabilities(fontId) {
  return apiCall('backend_font_capabilities', { fontId })
}

/** @param {string} fontId Canonical font ID. @param {number} faceIndex Catalog face index. @returns {Promise<number[]>} Font bytes. */
export function getFontData(fontId, faceIndex) {
  return invokeCommand('backend_font_data', { fontId, faceIndex })
}

/**
 * Returns render progress.
 * @returns {Promise<*>} Promise resolving to the operation result.
 */
export async function getRenderProgress() {
  return apiCall('backend_progress', {})
}

/**
 * Subscribes to streamed `render-progress` events emitted by the backend
 * `RenderController`. Each event carries a `RenderProgress` payload identical
 * to what `getRenderProgress` would return as a snapshot — the streaming
 * subscription replaces the previous 500 ms polling loop and delivers updates
 * at the true frame-production rate (per in-order frame-front advancement).
 *
 * The returned `UnlistenFn` must be invoked to release the subscription when
 * the consumer unmounts or the render session ends.
 *
 * @param {(progress: object) => void} handler - Called with each `RenderProgress` payload.
 * @returns {Promise<() => void>} Resolves to an unlisten function.
 */
export async function subscribeRenderProgress(handler) {
  if (!hasTauriRuntime()) {
    return () => {}
  }
  const { listen } = await import('@tauri-apps/api/event')
  return listen('render-progress', (event) => handler(event.payload))
}

/**
 * Submits the captured queue once. Rust owns all preparation and execution.
 * Subscribe to batch progress before submission, then read the initial snapshot.
 * @param {object} request Canonical BatchRenderRequest.
 * @returns {Promise<{batchId: string, snapshot: object}>} Accepted batch identity and snapshot.
 */
export async function submitBatchRender(request) {
  return invokeCommand('backend_submit_batch', { request })
}

/**
 * Reads the retained authoritative snapshot, including terminal results.
 * @param {string} batchId Accepted batch identity.
 * @returns {Promise<object>} Canonical BatchSnapshot.
 */
export async function getBatchRenderSnapshot(batchId) {
  return invokeCommand('backend_batch_snapshot', { batchId })
}

/**
 * Requests cancellation; terminal cancellation follows native cleanup.
 * @param {string} batchId Accepted batch identity.
 * @returns {Promise<object>} Current canonical BatchSnapshot.
 */
export async function cancelBatchRender(batchId) {
  return invokeCommand('backend_cancel_batch', { batchId })
}

/**
 * Observes full queue transitions and compact frame updates. Both carry native revisions.
 * @param {(event: {kind: 'snapshot'|'progress', data: object}) => void} handler Native event observer.
 * @returns {Promise<() => void>} Listener disposal function.
 */
export async function subscribeBatchRenderProgress(handler) {
  const { listen } = await import('@tauri-apps/api/event')
  return listen('batch-render-progress', (event) => handler(event.payload))
}

/**
 * Checks whether cancel render.
 * @returns {Promise<*>} Promise resolving to the operation result.
 */
export async function cancelRender() {
  return apiCall('backend_cancel', {})
}

async function openFolder(command) {
  return apiCall(command, {})
}

/**
 * Opens the remembered render output directory.
 * @param {string|undefined} directory - Optional remembered absolute directory.
 * @returns {Promise<*>} Promise resolving to the operation result.
 */
export async function openOutputDirectory(directory) {
  return apiCall('backend_open_output_directory', { directory: directory ?? null })
}

/**
 * Opens the user templates directory.
 * @returns {Promise<*>} Promise resolving to the operation result.
 */
export async function openTemplates() {
  return openFolder('backend_open_templates')
}

/**
 * Opens video.
 *
 * @param {string} outputPath - Complete absolute target path.
 * @returns {Promise<*>} Promise resolving to the operation result.
 */
export async function openVideo(outputPath) {
  return apiCall('backend_open_video', { outputPath })
}

/**
 * Lists templates.
 * @returns {Promise<Array<*>>} Promise resolving to the operation result.
 */
export async function listTemplates() {
  return apiCall('backend_list_templates', {})
}

/**
 * Returns template.
 *
 * @param {*} filename - Target filename for the operation.
 * @returns {Promise<*>} Promise resolving to the operation result.
 */
export async function getTemplate(filename) {
  return apiCall('backend_get_template', { filename })
}

/**
 * @param {*} filename - Target filename for the operation.
 * @returns {Promise<*>} Promise resolving to the operation result.
 */
export async function getDefaultTemplateSavePath(filename) {
  return invokeCommand('default_template_save_path', { filename })
}

/**
 * Writes template file.
 *
 * @param {*} path - Filesystem path for the target resource.
 * @param {*} contents - Serialized file contents to write.
 * @returns {Promise<*>} Promise resolving to the operation result.
 */
export async function writeTemplateFile(path, contents) {
  return invokeCommand('write_template_file', { path, contents })
}

/**
 * Reads raw bytes from one user-selected file path via the native Tauri shell.
 *
 * @param {string} path - Absolute path returned by the native file picker.
 * @returns {Promise<Uint8Array>} Promise resolving to the file bytes.
 */
export async function readSelectedFileBytes(path) {
  const payload = await invokeCommand('read_selected_file_bytes', { path })
  return payload instanceof Uint8Array ? payload : new Uint8Array(payload)
}

/** @param {string} path Absolute resolved source path. */
export async function selectedPathIsFile(path) {
  return invokeCommand('selected_path_is_file', { path })
}

/** @param {string} path - Absolute path returned by the native raster picker.
 * @returns {Promise<{width: number, height: number, resourceId: string}>} Validated Rust-owned image.
 * @throws {Error} Raster failures include a code; bridge failures propagate without a raster code.
 */
export async function loadSelectedRaster(path) {
  return invokeCommand('load_selected_raster', { path })
}

/** @param {string} resourceId - Rust-owned immutable image identity. @returns {Promise<string>} Oriented PNG bytes encoded as base64. */
export async function rasterPreviewPng(resourceId) {
  return invokeCommand('raster_preview_png', { resourceId })
}

/**
 * Lists supported video files (mp4/mov/mkv) directly inside a directory.
 * @param {string} directory - Absolute directory path.
 * @returns {Promise<string[]>} Absolute video paths sorted by name.
 */
export async function listDirectoryVideoFiles(directory) {
  return invokeCommand('list_directory_video_files', { directory })
}

/** @returns {Promise<string>} Absolute Documents/OVRLEY/projects directory. */
export async function getDefaultProjectDirectory() {
  return invokeCommand('default_project_directory')
}

/**
 * Lists `.oly` projects directly inside a project directory.
 * @param {string} directory Absolute project directory.
 * @returns {Promise<Array<{name: string, path: string, thumbnailDataUrl: string|null}>>} Project summaries sorted by name.
 */
export async function listProjectFiles(directory) {
  const projects = await invokeCommand('list_project_files', { directory })
  if (
    !Array.isArray(projects) ||
    projects.some(
      (project) =>
        !project ||
        typeof project !== 'object' ||
        typeof project.name !== 'string' ||
        !project.name ||
        typeof project.path !== 'string' ||
        !project.path ||
        (project.thumbnailDataUrl !== null &&
          (typeof project.thumbnailDataUrl !== 'string' || !project.thumbnailDataUrl.startsWith('data:image/png;base64,'))),
    )
  ) {
    throw new Error('Invalid project list returned by backend')
  }
  return projects
}

/**
 * Reads and validates an OVRLEY project archive.
 * @param {string} path - Absolute `.oly` path.
 * @returns {Promise<object>} Validated project and resolved source paths.
 */
export async function readProjectFile(path) {
  return invokeCommand('read_project_file', { path })
}

/**
 * Validates and atomically writes an OVRLEY project archive.
 * @param {string} path - Absolute `.oly` path.
 * @param {string} projectJson - Canonical project JSON.
 * @param {object} config - Current editor config containing Rust resource identities.
 * @returns {Promise<string>} Written path.
 */
export async function writeProjectFile(path, projectJson, config) {
  return invokeCommand('write_project_file', { path, projectJson, rasterResourceIds: rasterResourceIds(config) })
}

/**
 * Imports a video into the local HTTP preview server.
 *
 * @param {string} path - Absolute path to the source video file.
 * @returns {Promise<object>} Promise resolving to preview URL, import ID, metadata, and warnings.
 */
export async function importPreviewVideo(path) {
  return apiCall('backend_import_preview_video', { path })
}

/**
 * Probes a video without changing preview-server state.
 * @param {string} path Absolute source video path.
 * @returns {Promise<object>} Canonical source metadata.
 */
export async function preparePreviewVideo(path) {
  return apiCall('backend_prepare_preview_video', { path })
}

/**
 * Opens a fresh configuration inspection; it owns no editor preview or render.
 * @returns {Promise<{inspectionId: string}>} Opaque backend session identity.
 */
export async function createVideoInspection() {
  return apiCall('backend_create_video_inspection', {})
}

/**
 * @typedef {object} InspectedVideoSource
 * @property {string} sourceId Session-owned descriptor identity.
 * @property {object} metadata Canonical probe metadata with required timing, coded resolution, rotationDegrees, and timestamp provenance.
 * @property {string} metadata.path Canonical absolute source path.
 * @property {number} metadata.duration Source duration in seconds.
 * @property {number} metadata.fps Frame rate derived from its exact rational components.
 * @property {number} metadata.fpsNum Exact frame-rate numerator.
 * @property {number} metadata.fpsDen Exact frame-rate denominator.
 * @property {{width: number, height: number}} metadata.resolution Coded source dimensions.
 * @property {0|90|180|270} metadata.rotationDegrees This source's normalized display rotation, used by codec-specific rendering.
 * @property {string|null} metadata.creationTime Probed creation time, without the editor's override.
 * @property {string|null} metadata.syncTime Embedded synchronization hint when available.
 * @property {string|null} metadata.timeSource Timestamp provenance; trusted GPS timestamps keep absolute-time semantics.
 * @property {{width: number, height: number}} displayResolution Dimensions after applying this source's rotation.
 * @property {{sizeBytes: number, modifiedAtUnixNanos: string}} stamp File identity observed before and after probing.
 */

/**
 * Inspects a queued or calibration video without registering a preview or extracting its parsed activity.
 * Embedded activity resolution remains job preparation; duration supplies provisional work estimates.
 * Effective editor timestamp overrides stay in calibration inputs, separate from this file descriptor.
 * @param {string} inspectionId Active configuration inspection identity.
 * @param {string} path Source path, including reference videos outside the queued folder.
 * @returns {Promise<InspectedVideoSource>} Session-owned metadata descriptor.
 */
export async function inspectVideoSource(inspectionId, path) {
  return apiCall('backend_inspect_video_source', { inspectionId, path })
}

/**
 * Plans native batch destinations and frame totals for fresh inspected sources.
 * Coverage for embedded telemetry is finalized during owning-job preparation.
 * @param {{inspectionId: string, sourceIds: string[], calibrationSourceId: string|null}} selection Session-owned source identities.
 * @param {{exportMode: string, exportCodec: string, fps: number, updateRate: number, qualityType: string, qualityValue: number, qsvFullInitArgs: string[]|null}} encoding Captured encoder settings.
 * @param {string} outputDirectory Absolute output folder.
 * @returns {Promise<{status: 'planned', plans: Array<{sourceId: string, outputPath: string, outputDurationSeconds: number, plannedFrames: number, containerFpsNum: number, containerFpsDen: number}>}|{status: 'rejected', inspectionId: string, issues: object[]}>} Native planning result.
 */
export async function planBatchOutputs(selection, encoding, outputDirectory) {
  return apiCall('backend_plan_batch_outputs', { selection, encoding, outputDirectory })
}

/**
 * Discards session descriptors and prevents in-flight inspection from publishing into the closed session.
 * Accepted execution retains its owned descriptors independently.
 * @param {string} inspectionId Configuration inspection identity.
 * @returns {Promise<void>} Resolves after disposal.
 */
export async function disposeVideoInspection(inspectionId) {
  return invokeCommand('backend_dispose_video_inspection', { inspectionId })
}

/**
 * Registers an already-probed video with the local preview server.
 * @param {string} path Absolute source video path.
 * @returns {Promise<{importId: string, previewUrl: string}>} Runtime preview identity.
 */
export async function registerPreviewVideo(path) {
  return apiCall('backend_register_preview_video', { path })
}

/**
 * Clears the currently registered local HTTP preview video.
 *
 * @returns {Promise<*>} Promise resolving when the preview has been cleared.
 */
export async function clearPreviewVideo() {
  return apiCall('backend_clear_preview_video', {})
}

/**
 * Returns current local HTTP preview server state.
 *
 * @returns {Promise<object|null>} Promise resolving to current preview server state or null.
 */
export async function getVideoState() {
  return apiCall('backend_get_video_state', {})
}

/**
 * Detects available ffmpeg MP4 codecs and hardware acceleration methods.
 *
 * @returns {Promise<object>} Promise resolving to available codec flags.
 */
export async function detectCodecs() {
  const result = await invokeCommand('backend_detect_codecs', {})
  return typeof result === 'string' ? JSON.parse(result) : result
}

/**
 * Builds elevation widget geometry from config and activity data.
 *
 * Returns simplified, projected geometry points for the elevation preview
 * without rendering a Skia surface. The JS frontend uses this to avoid
 * duplicating the geometry pipeline.
 *
 * @param {*} config - Overlay template configuration data.
 * @param {*} parsedActivity - Normalized activity payload used by the app.
 * @returns {Promise<object>} Promise resolving to elevation geometry (points, progressValues, bbox, etc.).
 */
export async function buildElevationGeometry(config, parsedActivity) {
  const safeConfig = safeJsonStringify(config)
  const safeParsedActivity = safeJsonStringify(parsedActivity)
  return apiCall('backend_build_elevation_geometry', {
    configJson: safeConfig,
    parsedActivityJson: safeParsedActivity,
  })
}

/**
 * Builds route widget geometry from config and activity data.
 *
 * Returns simplified, projected geometry points for the route preview
 * without rendering a Skia surface. The JS frontend uses this to avoid
 * duplicating the geometry pipeline.
 *
 * @param {*} config - Overlay template configuration data.
 * @param {*} parsedActivity - Normalized activity payload used by the app.
 * @returns {Promise<object>} Promise resolving to route geometry (points, progressValues, bbox, etc.).
 */
export async function buildRouteGeometry(config, parsedActivity) {
  const safeConfig = safeJsonStringify(config)
  const safeParsedActivity = safeJsonStringify(parsedActivity)
  return apiCall('backend_build_route_geometry', {
    configJson: safeConfig,
    parsedActivityJson: safeParsedActivity,
  })
}

/**
 * Extracts MP4 telemetry from a video file.
 *
 * @param {string} filePath - Absolute path to the video file.
 * @returns {Promise<object|null>} Promise resolving to the native finalized activity response or null if no telemetry is found.
 */
export async function extractVideoTelemetry(filePath) {
  return invokeCommand('backend_extract_video_telemetry', { filePath })
}
