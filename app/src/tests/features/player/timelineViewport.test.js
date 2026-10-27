import { describe, expect, test } from 'vite-plus/test'
import {
  buildFitTargets,
  clampToView,
  computeTimelineTickStrip,
  fitRangeToViewport,
  fitToFull,
  followPlayhead,
  getMatchingFitTargetId,
  getTimelineTickStep,
  panViewport,
  rangesMatch,
  zoomRange,
} from '@/features/player/utils/timelineViewport'

describe('timelineViewport utilities', () => {
  test('clamps and fits viewports', () => {
    expect(clampToView(150, 250, 200)).toEqual({ viewStart: 100, viewEnd: 200 })
    expect(fitToFull(2.509)).toEqual({ viewStart: 0, viewEnd: 2.509 })
  })

  test('keeps negative video lead-in inside the viewport bounds', () => {
    expect(fitToFull(25, -5)).toEqual({ viewStart: -5, viewEnd: 25 })
    expect(clampToView(-20, 0, 25, -5)).toEqual({ viewStart: -5, viewEnd: 15 })
    expect(
      buildFitTargets({
        activityDurationSeconds: 25,
        fallbackDurationSeconds: 25,
        hasActivityData: true,
        totalDuration: 25,
        videoBounds: { startSecond: -5, endSecond: 25 },
      })[0].viewport,
    ).toEqual({ viewStart: -5, viewEnd: 25 })
  })

  test('zooms around a pivot and clamps to the duration', () => {
    const result = zoomRange({ direction: 1, pivot: 50, totalDuration: 200, viewEnd: 100, viewStart: 0 })

    expect(result.viewEnd - result.viewStart).toBeCloseTo(100 / 1.6)
    expect(result.viewStart).toBeLessThanOrEqual(50)
    expect(result.viewEnd).toBeGreaterThanOrEqual(50)
  })

  test('fits target ranges with padding and a minimum span', () => {
    const result = fitRangeToViewport({ rangeStart: 10, rangeEnd: 90, totalDuration: 100 })

    expect(result.viewStart).toBeCloseTo(6.8)
    expect(result.viewEnd).toBeCloseTo(93.2)
    expect(fitRangeToViewport({ rangeStart: 50, rangeEnd: 50, totalDuration: 100 }).viewEnd).toBeGreaterThan(50)
  })

  test('pans and follows the playhead inside bounds', () => {
    expect(panViewport({ deltaSeconds: 10, totalDuration: 200, viewEnd: 50, viewStart: 0 })).toEqual({ viewStart: 10, viewEnd: 60 })

    const followed = followPlayhead({ playheadSecond: 100, totalDuration: 200, viewEnd: 50, viewStart: 0 })
    expect(followed.viewStart).toBeCloseTo(100 - 0.85 * 50)
    expect(followed.viewEnd).toBeCloseTo(100 - 0.85 * 50 + 50)
  })

  test('follows before the playhead exits the right edge', () => {
    const viewport = followPlayhead({ playheadSecond: 45, totalDuration: 200, viewEnd: 50, viewStart: 0 })

    expect(viewport.viewStart).toBeCloseTo(45 - 0.85 * 50)
    expect(viewport.viewEnd - viewport.viewStart).toBeCloseTo(50)
  })

  test('builds and matches canonical fit targets', () => {
    const targets = buildFitTargets({
      activityDurationSeconds: 45,
      fallbackDurationSeconds: 73,
      hasActivityData: true,
      totalDuration: 100,
      videoBounds: { startSecond: 10, endSecond: 30 },
    })

    expect(targets.map((target) => target.id)).toEqual(['all', 'video', 'activity'])
    expect(getMatchingFitTargetId({ targets, viewport: targets[1].viewport })).toBe('video')
    expect(rangesMatch({ viewStart: 0, viewEnd: 100 }, { viewStart: 0, viewEnd: 100.0005 })).toBe(true)
  })

  test('fits the video target to the bounds spanning every imported clip', () => {
    const targets = buildFitTargets({
      activityDurationSeconds: undefined,
      fallbackDurationSeconds: 73,
      hasActivityData: false,
      totalDuration: 40,
      videoBounds: { startSecond: 0, endSecond: 40 },
    })

    expect(targets.map((target) => target.id)).toEqual(['video'])
    expect(targets[0].viewport).toEqual({ viewStart: 0, viewEnd: 40 })
  })

  test('computes a major and minor tick strip from its anchor', () => {
    const step = getTimelineTickStep(9)
    const ticks = computeTimelineTickStrip({ anchorSecond: 20, step, pxPerSecond: 9, widthPx: 900 })

    expect(step * 9).toBeGreaterThanOrEqual(90)
    expect(ticks.major[0]).toMatchObject({ second: 20, x: 0 })
    expect(ticks.major[1].x).toBe(step * 9)
    expect(ticks.major.at(-1).x).toBeGreaterThanOrEqual(900 + step * 9)
    expect(ticks.major[0].label).toBeTruthy()
    expect(ticks.minor).toHaveLength((ticks.major.length - 1) * 4)
    expect(ticks.minor[0].label).toBeUndefined()
  })
})
