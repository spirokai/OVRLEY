/**
 * Debug-only React mount for the generated Wasm preview renderer.
 * Includes a fixed-duration benchmark runner with live and final summaries.
 */

import { useCallback, useEffect, useRef, useState } from 'react'
import { AlertCircle, ArrowLeft, Play, Square } from 'lucide-react'

import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { getContainerFps } from '@/lib/update-rate'
import useStore from '@/store/useStore'
import { resolveBenchmarkFrameUpdate } from './benchmarkClock'
import { loadWasmFontFromBytes, renderDynamicTextWidgetSync } from './wasmPreviewRenderer'
import { createBenchmarkCollector, getPassAvgFpsThreshold, getPassP95FrameIntervalThresholdMs } from './benchmarkMetrics'

const BENCHMARK_DURATION_MS = 10_000
const BENCHMARK_WIDGET_COUNT = 6
const DEFAULT_TARGET_FPS = 30
const PREVIEW_CANVAS_WIDTH = 1920
const PREVIEW_CANVAS_HEIGHT = 1080
const PREVIEW_GRID_COLUMNS = 3
const PREVIEW_GRID_ROWS = 2
const PREVIEW_TILE_GAP = 48

// Sample speed data from debug activities for widget animation
const SPEED_DATA = [
  0.0, 0.5, 1.2, 2.1, 3.0, 4.2, 5.5, 6.8, 8.1, 9.5, 10.8, 12.1, 13.5, 14.8, 16.2, 17.5, 18.8, 20.1, 21.5, 22.8, 24.1, 25.5, 26.8, 28.1, 29.5, 30.8,
  32.1, 33.5, 34.8, 36.2, 37.5, 38.8, 40.1, 41.5, 42.8, 44.1, 45.5, 46.8, 48.1, 49.5, 50.8, 52.1, 53.5, 54.8, 56.2, 57.5, 58.8, 60.1, 61.5, 62.8,
  64.1, 65.5, 66.8, 68.1, 69.5, 70.8, 72.1, 73.5, 74.8, 76.2, 77.5, 78.8, 80.1, 81.5, 82.8, 84.1, 85.5, 86.8, 88.1, 89.5, 90.8, 92.1, 93.5, 94.8,
  96.2, 97.5, 98.8, 100.1, 101.5, 102.8, 104.1, 105.5, 106.8, 108.1, 109.5, 110.8, 112.1, 113.5, 114.8, 116.2, 117.5, 118.8, 120.1, 121.5, 122.8,
  124.1, 125.5, 126.8, 128.1, 129.5,
]

/**
 * Formats milliseconds to a compact display string.
 *
 * @param {number} ms - Milliseconds.
 * @returns {string} Formatted string.
 */
function fmtMs(ms) {
  if (ms < 1) return '<1'
  return ms < 100 ? ms.toFixed(1) : Math.round(ms).toString()
}

function fmtFps(fps) {
  return Number.isInteger(fps) ? fps.toString() : fps.toFixed(2)
}

function fmtCanvasSize(width, height) {
  return `${width} x ${height}`
}

function ensureBenchmarkSurfaceCanvases(surfaceCanvasesRef) {
  if (typeof document === 'undefined') {
    return []
  }

  for (let widgetIndex = 0; widgetIndex < BENCHMARK_WIDGET_COUNT; widgetIndex += 1) {
    if (surfaceCanvasesRef.current[widgetIndex]) {
      continue
    }

    const surfaceCanvas = document.createElement('canvas')
    surfaceCanvasesRef.current[widgetIndex] = surfaceCanvas
  }

  return surfaceCanvasesRef.current.slice(0, BENCHMARK_WIDGET_COUNT)
}

