import { DEFAULT_RENDER_PROGRESS } from '@/store/store-utils'

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
 * Presents authoritative processed work without counting failures as rendered frames.
 * @param {object|null} snapshot Native batch snapshot.
 * @returns {object} Batch panel progress.
 */
export function createBatchProgress(snapshot) {
  if (snapshot === null) return DEFAULT_RENDER_PROGRESS
  return {
    current: snapshot.processedFrames,
    total: snapshot.plannedFrames,
    percent: snapshot.plannedFrames > 0 ? Math.round((snapshot.processedFrames / snapshot.plannedFrames) * 100) : 0,
    encoded: snapshot.encodedFrames,
    status: snapshot.phase,
    estimatedSecondsRemaining: snapshot.estimatedSecondsRemaining,
    renderingFps: snapshot.elapsedSeconds > 0 ? snapshot.renderedFrames / snapshot.elapsedSeconds : null,
  }
}

/**
 * Formats native current-item progress for the row presentation.
 * @param {object|null} snapshot Native batch snapshot.
 * @returns {object} Active row progress.
 */
export function createBatchItemProgress(snapshot) {
  const progress = snapshot?.currentItemProgress
  if (!progress) return DEFAULT_RENDER_PROGRESS
  return {
    current: progress.currentFrames,
    total: progress.plannedFrames,
    percent: progress.plannedFrames > 0 ? Math.round((progress.currentFrames / progress.plannedFrames) * 100) : 0,
    encoded: progress.encodedFrames,
    status: snapshot.phase,
    estimatedSecondsRemaining: progress.estimatedSecondsRemaining,
    renderingFps: progress.elapsedSeconds > 0 ? progress.renderedFrames / progress.elapsedSeconds : null,
  }
}
