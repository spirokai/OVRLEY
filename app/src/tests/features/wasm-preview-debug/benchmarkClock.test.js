import { describe, expect, test } from 'vitest'

import { resolveBenchmarkFrameUpdate } from '@/features/wasm-preview-debug/benchmarkClock'

describe('resolveBenchmarkFrameUpdate', () => {
  test('renders on frame-index changes instead of every-third-tick interval gating', () => {
    const rafTimes = [16.6667, 33.3333, 50, 66.6667, 83.3333, 100, 116.6667, 133.3333, 150]
    let lastRenderedFrameIndex = -1
    const renderedFrames = []

    rafTimes.forEach((nowMs) => {
      const update = resolveBenchmarkFrameUpdate({
        nowMs,
        benchmarkStartMs: 0,
        lastRenderedFrameIndex,
        targetFps: 30,
      })

      if (!update.shouldRender) {
        return
      }

      renderedFrames.push({
        frameIndex: update.frameIndex,
        nowMs,
      })
      lastRenderedFrameIndex = update.frameIndex
    })

    expect(renderedFrames).toEqual([
      { frameIndex: 0, nowMs: 16.6667 },
      { frameIndex: 1, nowMs: 33.3333 },
      { frameIndex: 2, nowMs: 66.6667 },
      { frameIndex: 3, nowMs: 100 },
      { frameIndex: 4, nowMs: 133.3333 },
    ])
  })

  test('counts skipped frame indices as dropped frames', () => {
    expect(
      resolveBenchmarkFrameUpdate({
        nowMs: 200,
        benchmarkStartMs: 0,
        lastRenderedFrameIndex: 2,
        targetFps: 30,
      }),
    ).toEqual({
      droppedFrames: 3,
      frameIndex: 6,
      shouldRender: true,
    })
  })

  test('does not request a render when the scheduler is still within the same frame index', () => {
    expect(
      resolveBenchmarkFrameUpdate({
        nowMs: 20,
        benchmarkStartMs: 0,
        lastRenderedFrameIndex: 0,
        targetFps: 30,
      }),
    ).toEqual({
      droppedFrames: 0,
      frameIndex: 0,
      shouldRender: false,
    })
  })
})
