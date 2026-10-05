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
