/**
 * Tests for the shared FPS resolver used by playback engine and video
 * playback clock. Covers the same behavior previously tested through
 * getEffectivePreviewFps before it was inlined.
 *
 * Also tests interpolated activity value retrieval with linear and hold
 * interpolation modes.
 */

import { describe, expect, test } from 'vitest'
import { getContainerFps } from '@/lib/update-rate'
import { getInterpolatedActivityValue } from '@/features/overlay-editor'

describe('getContainerFps', () => {
  test('returns a number for common FPS and update rate combinations', () => {
    expect(typeof getContainerFps(30, 1)).toBe('number')
    expect(getContainerFps(30, 1)).toBeGreaterThan(0)
  })

  test('handles 60 FPS with update rate 2', () => {
    const result = getContainerFps(60, 2)
    expect(typeof result).toBe('number')
    expect(result).toBeGreaterThan(0)
  })

  test('handles 24 FPS with update rate 1', () => {
    const result = getContainerFps(24, 1)
    expect(typeof result).toBe('number')
    expect(result).toBeGreaterThan(0)
  })

  test('handles edge case with FPS 0', () => {
    const result = getContainerFps(0, 1)
    expect(typeof result).toBe('number')
  })
})

describe('getInterpolatedActivityValue — hold interpolation', () => {
  const baseActivity = {
    sample_elapsed_seconds: [0, 1, 2, 3, 4],
    iso: [100, 200, 400, 800, 1600],
    altitude: [10, 20, 30, 40, 50],
  }

  test('hold metric returns last known value at or before elapsedSecond, not interpolated', () => {
    // iso has hold interpolation in the manifest
    expect(getInterpolatedActivityValue(baseActivity, 'iso', 2.5)).toBe(400)
    expect(getInterpolatedActivityValue(baseActivity, 'iso', 1.2)).toBe(200)
    expect(getInterpolatedActivityValue(baseActivity, 'iso', 0.0)).toBe(100)
    expect(getInterpolatedActivityValue(baseActivity, 'iso', 3.9)).toBe(800)
    expect(getInterpolatedActivityValue(baseActivity, 'iso', 4.0)).toBe(1600)
  })

  test('linear metric still interpolates between samples', () => {
    // altitude has linear interpolation in the manifest
    expect(getInterpolatedActivityValue(baseActivity, 'altitude', 2.5)).toBe(35)
    expect(getInterpolatedActivityValue(baseActivity, 'altitude', 1.2)).toBe(22)
  })

  test('hold metric returns null for elapsedSecond before first sample', () => {
    expect(getInterpolatedActivityValue(baseActivity, 'iso', -1)).toBe(null)
  })

  test('hold metric returns last sample for elapsedSecond beyond last sample', () => {
    expect(getInterpolatedActivityValue(baseActivity, 'iso', 5)).toBe(1600)
  })

  test('hold metric falls back to DEFAULT_ACTIVITY_PREVIEW when series missing', () => {
    // speed is not in baseActivity, falls back to preview default
    expect(getInterpolatedActivityValue(baseActivity, 'speed', 1)).toBe(8.4)
  })

  test('hold metric falls back to DEFAULT_ACTIVITY_PREVIEW when activity is null', () => {
    expect(getInterpolatedActivityValue(null, 'iso', 1)).toBe(400)
  })

  test('hold metric falls back to DEFAULT_ACTIVITY_PREVIEW when series key is missing from activity', () => {
    const emptyActivity = { sample_elapsed_seconds: [0, 1] }
    expect(getInterpolatedActivityValue(emptyActivity, 'iso', 1)).toBe(400)
  })

  test('hold metric with sparse data returns last known value skipping nulls', () => {
    const sparseActivity = {
      sample_elapsed_seconds: [0, 1, 2, 3, 4],
      iso: [100, null, null, 800, null],
    }
    expect(getInterpolatedActivityValue(sparseActivity, 'iso', 1.5)).toBe(100)
    expect(getInterpolatedActivityValue(sparseActivity, 'iso', 3.5)).toBe(800)
  })
})