function drawCompositeWidgetGrid(targetCanvas, sourceCanvases) {
  if (!targetCanvas) {
    throw new Error('Benchmark canvas is not ready.')
  }

  if (targetCanvas.width !== PREVIEW_CANVAS_WIDTH) {
    targetCanvas.width = PREVIEW_CANVAS_WIDTH
  }
  if (targetCanvas.height !== PREVIEW_CANVAS_HEIGHT) {
    targetCanvas.height = PREVIEW_CANVAS_HEIGHT
  }

  const context = targetCanvas.getContext('2d')
  if (!context) {
    throw new Error('Browser did not provide a 2D canvas context.')
  }

  context.imageSmoothingEnabled = true
  context.imageSmoothingQuality = 'high'

  context.fillStyle = '#0c0e12'
  context.fillRect(0, 0, PREVIEW_CANVAS_WIDTH, PREVIEW_CANVAS_HEIGHT)

  const cellWidth = PREVIEW_CANVAS_WIDTH / PREVIEW_GRID_COLUMNS
  const cellHeight = PREVIEW_CANVAS_HEIGHT / PREVIEW_GRID_ROWS

  sourceCanvases.forEach((sourceCanvas, widgetIndex) => {
    const columnIndex = widgetIndex % PREVIEW_GRID_COLUMNS
    const rowIndex = Math.floor(widgetIndex / PREVIEW_GRID_COLUMNS)
    const cellX = columnIndex * cellWidth
    const cellY = rowIndex * cellHeight
    const innerX = cellX + PREVIEW_TILE_GAP
    const innerY = cellY + PREVIEW_TILE_GAP
    const innerWidth = cellWidth - PREVIEW_TILE_GAP * 2
    const innerHeight = cellHeight - PREVIEW_TILE_GAP * 2
    const sourceWidth = sourceCanvas?.width ?? 0
    const sourceHeight = sourceCanvas?.height ?? 0
    if (!sourceWidth || !sourceHeight) {
      return
    }

    const scale = Math.min(innerWidth / sourceWidth, innerHeight / sourceHeight)
    const drawWidth = sourceWidth * scale
    const drawHeight = sourceHeight * scale
    const drawX = innerX + (innerWidth - drawWidth) / 2
    const drawY = innerY + (innerHeight - drawHeight) / 2

    context.fillStyle = '#181c24'
    context.fillRect(cellX, cellY, cellWidth, cellHeight)
    context.drawImage(sourceCanvas, 0, 0, sourceWidth, sourceHeight, drawX, drawY, drawWidth, drawHeight)
  })
}

/**
 * Renders the debug Wasm preview surface with benchmark runner.
 *
 * @returns {JSX.Element} Rendered component output.
 */
