import { describe, expect, test } from 'vitest'

import { buildPreviewFrameWindow, resolveActivityDuration, resolvePreviewSecond } from '@/lib/preview-timing'

describe('preview timing helpers', () => {
  test('resolveActivityDuration prefers trim_end_seconds and falls back to dummy duration', () => {
    expect(resolveActivityDuration({ sourceActivity: { trim_end_seconds: 42 } })).toBe(42)
    expect(resolveActivityDuration({ sourceActivity: { metadata: { duration_seconds: 18 } } })).toBe(18)
    expect(resolveActivityDuration({ fallbackDurationSeconds: 9, sourceActivity: null })).toBe(9)
  })

  test('resolvePreviewSecond clamps the selected second to the activity duration', () => {
    expect(resolvePreviewSecond({ fallbackDurationSeconds: 10, selectedSecond: 12, sourceActivity: null })).toBe(10)
    expect(resolvePreviewSecond({ fallbackDurationSeconds: 10, selectedSecond: -2, sourceActivity: null })).toBe(0)
    expect(resolvePreviewSecond({ selectedSecond: 4.25, sourceActivity: { trim_end_seconds: 12 } })).toBe(4.25)
  })

  test('buildPreviewFrameWindow creates a valid one-frame render window around the preview second', () => {
    expect(buildPreviewFrameWindow({ activityDuration: 12, previewSecond: 3.5, sceneFps: 30 })).toEqual({
      start: 3.5,
      end: 3.5 + 1 / 30,
    })
  })

  test('buildPreviewFrameWindow clamps near the activity end', () => {
    expect(buildPreviewFrameWindow({ activityDuration: 5, previewSecond: 5, sceneFps: 20 })).toEqual({
      start: 4.95,
      end: 5,
    })
  })
})
