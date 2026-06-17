/**
 * Benchmark metrics collection and computation for the Wasm preview POC.
 *
 * Tracks initialization time, per-frame draw time, frame intervals,
 * dropped frames, and produces a final summary with pass/fail decision.
 */

const DEFAULT_CANVAS_HEIGHT = 1080
const DEFAULT_CANVAS_WIDTH = 1920
const DEFAULT_TARGET_FPS = 30
const BENCHMARK_DURATION_MS = 10_000

export function getPassP95FrameIntervalThresholdMs(targetFps) {
  const safeTargetFps = Math.max(Number(targetFps) || 0, 1)
  return Math.round((1000 / safeTargetFps) * 1.2 * 100) / 100
}

export function getPassAvgFpsThreshold(targetFps) {
  const safeTargetFps = Math.max(Number(targetFps) || 0, 1)
  return Math.max(Math.round((safeTargetFps - 1) * 10) / 10, 1)
}

/**
 * Creates a fresh benchmark metrics collector.
 *
 * @returns {object} Collector with methods to record frames and produce a summary.
 */
export function createBenchmarkCollector({
  canvasHeight = DEFAULT_CANVAS_HEIGHT,
  canvasWidth = DEFAULT_CANVAS_WIDTH,
  targetFps = DEFAULT_TARGET_FPS,
} = {}) {
  /** @type {number[]} Draw times in ms (wall-clock of the render call). */
  const drawTimes = []

  /** @type {number[]} Frame intervals in ms (time between consecutive frame starts). */
  const frameIntervals = []

  /** @type {number[]} Per-phase timings. */
  const prepareTimes = []
  const wasmDrawTimes = []
  const bufferCopyTimes = []
  const putImageTimes = []

  let initStartMs = 0
  let initEndMs = 0
  let benchmarkStartMs = 0
  let benchmarkEndMs = 0
  let totalFramesRequested = 0
  let framesDrawn = 0
  let droppedFrames = 0
  let lastFrameStartMs = 0
  let crashed = false
  let error = null
  const safeTargetFps = Math.max(Number(targetFps) || 0, 1)
  const passP95FrameIntervalMs = getPassP95FrameIntervalThresholdMs(safeTargetFps)
  const passAvgFps = getPassAvgFpsThreshold(safeTargetFps)

  return {
    /** Record the start of the initialization phase. */
    markInitStart() {
      initStartMs = performance.now()
    },

    /** Record the end of the initialization phase. */
    markInitEnd() {
      initEndMs = performance.now()
    },

    /** Record the start of the benchmark run. */
    markBenchmarkStart(nowMs = performance.now()) {
      benchmarkStartMs = nowMs
      lastFrameStartMs = benchmarkStartMs
    },

    /**
     * Record a single frame's draw time and phase breakdown.
     *
     * @param {number} drawMs - Wall-clock ms spent in the render call.
     * @param {object} [timing] - Optional per-phase timing breakdown.
     */
    recordFrameDraw(drawMs, timing) {
      drawTimes.push(drawMs)
      if (timing) {
        prepareTimes.push(timing.prepare)
        wasmDrawTimes.push(timing.wasmDraw)
        bufferCopyTimes.push(timing.bufferCopy)
        putImageTimes.push(timing.putImage)
      }
      framesDrawn++
    },

    /**
     * Record that a frame was requested but not drawn (dropped).
     *
     * @param {number} frameStartMs - When the frame was scheduled.
     */
    recordDroppedFrame(frameStartMs) {
      droppedFrames++
      totalFramesRequested++
      lastFrameStartMs = frameStartMs
    },

    /**
     * Mark that a frame was requested and drawn, recording its interval.
     *
     * @param {number} frameStartMs - When the frame was scheduled.
     */
    recordFrameInterval(frameStartMs) {
      if (lastFrameStartMs > 0) {
        frameIntervals.push(frameStartMs - lastFrameStartMs)
      }
      totalFramesRequested++
      lastFrameStartMs = frameStartMs
    },

    /** Record that the benchmark ended due to a crash. */
    markCrashed(err) {
      crashed = true
      error = err
      benchmarkEndMs = performance.now()
    },

    /** Record the end of the benchmark run. */
    markBenchmarkEnd() {
      benchmarkEndMs = performance.now()
    },

    /**
     * Compute percentile from a sorted array.
     *
     * @param {number[]} sorted - Ascending-sorted array.
     * @param {number} p - Percentile (0-100).
     * @returns {number} Value at the percentile.
     */
    percentile(sorted, p) {
      if (sorted.length === 0) return 0
      const idx = Math.ceil((p / 100) * sorted.length) - 1
      return sorted[Math.max(0, idx)]
    },

    /**
     * Produce the live summary snapshot (computed on demand during the run).
     *
     * @returns {object} Live metrics.
     */
    liveSummary() {
      const now = performance.now()
      const elapsed = now - benchmarkStartMs
      const avgFps = framesDrawn > 0 ? (framesDrawn * 1000) / elapsed : 0

      const sortedDraw = [...drawTimes].sort((a, b) => a - b)
      const sortedIntervals = [...frameIntervals].sort((a, b) => a - b)
      const sortedPrepare = [...prepareTimes].sort((a, b) => a - b)
      const sortedWasm = [...wasmDrawTimes].sort((a, b) => a - b)
      const sortedBuf = [...bufferCopyTimes].sort((a, b) => a - b)
      const sortedPut = [...putImageTimes].sort((a, b) => a - b)

      return {
        elapsedMs: elapsed,
        avgFps: Math.round(avgFps * 10) / 10,
        totalFramesRequested,
        framesDrawn,
        droppedFrames,
        p50DrawMs: Math.round(this.percentile(sortedDraw, 50) * 100) / 100,
        p95DrawMs: Math.round(this.percentile(sortedDraw, 95) * 100) / 100,
        maxDrawMs: sortedDraw.length > 0 ? sortedDraw[sortedDraw.length - 1] : 0,
        p50FrameIntervalMs: Math.round(this.percentile(sortedIntervals, 50) * 100) / 100,
        p95FrameIntervalMs: Math.round(this.percentile(sortedIntervals, 95) * 100) / 100,
        maxFrameIntervalMs: sortedIntervals.length > 0 ? sortedIntervals[sortedIntervals.length - 1] : 0,
        avgPrepareMs: sortedPrepare.length > 0 ? Math.round((sortedPrepare.reduce((a, b) => a + b, 0) / sortedPrepare.length) * 100) / 100 : 0,
        avgWasmDrawMs: sortedWasm.length > 0 ? Math.round((sortedWasm.reduce((a, b) => a + b, 0) / sortedWasm.length) * 100) / 100 : 0,
        avgBufferCopyMs: sortedBuf.length > 0 ? Math.round((sortedBuf.reduce((a, b) => a + b, 0) / sortedBuf.length) * 100) / 100 : 0,
        avgPutImageMs: sortedPut.length > 0 ? Math.round((sortedPut.reduce((a, b) => a + b, 0) / sortedPut.length) * 100) / 100 : 0,
      }
    },

    /**
     * Produce the final benchmark summary with pass/fail decision.
     *
     * @returns {object} Full summary including pass/fail.
     */
    finalSummary() {
      const initMs = initEndMs - initStartMs
      const benchmarkMs = benchmarkEndMs - benchmarkStartMs
      const avgFps = framesDrawn > 0 ? (framesDrawn * 1000) / benchmarkMs : 0

      const sortedDraw = [...drawTimes].sort((a, b) => a - b)
      const sortedIntervals = [...frameIntervals].sort((a, b) => a - b)

      const avg = (arr) => (arr.length > 0 ? Math.round((arr.reduce((a, b) => a + b, 0) / arr.length) * 100) / 100 : 0)

      const p95FrameInterval = this.percentile(sortedIntervals, 95)
      const avgFpsValue = Math.round(avgFps * 10) / 10

      const initOk = initMs > 0 && !crashed
      const durationOk = benchmarkMs >= BENCHMARK_DURATION_MS * 0.9
      const p95Ok = p95FrameInterval <= passP95FrameIntervalMs
      const fpsOk = avgFpsValue >= passAvgFps

      let pass = initOk && durationOk && p95Ok && fpsOk && !crashed
      let bottleneck = null

      if (crashed) {
        bottleneck = 'crash'
        pass = false
      } else if (!initOk) {
        bottleneck = 'initialization'
        pass = false
      } else if (!durationOk) {
        bottleneck = 'duration-incomplete'
        pass = false
      } else if (!p95Ok && !fpsOk) {
        bottleneck = 'frame-pacing-and-throughput'
        pass = false
      } else if (!p95Ok) {
        bottleneck = 'frame-pacing'
        pass = false
      } else if (!fpsOk) {
        bottleneck = 'throughput'
        pass = false
      }

      return {
        canvasWidth,
        canvasHeight,
        targetFps: safeTargetFps,
        benchmarkDurationMs: BENCHMARK_DURATION_MS,
        initMs: Math.round(initMs * 100) / 100,
        benchmarkMs: Math.round(benchmarkMs * 100) / 100,
        avgFps: avgFpsValue,
        totalFramesRequested,
        framesDrawn,
        droppedFrames,
        p50DrawMs: Math.round(this.percentile(sortedDraw, 50) * 100) / 100,
        p95DrawMs: Math.round(this.percentile(sortedDraw, 95) * 100) / 100,
        maxDrawMs: sortedDraw.length > 0 ? sortedDraw[sortedDraw.length - 1] : 0,
        p50FrameIntervalMs: Math.round(this.percentile(sortedIntervals, 50) * 100) / 100,
        p95FrameIntervalMs: Math.round(this.percentile(sortedIntervals, 95) * 100) / 100,
        maxFrameIntervalMs: sortedIntervals.length > 0 ? sortedIntervals[sortedIntervals.length - 1] : 0,
        avgPrepareMs: avg(prepareTimes),
        avgWasmDrawMs: avg(wasmDrawTimes),
        avgBufferCopyMs: avg(bufferCopyTimes),
        avgPutImageMs: avg(putImageTimes),
        userAgent: typeof navigator !== 'undefined' ? navigator.userAgent : 'unknown',
        rendererBackend: 'software-raster-rgba8888',
        timestamp: new Date().toISOString(),
        crashed,
        error: error ? String(error) : null,
        pass,
        bottleneck,
        passConditions: {
          initOk,
          durationOk,
          p95FrameIntervalOk: p95Ok,
          avgFpsOk: fpsOk,
          p95FrameIntervalThresholdMs: passP95FrameIntervalMs,
          avgFpsThreshold: passAvgFps,
        },
      }
    },
  }
}
