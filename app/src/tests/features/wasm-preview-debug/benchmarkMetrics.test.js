import { describe, expect, test } from 'vitest'

import { getPassAvgFpsThreshold, getPassP95FrameIntervalThresholdMs } from '@/features/wasm-preview-debug/benchmarkMetrics'

describe('benchmarkMetrics thresholds', () => {
  test('scales frame interval and fps thresholds with the effective target fps', () => {
    expect(getPassP95FrameIntervalThresholdMs(30)).toBe(40)
    expect(getPassP95FrameIntervalThresholdMs(60)).toBe(20)
    expect(getPassAvgFpsThreshold(30)).toBe(29)
    expect(getPassAvgFpsThreshold(60)).toBe(59)
  })
})
