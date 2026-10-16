import { describe, expect, test } from 'vite-plus/test'

import { buildPreviewFrameWindow, resolveActivityDuration } from '@/lib/preview-timing'

describe('preview timing helpers', () => {
  test('resolveActivityDuration prefers trim_end_seconds and falls back to dummy duration', () => {
    expect(resolveActivityDuration({ sourceActivity: { trim_end_seconds: 42 } })).toBe(42)
    expect(resolveActivityDuration({ sourceActivity: { metadata: { duration_seconds: 18 } } })).toBe(18)
    expect(resolveActivityDuration({ fallbackDurationSeconds: 9, sourceActivity: null })).toBe(9)
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
