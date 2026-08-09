/**
 * Tests for the shared FPS resolver used by playback engine and video
 * playback clock. Covers the same behavior previously tested through
 * getEffectivePreviewFps before it was inlined.
 *
 * Also tests interpolated activity value retrieval with linear, hold, and
 * preserve interpolation modes.
 */

import { describe, expect, test } from 'vitest'
import { getContainerFps } from '@/lib/update-rate'
import { getInterpolatedActivityValue, getInterpolatedTimeValue, getMetricSeries } from '@/features/overlay-editor'
import { getPreviewActivity } from '@/features/overlay-editor/utils/overlayEditorUtils'

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

describe('getMetricSeries', () => {
  test('uses manifest data sources and prefers barometric altitude when present', () => {
    const barometricAltitude = [null, 101]
    const elevation = [10, 20]
    const activity = {
      barometric_altitude: barometricAltitude,
      elevation,
      distance: [0, 5],
    }

    expect(getMetricSeries(activity, 'altitude')).toBe(barometricAltitude)
    expect(getMetricSeries(activity, 'distance')).toBe(activity.distance)
    expect(getMetricSeries({ elevation }, 'altitude')).toBe(elevation)
  })
})

describe('getInterpolatedActivityValue — hold interpolation', () => {
  const baseActivity = {
    trim_end_seconds: 4,
    sample_elapsed_seconds: [0, 1, 2, 3, 4],
    iso: [100, 200, 400, 800, 1600],
    elevation: [10, 20, 30, 40, 50],
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

  test('activity-backed values use the default before the activity starts', () => {
    expect(getInterpolatedActivityValue(getPreviewActivity(baseActivity, -1), 'iso', -1)).toBeNull()
    expect(
      getInterpolatedActivityValue(
        {
          trim_end_seconds: 0.5,
          sample_elapsed_seconds: [0.110097, 0.5],
          iso: [100, 200],
        },
        'iso',
        0,
      ),
    ).toBe(100)
  })

  test('activity-backed values use the default after the activity ends', () => {
    expect(getInterpolatedActivityValue(getPreviewActivity(baseActivity, 5), 'iso', 5)).toBeNull()
  })

  test('hold metric returns null when series missing', () => {
    // speed is not in baseActivity, no fallback configured
    expect(getInterpolatedActivityValue(baseActivity, 'speed', 1)).toBeNull()
  })

  test('hold metric returns null when activity is null', () => {
    expect(getInterpolatedActivityValue(null, 'iso', 1)).toBeNull()
  })

  test('hold metric returns null when series key is missing from activity', () => {
    const emptyActivity = { trim_end_seconds: 1, sample_elapsed_seconds: [0, 1] }
    expect(getInterpolatedActivityValue(emptyActivity, 'iso', 1)).toBeNull()
  })

  test('preserve metric with an unavailable empty series remains missing', () => {
    expect(
      getInterpolatedActivityValue(
        {
          trim_end_seconds: 1,
          sample_elapsed_seconds: [0, 1],
          cadence: [],
        },
        'cadence',
        0.5,
      ),
    ).toBeNull()
  })

  test('preserve metric displays an exact missing sample as zero', () => {
    expect(
      getInterpolatedActivityValue(
        {
          trim_end_seconds: 2,
          sample_elapsed_seconds: [0, 1, 2],
          torque: [10, null, 30],
        },
        'torque',
        1,
      ),
    ).toBe(0)
  })

  test('hold metric with sparse data returns last known value skipping nulls', () => {
    const sparseActivity = {
      trim_end_seconds: 4,
      sample_elapsed_seconds: [0, 1, 2, 3, 4],
      iso: [100, null, null, 800, null],
    }
    expect(getInterpolatedActivityValue(sparseActivity, 'iso', 1.5)).toBe(100)
    expect(getInterpolatedActivityValue(sparseActivity, 'iso', 3.5)).toBe(800)
  })
})

describe('getInterpolatedTimeValue', () => {
  test('exposes activity only for elapsed seconds inside its preview range', () => {
    const activity = { trim_end_seconds: 4 }

    expect(getPreviewActivity(activity, -1)).toBeNull()
    expect(getPreviewActivity(activity, 4)).toBe(activity)
    expect(getPreviewActivity(activity, 5)).toBeNull()
  })

  test('uses the source time series before sync_time', () => {
    const activity = {
      trim_end_seconds: 60,
      sample_elapsed_seconds: [0, 60],
      sync_time: '2026-07-18T08:20:03.000Z',
      time: ['2026-07-18T07:20:03.000Z', '2026-07-18T07:21:03.000Z'],
    }

    expect(getInterpolatedTimeValue(activity, 60)).toBe('2026-07-18T07:21:03.000Z')
  })
})
