/**
 * Pure helpers for resolving preview-video source state.
 */

import { convertFileSrc } from '@tauri-apps/api/core'
import { USE_LOCAL_HTTP_VIDEO_PREVIEW } from '../data/videoPreviewConstants'

function currentWindowProtocol() {
  if (typeof window === 'undefined' || !window.location?.protocol) {
    return ''
  }

  return window.location.protocol
}

/**
 * Resolves the active source URL for the preview video element.
 *
 * @param {object} options - Source inputs.
 * @param {(path: string) => string} [options.convertFileSrc] - File-source resolver.
 * @param {string|null} options.importedVideoPath - Imported video path.
 * @param {string|null} options.importedVideoPreviewUrl - Local preview-server URL.
 * @param {string} [options.windowProtocol] - Current app window protocol for mixed-content checks.
 * @param {boolean} [options.useLocalHttpPreview] - Whether HTTP preview URLs should win.
 * @returns {string} Video source URL for the preview element.
 */
export function resolveVideoPreviewSource({
  convertFileSrc: convertFileSrcImpl = convertFileSrc,
  importedVideoPath,
  importedVideoPreviewUrl,
  windowProtocol = currentWindowProtocol(),
  useLocalHttpPreview = USE_LOCAL_HTTP_VIDEO_PREVIEW,
}) {
  const blocksMixedContent = windowProtocol === 'https:' && importedVideoPreviewUrl?.startsWith('http://')

  if (useLocalHttpPreview && importedVideoPreviewUrl && !blocksMixedContent) {
    return importedVideoPreviewUrl
  }

  if (importedVideoPath) {
    return convertFileSrcImpl(importedVideoPath)
  }

  return ''
}

/**
 * Reports whether the current timeline playhead sits outside the imported video window.
 *
 * @param {object} options - Window inputs.
 * @param {number} options.selectedSecond - Current preview second.
 * @param {number} options.videoDuration - Imported video duration in seconds.
 * @param {number} options.videoSyncOffsetSeconds - Timeline second where the video starts.
 * @returns {boolean} Whether the playhead is outside the imported video range.
 */
export function isVideoPreviewOutOfRange({ selectedSecond, videoDuration, videoSyncOffsetSeconds }) {
  const safeSelectedSecond = Number(selectedSecond) || 0
  const safeOffset = Number(videoSyncOffsetSeconds) || 0
  const safeDuration = Number(videoDuration) || 0

  return safeSelectedSecond < safeOffset || safeSelectedSecond > safeOffset + safeDuration
}