export function WasmPreviewDebug() {
  const sceneFps = useStore((state) => state.config?.scene?.fps ?? DEFAULT_TARGET_FPS)
  const updateRate = useStore((state) => state.updateRate ?? 1)
  const canvasRef = useRef(null)
  const surfaceCanvasesRef = useRef([])
  const runIdRef = useRef(0)
  const frameLoopRef = useRef(null)
  const collectorRef = useRef(null)
  const [status, setStatus] = useState('idle')
  const [errorMessage, setErrorMessage] = useState('')
  const [liveMetrics, setLiveMetrics] = useState(null)
  const [finalSummary, setFinalSummary] = useState(null)
  const targetFps = Math.max(1, getContainerFps(sceneFps, updateRate) || DEFAULT_TARGET_FPS)
  const passP95FrameIntervalThresholdMs = getPassP95FrameIntervalThresholdMs(targetFps)
  const passAvgFpsThreshold = getPassAvgFpsThreshold(targetFps)

  const clearCanvas = useCallback(() => {
    const visibleCanvas = canvasRef.current
    const visibleContext = visibleCanvas?.getContext('2d')
    if (visibleCanvas && visibleContext) {
      visibleContext.clearRect(0, 0, visibleCanvas.width, visibleCanvas.height)
    }

    surfaceCanvasesRef.current.forEach((canvas) => {
      const context = canvas?.getContext('2d')
      if (canvas && context) {
        context.clearRect(0, 0, canvas.width, canvas.height)
      }
    })
  }, [])

  const loadFont = useCallback(async () => {
    const response = await fetch('/fonts/JetBrains Mono.ttf')
    if (!response.ok) {
      throw new Error(`Failed to fetch font: ${response.statusText}`)
    }
    const fontBytes = await response.arrayBuffer()
    await loadWasmFontFromBytes(fontBytes)
  }, [])

  const startBenchmark = useCallback(async () => {
    const benchmarkTargetFps = targetFps
    const frameDurationMs = 1000 / benchmarkTargetFps
    const runId = runIdRef.current + 1
    runIdRef.current = runId
    setStatus('loading')
    setErrorMessage('')
    setFinalSummary(null)
    setLiveMetrics(null)

    const collector = createBenchmarkCollector({
      canvasHeight: PREVIEW_CANVAS_HEIGHT,
      canvasWidth: PREVIEW_CANVAS_WIDTH,
      targetFps: benchmarkTargetFps,
    })
    collectorRef.current = collector

    try {
      // --- Initialization phase ---
      collector.markInitStart()
      await loadFont()
      collector.markInitEnd()

      if (runIdRef.current !== runId) return

      // --- Benchmark run phase ---
      const benchmarkStartMs = performance.now()
      collector.markBenchmarkStart(benchmarkStartMs)
      setStatus('benchmarking')

      const benchmarkCanvas = canvasRef.current
      const benchmarkCanvases = ensureBenchmarkSurfaceCanvases(surfaceCanvasesRef)
      if (!benchmarkCanvas || benchmarkCanvases.length !== BENCHMARK_WIDGET_COUNT || benchmarkCanvases.some((canvas) => !canvas)) {
        throw new Error('Benchmark canvas is not ready.')
      }

      const benchmarkEnd = benchmarkStartMs + BENCHMARK_DURATION_MS
      let lastLiveUpdate = 0
      let frameCount = 0
      let lastRenderedFrameIndex = -1

      const animate = (now) => {
        if (runIdRef.current !== runId) return

        const tIterationStart = performance.now()

        // Check if benchmark duration has elapsed
        if (now >= benchmarkEnd) {
          collector.markBenchmarkEnd()
          const summary = collector.finalSummary()
          setFinalSummary(summary)
          setStatus(summary.pass ? 'passed' : 'failed')
          setLiveMetrics(null)
          return
        }

        const frameUpdate = resolveBenchmarkFrameUpdate({
          nowMs: now,
          benchmarkStartMs,
          lastRenderedFrameIndex,
          targetFps: benchmarkTargetFps,
        })

        if (frameUpdate.shouldRender) {
          for (let droppedFrame = frameUpdate.frameIndex - frameUpdate.droppedFrames; droppedFrame < frameUpdate.frameIndex; droppedFrame += 1) {
            collector.recordDroppedFrame(benchmarkStartMs + droppedFrame * frameDurationMs)
          }

          // Measure draw time (synchronous — no alloc/dealloc churn)
          const drawStart = performance.now()
          const aggregateTiming = {
            bufferCopy: 0,
            prepare: 0,
            putImage: 0,
            total: 0,
            wasmDraw: 0,
          }
          try {
            benchmarkCanvases.forEach((surfaceCanvas, widgetIndex) => {
              const speed = SPEED_DATA[(frameUpdate.frameIndex + widgetIndex * 7) % SPEED_DATA.length]
              const renderResult = renderDynamicTextWidgetSync(surfaceCanvas, speed.toFixed(1), 'km/h')
              aggregateTiming.prepare += renderResult.timing.prepare
              aggregateTiming.wasmDraw += renderResult.timing.wasmDraw
              aggregateTiming.bufferCopy += renderResult.timing.bufferCopy
              aggregateTiming.putImage += renderResult.timing.putImage
              aggregateTiming.total += renderResult.timing.total
            })
            drawCompositeWidgetGrid(benchmarkCanvas, benchmarkCanvases)
          } catch (err) {
            collector.markCrashed(err)
            setErrorMessage(err instanceof Error ? err.message : 'Frame rendering failed')
            setStatus('failed')
            setFinalSummary(collector.finalSummary())
            return
          }
          const drawEnd = performance.now()

          if (runIdRef.current !== runId) return

          collector.recordFrameDraw(drawEnd - drawStart, aggregateTiming)
          collector.recordFrameInterval(now)
          lastRenderedFrameIndex = frameUpdate.frameIndex

          // Update live summary at ~2Hz
          if (now - lastLiveUpdate > 500) {
            setLiveMetrics(collector.liveSummary())
            lastLiveUpdate = now
          }

          frameCount++
          if (frameCount <= 5 || frameCount % 30 === 0) {
            console.info(
              `[wasm-bench] frame ${frameCount}: index=${frameUpdate.frameIndex} widgets=${BENCHMARK_WIDGET_COUNT} dropped=${frameUpdate.droppedFrames} draw=${(drawEnd - drawStart).toFixed(1)}ms elapsed=${(now - benchmarkStartMs).toFixed(1)}ms`,
            )
          }
        }

        const tIterationEnd = performance.now()
        if (frameCount <= 5 || frameCount % 30 === 0) {
          console.info(`[wasm-bench] iteration overhead: ${(tIterationEnd - tIterationStart).toFixed(1)}ms`)
        }

        frameLoopRef.current = requestAnimationFrame(animate)
      }

      frameLoopRef.current = requestAnimationFrame(animate)
    } catch (err) {
      if (runIdRef.current !== runId) return
      collector.markCrashed(err)
      setErrorMessage(err instanceof Error ? err.message : 'Wasm renderer failed to initialize.')
      setFinalSummary(collector.finalSummary())
      setStatus('failed')
    }
  }, [loadFont, targetFps])

  const handleStop = useCallback(() => {
    runIdRef.current += 1
    if (frameLoopRef.current) {
      cancelAnimationFrame(frameLoopRef.current)
      frameLoopRef.current = null
    }
    clearCanvas()
    setErrorMessage('')
    setLiveMetrics(null)
    setStatus('idle')
  }, [clearCanvas])

  useEffect(() => {
    return () => {
      runIdRef.current += 1
      if (frameLoopRef.current) {
        cancelAnimationFrame(frameLoopRef.current)
      }
    }
  }, [])

  const isLoading = status === 'loading'
  const isRunning = status === 'benchmarking'

  return (
    <div className="flex h-screen min-h-0 flex-col bg-background text-foreground">
      <header className="flex shrink-0 items-center justify-between border-b border-border/70 bg-card/80 px-4 py-3">
        <div className="flex min-w-0 items-center gap-3">
          <Button asChild variant="ghost" size="icon" aria-label="Back to editor">
            <a href="#">
              <ArrowLeft />
            </a>
          </Button>
          <div className="min-w-0">
            <h1 className="truncate text-base font-semibold tracking-normal text-foreground">Wasm Preview Benchmark</h1>
            <p className="truncate text-xs normal-case tracking-normal text-muted-foreground">
              {`${fmtCanvasSize(PREVIEW_CANVAS_WIDTH, PREVIEW_CANVAS_HEIGHT)}@${fmtFps(targetFps)}fps benchmark — ${BENCHMARK_WIDGET_COUNT} text widgets, ${BENCHMARK_DURATION_MS / 1000}s duration`}
            </p>
          </div>
        </div>
        <div className="flex shrink-0 items-center gap-2">
          <Badge variant={status === 'passed' ? 'default' : status === 'failed' ? 'destructive' : isRunning ? 'default' : 'secondary'}>
            {status}
          </Badge>
          <Button onClick={startBenchmark} disabled={isLoading || isRunning} size="sm">
            <Play />
            Run Benchmark
          </Button>
          <Button onClick={handleStop} disabled={!isRunning} variant="outline" size="sm">
            <Square />
            Stop
          </Button>
        </div>
      </header>

      <main className="grid min-h-0 flex-1 grid-cols-[minmax(0,1fr)_22rem] overflow-hidden">
        <section className="min-h-0 overflow-auto bg-surface-darken p-4">
          <div className="w-fit overflow-hidden border border-border/70 bg-black shadow-sm">
            <div className="border-b border-border/70 bg-card/40 px-3 py-2 text-xs normal-case tracking-normal text-muted-foreground">
              {BENCHMARK_WIDGET_COUNT} text widgets composited into one {fmtCanvasSize(PREVIEW_CANVAS_WIDTH, PREVIEW_CANVAS_HEIGHT)} canvas
            </div>
            <canvas ref={canvasRef} width="1920" height="1080" className="block h-auto w-[min(1920px,calc(100vw-25rem))] max-w-none" />
          </div>
        </section>

        <aside className="space-y-4 overflow-y-auto border-l border-border/70 bg-card/60 p-4">
          {/* --- Live metrics while benchmark is running --- */}
          {isRunning && liveMetrics ? (
            <div className="space-y-2">
              <h2 className="text-sm font-semibold tracking-normal text-foreground">Live Benchmark</h2>
              <div className="grid grid-cols-2 gap-2 text-xs normal-case tracking-normal">
                <span className="text-muted-foreground">Elapsed</span>
                <span className="text-right text-foreground font-mono">{fmtMs(liveMetrics.elapsedMs)}s</span>
                <span className="text-muted-foreground">Avg fps</span>
                <span className="text-right text-foreground font-mono">{liveMetrics.avgFps}</span>
                <span className="text-muted-foreground">Frames drawn</span>
                <span className="text-right text-foreground font-mono">{liveMetrics.framesDrawn}</span>
                <span className="text-muted-foreground">Dropped</span>
                <span className="text-right text-foreground font-mono">{liveMetrics.droppedFrames}</span>
                <span className="text-muted-foreground">Draw p50/p95</span>
                <span className="text-right text-foreground font-mono">
                  {fmtMs(liveMetrics.p50DrawMs)}/{fmtMs(liveMetrics.p95DrawMs)} ms
                </span>
                <span className="text-muted-foreground">Interval p50/p95</span>
                <span className="text-right text-foreground font-mono">
                  {fmtMs(liveMetrics.p50FrameIntervalMs)}/{fmtMs(liveMetrics.p95FrameIntervalMs)} ms
                </span>
                <span className="col-span-2 border-t border-border/50 pt-1 text-muted-foreground">Phase avg</span>
                <span className="text-muted-foreground">prepare</span>
                <span className="text-right text-foreground font-mono">{fmtMs(liveMetrics.avgPrepareMs)} ms</span>
                <span className="text-muted-foreground">wasm draw</span>
                <span className="text-right text-foreground font-mono">{fmtMs(liveMetrics.avgWasmDrawMs)} ms</span>
                <span className="text-muted-foreground">buffer copy</span>
                <span className="text-right text-foreground font-mono">{fmtMs(liveMetrics.avgBufferCopyMs)} ms</span>
                <span className="text-muted-foreground">putImageData</span>
                <span className="text-right text-foreground font-mono">{fmtMs(liveMetrics.avgPutImageMs)} ms</span>
              </div>
            </div>
          ) : null}

          {/* --- Final summary after benchmark completes --- */}
          {finalSummary ? (
            <div className="space-y-3">
              <div className="flex items-center gap-2">
                <h2 className="text-sm font-semibold tracking-normal text-foreground">Final Summary</h2>
                <Badge variant={finalSummary.pass ? 'default' : 'destructive'}>{finalSummary.pass ? 'PASS' : 'FAIL'}</Badge>
              </div>

              {finalSummary.bottleneck ? (
                <div className="rounded-md border border-destructive/50 bg-destructive/10 px-3 py-2 text-xs normal-case tracking-normal text-destructive">
                  Bottleneck: {finalSummary.bottleneck}
                </div>
              ) : null}

              <div className="space-y-2">
                <h3 className="text-xs font-semibold tracking-normal text-muted-foreground uppercase">Setup</h3>
                <div className="grid grid-cols-2 gap-2 text-xs normal-case tracking-normal">
                  <span className="text-muted-foreground">Init time</span>
                  <span className="text-right text-foreground font-mono">{fmtMs(finalSummary.initMs)} ms</span>
                  <span className="text-muted-foreground">Benchmark run</span>
                  <span className="text-right text-foreground font-mono">{fmtMs(finalSummary.benchmarkMs)} ms</span>
                  <span className="text-muted-foreground">Canvas</span>
                  <span className="text-right text-foreground">
                    {finalSummary.canvasWidth} x {finalSummary.canvasHeight}
                  </span>
                  <span className="text-muted-foreground">Widgets</span>
                  <span className="text-right text-foreground">{BENCHMARK_WIDGET_COUNT}</span>
                  <span className="text-muted-foreground">Backend</span>
                  <span className="text-right text-foreground">{finalSummary.rendererBackend}</span>
                </div>
              </div>

              <div className="space-y-2">
                <h3 className="text-xs font-semibold tracking-normal text-muted-foreground uppercase">Frame Metrics</h3>
                <div className="grid grid-cols-2 gap-2 text-xs normal-case tracking-normal">
                  <span className="text-muted-foreground">Avg fps</span>
                  <span className="text-right text-foreground font-mono">{finalSummary.avgFps}</span>
                  <span className="text-muted-foreground">Requested</span>
                  <span className="text-right text-foreground font-mono">{finalSummary.totalFramesRequested}</span>
                  <span className="text-muted-foreground">Drawn</span>
                  <span className="text-right text-foreground font-mono">{finalSummary.framesDrawn}</span>
                  <span className="text-muted-foreground">Dropped</span>
                  <span className="text-right text-foreground font-mono">{finalSummary.droppedFrames}</span>
                </div>
              </div>

              <div className="space-y-2">
                <h3 className="text-xs font-semibold tracking-normal text-muted-foreground uppercase">Draw Time</h3>
                <div className="grid grid-cols-2 gap-2 text-xs normal-case tracking-normal">
                  <span className="text-muted-foreground">p50</span>
                  <span className="text-right text-foreground font-mono">{fmtMs(finalSummary.p50DrawMs)} ms</span>
                  <span className="text-muted-foreground">p95</span>
                  <span className="text-right text-foreground font-mono">{fmtMs(finalSummary.p95DrawMs)} ms</span>
                  <span className="text-muted-foreground">max</span>
                  <span className="text-right text-foreground font-mono">{fmtMs(finalSummary.maxDrawMs)} ms</span>
                </div>
              </div>

              <div className="space-y-2">
                <h3 className="text-xs font-semibold tracking-normal text-muted-foreground uppercase">Phase Breakdown (avg)</h3>
                <div className="grid grid-cols-2 gap-2 text-xs normal-case tracking-normal">
                  <span className="text-muted-foreground">prepare</span>
                  <span className="text-right text-foreground font-mono">{fmtMs(finalSummary.avgPrepareMs)} ms</span>
                  <span className="text-muted-foreground">wasm draw</span>
                  <span className="text-right text-foreground font-mono">{fmtMs(finalSummary.avgWasmDrawMs)} ms</span>
                  <span className="text-muted-foreground">buffer copy</span>
                  <span className="text-right text-foreground font-mono">{fmtMs(finalSummary.avgBufferCopyMs)} ms</span>
                  <span className="text-muted-foreground">putImageData</span>
                  <span className="text-right text-foreground font-mono">{fmtMs(finalSummary.avgPutImageMs)} ms</span>
                </div>
              </div>

              <div className="space-y-2">
                <h3 className="text-xs font-semibold tracking-normal text-muted-foreground uppercase">Frame Interval</h3>
                <div className="grid grid-cols-2 gap-2 text-xs normal-case tracking-normal">
                  <span className="text-muted-foreground">p50</span>
                  <span className="text-right text-foreground font-mono">{fmtMs(finalSummary.p50FrameIntervalMs)} ms</span>
                  <span className="text-muted-foreground">p95</span>
                  <span className="text-right text-foreground font-mono">
                    {fmtMs(finalSummary.p95FrameIntervalMs)} ms
                    {finalSummary.passConditions.p95FrameIntervalOk ? ' ✓' : ' ✗'}
                  </span>
                  <span className="text-muted-foreground">max</span>
                  <span className="text-right text-foreground font-mono">{fmtMs(finalSummary.maxFrameIntervalMs)} ms</span>
                </div>
              </div>

              <div className="space-y-2">
                <h3 className="text-xs font-semibold tracking-normal text-muted-foreground uppercase">Pass Conditions</h3>
                <div className="grid grid-cols-2 gap-2 text-xs normal-case tracking-normal">
                  <span className="text-muted-foreground">Init ok</span>
                  <span className="text-right text-foreground">{finalSummary.passConditions.initOk ? '✓' : '✗'}</span>
                  <span className="text-muted-foreground">Duration ok</span>
                  <span className="text-right text-foreground">{finalSummary.passConditions.durationOk ? '✓' : '✗'}</span>
                  <span className="text-muted-foreground">p95 ≤ {fmtMs(finalSummary.passConditions.p95FrameIntervalThresholdMs)}ms</span>
                  <span className="text-right text-foreground">{finalSummary.passConditions.p95FrameIntervalOk ? '✓' : '✗'}</span>
                  <span className="text-muted-foreground">fps ≥ {fmtFps(finalSummary.passConditions.avgFpsThreshold)}</span>
                  <span className="text-right text-foreground">{finalSummary.passConditions.avgFpsOk ? '✓' : '✗'}</span>
                </div>
              </div>
            </div>
          ) : null}

          {/* --- Static info when idle --- */}
          {!isRunning && !finalSummary ? (
            <div className="space-y-2">
              <h2 className="text-sm font-semibold tracking-normal text-foreground">Benchmark Config</h2>
              <div className="grid grid-cols-2 gap-2 text-xs normal-case tracking-normal">
                <span className="text-muted-foreground">Canvas</span>
                <span className="text-right text-foreground">{fmtCanvasSize(PREVIEW_CANVAS_WIDTH, PREVIEW_CANVAS_HEIGHT)}</span>
                <span className="text-muted-foreground">Widgets</span>
                <span className="text-right text-foreground">{BENCHMARK_WIDGET_COUNT}</span>
                <span className="text-muted-foreground">Target fps</span>
                <span className="text-right text-foreground">{fmtFps(targetFps)}</span>
                <span className="text-muted-foreground">Duration</span>
                <span className="text-right text-foreground">{BENCHMARK_DURATION_MS / 1000}s</span>
                <span className="text-muted-foreground">Pass p95</span>
                <span className="text-right text-foreground">≤ {fmtMs(passP95FrameIntervalThresholdMs)} ms</span>
                <span className="text-muted-foreground">Pass fps</span>
                <span className="text-right text-foreground">≥ {fmtFps(passAvgFpsThreshold)}</span>
              </div>
            </div>
          ) : null}

          {errorMessage ? (
            <Alert variant="destructive">
              <AlertCircle />
              <AlertTitle>Wasm preview failed</AlertTitle>
              <AlertDescription className="normal-case tracking-normal">{errorMessage}</AlertDescription>
            </Alert>
          ) : null}
        </aside>
      </main>
    </div>
  )
}
