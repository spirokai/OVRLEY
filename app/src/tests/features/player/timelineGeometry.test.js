import { describe, expect, test } from 'vite-plus/test'
import {
  getClipGeometry,
  pointerToSecond,
  roundToDevicePixel,
  secondsToViewPx,
  snapClipDelta,
  videoIdFromLaneId,
} from '@/features/player/utils/timelineGeometry'

describe('timelineGeometry utilities', () => {
  test('maps pointer clientX to a clamped timeline second', () => {
    const rect = { left: 100, width: 400 }

    expect(pointerToSecond({ clientX: 300, rect, viewStart: 0, viewEnd: 100, widthPx: 500, totalDuration: 100 })).toBe(50)
    expect(pointerToSecond({ clientX: -50, rect: { left: 0 }, viewStart: 0, viewEnd: 100, widthPx: 500, totalDuration: 100 })).toBe(0)
    expect(pointerToSecond({ clientX: 999, rect: { left: 0 }, viewStart: 0, viewEnd: 100, widthPx: 500, totalDuration: 100 })).toBe(100)
  })

  test('maps pointers to a negative timeline minimum', () => {
    expect(
      pointerToSecond({
        clientX: 0,
        rect: { left: 0, width: 400 },
        viewStart: -5,
        viewEnd: 25,
        widthPx: 400,
        timelineMinimum: -5,
        totalDuration: 25,
      }),
    ).toBe(-5)
  })

  test('maps timeline seconds to viewport pixels', () => {
    expect(secondsToViewPx({ second: 50, viewStart: 0, viewEnd: 100, widthPx: 500 })).toBe(250)
    expect(secondsToViewPx({ second: 5, viewStart: 10, viewEnd: 10, widthPx: 500 })).toBe(0)
  })

  test('computes visible clip geometry', () => {
    expect(getClipGeometry({ startSecond: 10, durationSeconds: 20, viewStart: 0, viewEnd: 100, widthPx: 500 })).toEqual({
      isVisible: true,
      width: 100,
      x: 50,
    })
    expect(getClipGeometry({ startSecond: 110, durationSeconds: 20, viewStart: 0, viewEnd: 100, widthPx: 500 }).isVisible).toBe(false)
  })

  test('rounds to the active device pixel grid', () => {
    expect(roundToDevicePixel(10.26, 2)).toBe(10.5)
  })

  test('snaps clip edges using a screen-pixel threshold', () => {
    const viewport = { viewEnd: 100, viewStart: 0, widthPx: 500 }

    expect(snapClipDelta({ ...viewport, proposedDelta: 0.5, movingEdges: [0, 20], fixedEdges: [0, 100] })).toEqual({ delta: 0, guidelineSecond: 0 })
    // 1 px is 0.2 s: 2.5 s is 12.5 px, past the 10 px default.
    expect(snapClipDelta({ ...viewport, proposedDelta: 2.5, movingEdges: [0, 20], fixedEdges: [0, 100] })).toEqual({
      delta: 2.5,
      guidelineSecond: null,
    })
    expect(snapClipDelta({ ...viewport, proposedDelta: 2.5, movingEdges: [0, 20], fixedEdges: [0, 100], thresholdPx: 15 })).toEqual({
      delta: 0,
      guidelineSecond: 0,
    })
  })

  test('snaps whichever moving edge lands closest to a fixed edge', () => {
    expect(snapClipDelta({ proposedDelta: 9.6, movingEdges: [0, 20], fixedEdges: [0, 30, 10.2], viewEnd: 100, viewStart: 0, widthPx: 500 })).toEqual({
      delta: 10,
      guidelineSecond: 30,
    })
  })

  test('maps timeline lane ids to imported video ids', () => {
    expect(videoIdFromLaneId('activity')).toBeNull()
    expect(videoIdFromLaneId('video:clip-a')).toBe('clip-a')
    expect(videoIdFromLaneId('video:video:nested')).toBe('video:nested')
  })
})
