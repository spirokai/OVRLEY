import { normalizeUpdateRateForFps } from '@/lib/update-rate'

/**
 * Translates backend progress into the shared frontend progress model.
 * @param {object} data Backend render progress payload.
 * @returns {object} Render progress for the current-video and batch panels.
 */
export function createRenderProgress(data) {
  return {
    renderId: data.render_id,
    current: data.current,
    total: data.total,
    percent: data.total > 0 ? Math.round((data.current / data.total) * 100) : 0,
    encoded: data.encoded,
    status: data.status,
    message: data.message,
    estimatedSecondsRemaining: data.estimated_seconds_remaining,
    renderingFps: data.rendering_fps,
    filename: data.filename,
  }
}

/**
 * Estimates a queued job's output frames until the renderer reports its exact total.
 * @param {object} metadata Probed video duration, FPS, and embedded activity duration.
 * @param {object} settings Captured batch render settings.
 * @param {number|null} activityDuration Loaded activity-file duration, or null for per-video telemetry.
 * @returns {number} Estimated output frame count.
 */
export function estimateBatchFrameCount(metadata, settings, activityDuration) {
  if (settings.exportMode === 'composite') return Math.ceil(metadata.duration * metadata.fps)
  const layoutFrames = Math.ceil((activityDuration ?? metadata.activityDuration) * settings.fps)
  return Math.ceil(layoutFrames / normalizeUpdateRateForFps(settings.fps, settings.updateRate))
}

/**
 * Combines completed jobs with the current job for the batch progress panel.
 * @param {object} progress Current job's frontend progress.
 * @param {number} completedFrames Frames from successfully completed jobs.
 * @param {number} totalFrames Total eligible frames, corrected by each job's backend total.
 * @returns {object} Overall batch progress in the shared frontend shape.
 */
export function createBatchRenderProgress(progress, completedFrames, totalFrames) {
  const current = completedFrames + progress.current
  return {
    ...progress,
    current,
    total: totalFrames,
    percent: totalFrames > 0 ? Math.min(current < totalFrames ? 99 : 100, Math.round((current / totalFrames) * 100)) : 0,
    encoded: completedFrames + progress.encoded,
    estimatedSecondsRemaining: progress.renderingFps > 0 ? Math.round((totalFrames - current) / progress.renderingFps) : null,
  }
}
