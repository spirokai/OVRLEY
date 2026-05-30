/**
 * Direct-import tests for gap detection and series building utilities.
 *
 * These tests import helpers directly from parse-helpers.js rather than
 * using the old parser.js createActivityHelpers() bundle. The gap-utils
 * functions now import helpers directly themselves.
 */

import { describe, expect, test } from 'vitest'
import { buildDistanceSeries, buildElapsedSeries, insertIdleGapSamples } from '@/lib/activity/gap-utils'

describe('insertIdleGapSamples', () => {
  test('does not insert idle samples when there are fewer than 2 samples', () => {
    const samples = [{ timestamp: '2024-01-01T00:00:00Z', elapsedSeconds: 0, speed: 10 }]
    const result = insertIdleGapSamples(samples)
    expect(result.rawSamples).toHaveLength(1)
    expect(result.gapDebug.inserted_sample_count).toBe(0)
  })

  test('fills idle samples for large time gaps with minimal movement', () => {
    const samples = [
      { timestamp: '2024-01-01T00:00:00Z', elapsedSeconds: 0, speed: 10, distance: 0, latitude: 50.0, longitude: 14.0 },
      { timestamp: '2024-01-01T00:00:30Z', elapsedSeconds: 30, speed: 0, distance: 0, latitude: 50.0, longitude: 14.0 },
    ]
    const result = insertIdleGapSamples(samples)
    expect(result.rawSamples.length).toBeGreaterThan(2)
    expect(result.gapDebug.inserted_sample_count).toBeGreaterThan(0)
  })

  test('does not insert idle samples when there is significant movement', () => {
    const samples = [
      { timestamp: '2024-01-01T00:00:00Z', elapsedSeconds: 0, speed: 20, distance: 0, latitude: 50.0, longitude: 14.0 },
      { timestamp: '2024-01-01T00:00:30Z', elapsedSeconds: 30, speed: 20, distance: 200, latitude: 50.1, longitude: 14.1 },
    ]
    const result = insertIdleGapSamples(samples)
    expect(result.gapDebug.inserted_sample_count).toBe(0)
  })

  test('idle samples have zero values for speed, cadence, power, HR', () => {
    const samples = [
      {
        timestamp: '2024-01-01T00:00:00Z',
        elapsedSeconds: 0,
        speed: 10,
        cadence: 80,
        power: 200,
        heartrate: 150,
        distance: 0,
        latitude: 50.0,
        longitude: 14.0,
      },
      {
        timestamp: '2024-01-01T00:00:30Z',
        elapsedSeconds: 30,
        speed: 0,
        cadence: 0,
        power: 0,
        heartrate: 0,
        distance: 0,
        latitude: 50.0,
        longitude: 14.0,
      },
    ]
    const result = insertIdleGapSamples(samples)
    const idleSamples = result.rawSamples.filter((s) => s.syntheticIdle)
    expect(idleSamples.length).toBeGreaterThan(0)
    idleSamples.forEach((s) => {
      expect(s.speed).toBe(0)
      expect(s.cadence).toBe(0)
      expect(s.power).toBe(0)
      expect(s.syntheticIdle).toBe(true)
    })
  })
})

describe('buildElapsedSeries', () => {
  test('falls back to index-based series when no timestamps or elapsed available', () => {
    const rawSamples = [{}, {}, {}]
    const timeSeries = [null, null, null]
    const result = buildElapsedSeries(rawSamples, timeSeries)
    expect(result[0]).toBe(0)
    expect(result).toHaveLength(3)
  })

  test('builds monotonic elapsed series from explicit elapsedSeconds', () => {
    const rawSamples = [{ elapsedSeconds: 0 }, { elapsedSeconds: 5 }, { elapsedSeconds: 10 }]
    const timeSeries = [null, null, null]
    const result = buildElapsedSeries(rawSamples, timeSeries)
    expect(result[0]).toBe(0)
    expect(result[2]).toBe(10)
  })

  test('builds elapsed series from timestamps when no explicit elapsed is available', () => {
    const rawSamples = [{}, {}, {}]
    const timeSeries = ['2024-01-01T00:00:00Z', '2024-01-01T00:00:05Z', '2024-01-01T00:00:10Z']
    const result = buildElapsedSeries(rawSamples, timeSeries)
    expect(result[0]).toBe(0)
    expect(result[1]).toBe(5)
    expect(result[2]).toBe(10)
  })

  test('handles samples with gaps in explicit elapsedSeconds', () => {
    const rawSamples = [{ elapsedSeconds: 0 }, { elapsedSeconds: null }, { elapsedSeconds: 10 }]
    const timeSeries = [null, null, null]
    const result = buildElapsedSeries(rawSamples, timeSeries)
    expect(result[0]).toBe(0)
    expect(result[1]).toBe(0)
    expect(result[2]).toBe(10)
  })
})

describe('buildDistanceSeries', () => {
  test('builds cumulative distance from course points', () => {
    const coursePoints = [
      [50.0, 14.0],
      [50.001, 14.0],
      [50.002, 14.0],
    ]
    const directDistanceSeries = [0, null, null]
    const result = buildDistanceSeries(coursePoints, directDistanceSeries)
    expect(result).toHaveLength(3)
    expect(result[0]).toBe(0)
    expect(result[result.length - 1]).toBeGreaterThan(0)
  })

  test('uses direct distance when available', () => {
    const coursePoints = [
      [50.0, 14.0],
      [50.001, 14.0],
    ]
    const directDistanceSeries = [0, 111]
    const result = buildDistanceSeries(coursePoints, directDistanceSeries)
    expect(result[0]).toBe(0)
    expect(result[1]).toBe(111)
  })

  test('handles empty course points', () => {
    const result = buildDistanceSeries([], [])
    expect(result).toEqual([])
  })

  test('maintains monotonic distance', () => {
    const coursePoints = [
      [50.0, 14.0],
      [50.001, 14.0],
      [50.002, 14.0],
    ]
    const directDistanceSeries = [0, null, 50]
    const result = buildDistanceSeries(coursePoints, directDistanceSeries)
    expect(result[0]).toBe(0)
    expect(result[2]).toBeGreaterThanOrEqual(result[0])
  })
})
