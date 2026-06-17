/**
 * Derives benchmark frame updates from a wall-clock anchor, matching the
 * live preview's frame-index dedupe instead of interval-gating rAF ticks.
 *
 * @param {object} options
 * @param {number} options.nowMs - Current scheduler timestamp.
 * @param {number} options.benchmarkStartMs - Benchmark wall-clock anchor.
 * @param {number} options.lastRenderedFrameIndex - Last frame index rendered.
 * @param {number} options.targetFps - Expected widget update rate.
 * @returns {{ droppedFrames: number, frameIndex: number, shouldRender: boolean }}
 */
export function resolveBenchmarkFrameUpdate({ nowMs, benchmarkStartMs, lastRenderedFrameIndex, targetFps }) {
  const safeTargetFps = Math.max(Number(targetFps) || 0, 1)
  const elapsedMs = Math.max(Number(nowMs) - Number(benchmarkStartMs), 0)
  const frameIndex = Math.floor((elapsedMs * safeTargetFps) / 1000 + 1e-6)
  const shouldRender = frameIndex !== lastRenderedFrameIndex
  const droppedFrames = lastRenderedFrameIndex >= 0 ? Math.max(frameIndex - lastRenderedFrameIndex - 1, 0) : 0

  return {
    droppedFrames,
    frameIndex,
    shouldRender,
  }
}
