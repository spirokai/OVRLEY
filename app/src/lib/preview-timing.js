/**
 * Shared preview-timing helpers used by both the live editor canvas and the
 * debug preview-PNG workflow.
 *
 * The key goal is to keep "what second are we previewing?" and "what scene
 * window should the backend render for that second?" in one place so those
 * two paths cannot drift independently.
 */

import { sanitizeIntegerFps } from '@/lib/update-rate'
import { clamp } from '@/lib/utils'

/**
 * Resolves the effective activity duration used by preview-oriented UI.
 *
 * Parsed activities may report duration either through `trim_end_seconds` or
 * metadata, while templates without activity data fall back to the editor's
 * dummy duration. The result is always clamped to a non-negative finite value.
 *
 * @param {object} options
 * @param {number} [options.dummyDurationSeconds] - Fallback duration without activity data.
 * @param {object|null} [options.sourceActivity] - Parsed activity payload.
 * @returns {number} Effective preview/activity duration in seconds.
 */
export function resolveActivityDuration({ dummyDurationSeconds, sourceActivity }) {
  const activityDuration = Number(sourceActivity?.trim_end_seconds ?? sourceActivity?.metadata?.duration_seconds ?? dummyDurationSeconds ?? 0)
  return Math.max(Number.isFinite(activityDuration) ? activityDuration : 0, 0)
}

/**
 * Clamps the current editor playhead into the effective previewable duration.
 *
 * @param {object} options
 * @param {number} [options.dummyDurationSeconds] - Fallback duration without activity data.
 * @param {number} [options.selectedSecond] - Current editor playhead second.
 * @param {object|null} [options.sourceActivity] - Parsed activity payload.
 * @returns {number} Preview second used consistently by canvas and preview rendering.
 */
export function resolvePreviewSecond({ dummyDurationSeconds, selectedSecond, sourceActivity }) {
  const rawSecond = Number(selectedSecond) || 0
  const maxSecond = resolveActivityDuration({ dummyDurationSeconds, sourceActivity })
  return clamp(rawSecond, 0, maxSecond)
}

/**
 * Builds the smallest valid backend render window that still contains the
 * requested preview frame.
 *
 * The backend renderer still expects a `scene.start/end` range, so preview
 * rendering synthesizes a one-frame scene window anchored around the exact
 * preview second. Near the activity end, that window shifts backward so
 * `end > start` remains valid.
 *
 * @param {object} options
 * @param {number} options.activityDuration - Effective activity duration in seconds.
 * @param {number} options.previewSecond - Exact preview second to render.
 * @param {number} options.sceneFps - Scene FPS used to derive one frame of duration.
 * @returns {{ start: number, end: number }} One-frame render window in activity seconds.
 */
export function buildPreviewFrameWindow({ activityDuration, previewSecond, sceneFps }) {
  const safeDuration = Math.max(Number(activityDuration) || 0, 0)
  const safePreviewSecond = clamp(Number(previewSecond) || 0, 0, safeDuration)
  const frameDuration = 1 / sanitizeIntegerFps(sceneFps)

  if (safeDuration <= 0) {
    return {
      start: 0,
      end: frameDuration,
    }
  }

  const maxWindowStart = Math.max(safeDuration - frameDuration, 0)
  const start = clamp(safePreviewSecond, 0, maxWindowStart)
  const end = Math.min(safeDuration, Math.max(start + frameDuration, safePreviewSecond + Number.EPSILON))

  return { start, end }
}
