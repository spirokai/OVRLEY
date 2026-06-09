import { describe, expect, test } from 'vitest'

import { deriveActivityMetricSeries } from '@/lib/activity/metric-series'
import { calculateBearingDegrees } from '@/lib/activity/parse-helpers'

function makeHelpers(overrides = {}) {
  return {
    calculateBearingDegrees: () => null,
    haversineDistanceMeters: () => 0,
    isFiniteNumber: (v) => typeof v === 'number' && Number.isFinite(v),
    roundValue: (v) => v,
    safeNumber: (v) => (v === null || v === undefined ? null : Number(v)),
    safeTimestamp: (v) => v,
    ...overrides,
  }
}

function makeNormalizedSamples(values) {
  const fields = [
    'airPressure',
    'altitude',
    'cadence',
    'coreTemperature',
    'distance',
    'elevation',
    'gForce',
    'gearPosition',
    'gradient',
    'groundContactTime',
    'heading',
    'heartrate',
    'latitude',
    'leftRightBalance',
    'longitude',
    'pace',
    'power',
    'speed',
    'strideLength',
    'strokeRate',
    'temperature',
    'torque',
    'verticalOscillation',
    'verticalSpeed',
  ]
  return values.map((row) => {
    const sample = { elapsedSeconds: 0, timestamp: null, syntheticIdle: false }
    fields.forEach((f) => {
      sample[f] = row[f] ?? null
    })
    return sample
  })
}

describe('combineSeries and combineSeriesPreferDerived (via deriveActivityMetricSeries)', () => {
  test('all 22 metric series are present in result with series+source shape', () => {
    const samples = makeNormalizedSamples([{ speed: 10 }, { speed: 20 }, { speed: 30 }, { speed: 40 }])
    const result = deriveActivityMetricSeries({
      courseSeries: [
        [0, 0],
        [0, 0],
        [0, 0],
        [0, 0],
      ],
      distanceSeries: [0, 5, 10, 15],
      elevationBaseSeries: [0, 0, 0, 0],
      elapsedSeries: [0, 1, 2, 3],
      normalizedRawSamples: samples,
      useLegacyGpxDerivations: false,
      helpers: makeHelpers(),
    })

    const expectedMetrics = [
      'air_pressure',
      'altitude',
      'cadence',
      'core_temperature',
      'distance',
      'elevation',
      'g_force',
      'gear_position',
      'gradient',
      'ground_contact_time',
      'heading',
      'heartrate',
      'left_right_balance',
      'pace',
      'power',
      'speed',
      'stride_length',
      'stroke_rate',
      'temperature',
      'torque',
      'vertical_oscillation',
      'vertical_speed',
    ]
    expectedMetrics.forEach((metric) => {
      expect(result.metricSeriesMap).toHaveProperty(metric)
      expect(result.metricSeriesMap[metric]).toHaveProperty('series')
      expect(result.metricSeriesMap[metric]).toHaveProperty('source')
      expect(['direct', 'derived', 'mixed', 'missing']).toContain(result.metricSeriesMap[metric].source)
    })
  })

  test('missing metric marks source as missing with all-null series', () => {
    const samples = makeNormalizedSamples([{}, {}, {}, {}])
    const result = deriveActivityMetricSeries({
      courseSeries: [
        [0, 0],
        [0, 0],
        [0, 0],
        [0, 0],
      ],
      distanceSeries: [0, 0, 0, 0],
      elevationBaseSeries: [0, 0, 0, 0],
      elapsedSeries: [0, 1, 2, 3],
      normalizedRawSamples: samples,
      useLegacyGpxDerivations: false,
      helpers: makeHelpers(),
    })

    expect(result.metricSeriesMap.air_pressure.source).toBe('missing')
    expect(result.metricSeriesMap.air_pressure.series).toEqual([null, null, null, null])
  })

  test('gradient uses prefer-derived variant (derived wins over direct fallback)', () => {
    const samples = makeNormalizedSamples([
      { gradient: 5, elevation: 10 },
      { gradient: 7, elevation: 12 },
      { gradient: null, elevation: 14 },
      { gradient: null, elevation: 16 },
    ])
    const result = deriveActivityMetricSeries({
      courseSeries: [
        [0, 0],
        [0, 0],
        [0, 0],
        [0, 0],
      ],
      distanceSeries: [0, 5, 10, 15],
      elevationBaseSeries: [10, 12, 14, 16],
      elapsedSeries: [0, 1, 2, 3],
      normalizedRawSamples: samples,
      useLegacyGpxDerivations: false,
      helpers: makeHelpers(),
    })

    expect(result.metricSeriesMap.gradient).toBeDefined()
    expect(result.metricSeriesMap.gradient.series.length).toBe(4)
  })

  test('windowed rate derivation smooths speed across flat distance segments', () => {
    // Simulate dense SRT-like data: 10 points over 1 second, distance jumps every few frames
    // like GPS that only updates periodically
    const elapsedSeries = [0, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9]
    const distanceSeries = [0, 0, 0, 0, 2, 2, 2, 2, 4, 4]
    const elevationSeries = [100, 100.05, 100.1, 100.15, 100.2, 100.25, 100.3, 100.35, 100.4, 100.45]

    const helpers = makeHelpers()
    const samples = makeNormalizedSamples(distanceSeries.map(() => ({})))

    const result = deriveActivityMetricSeries({
      courseSeries: distanceSeries.map(() => [0, 0]),
      distanceSeries,
      elevationBaseSeries: elevationSeries,
      elapsedSeries,
      normalizedRawSamples: samples,
      useLegacyGpxDerivations: false,
      helpers,
      useWindowedRate: true,
      rateWindowSeconds: 0.5,
    })

    // Per-sample (non-windowed) would give: null, 0, 0, 0, 20, 0, 0, 0, 20, 0
    // Windowed gives more stable values since it looks back ~0.5s
    expect(result.metricSeriesMap.speed).toBeDefined()
    const speed = result.metricSeriesMap.speed.series
    // all non-null speed values should be reasonable (not spiking to 20, then 0)
    const nonNullSpeeds = speed.filter((v) => v !== null)
    nonNullSpeeds.forEach((v) => {
      expect(v).toBeGreaterThanOrEqual(0)
      expect(v).toBeLessThan(15) // not absurd spikes
    })
  })

  test('windowed rate leaves standard data unchanged', () => {
    // Legacy ~1 sample/sec data: windowed should produce same results as non-windowed
    // because lookback naturally lands on the previous sample
    const elapsedSeries = [0, 1, 2, 3, 4]
    const distanceSeries = [0, 10, 20, 30, 40]
    const elevationSeries = [100, 110, 120, 130, 140]

    const helpers = makeHelpers()
    const samples = makeNormalizedSamples(distanceSeries.map(() => ({})))

    const result = deriveActivityMetricSeries({
      courseSeries: distanceSeries.map(() => [0, 0]),
      distanceSeries,
      elevationBaseSeries: elevationSeries,
      elapsedSeries,
      normalizedRawSamples: samples,
      useLegacyGpxDerivations: false,
      helpers,
      useWindowedRate: true,
      rateWindowSeconds: 1,
    })

    // With 1-second sample spacing, windowed should give ~10 m/s consistently
    const speed = result.metricSeriesMap.speed.series
    const nonNullSpeeds = speed.filter((v) => v !== null)
    nonNullSpeeds.forEach((v) => {
      expect(v).toBeCloseTo(10, 0)
    })
  })

  test('distance-based heading derivation ignores sub-meter GPS jitter', () => {
    const elapsedSeries = [0, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7]
    const courseSeries = [
      [0, 0],
      [0.0000008, 0.0000008],
      [-0.0000008, 0.0000016],
      [0.0000008, 0.0000024],
      [0, 0.0000120],
      [0.0000008, 0.0000128],
      [-0.0000008, 0.0000136],
      [0, 0.0000240],
    ]
    const helpers = makeHelpers({ calculateBearingDegrees })
    const samples = makeNormalizedSamples(courseSeries.map(() => ({})))

    const result = deriveActivityMetricSeries({
      courseSeries,
      distanceSeries: [0, 0.13, 0.26, 0.39, 1.45, 1.58, 1.71, 3.0],
      elevationBaseSeries: courseSeries.map(() => 0),
      elapsedSeries,
      normalizedRawSamples: samples,
      useLegacyGpxDerivations: false,
      helpers,
    })

    const heading = result.metricSeriesMap.heading.series
    expect(heading.slice(0, 4)).toEqual([null, null, null, null])
    const stabilizedHeadings = heading.slice(4).filter((value) => value !== null)
    const minHeading = Math.min(...stabilizedHeadings)
    const maxHeading = Math.max(...stabilizedHeadings)
    const averageHeading = stabilizedHeadings.reduce((sum, value) => sum + value, 0) / stabilizedHeadings.length

    expect(maxHeading - minHeading).toBeLessThan(3)
    expect(averageHeading).toBeGreaterThan(88)
    expect(averageHeading).toBeLessThan(93)
  })

  test('circular EMA keeps a gentle turn continuous', () => {
    const courseSeries = [
      [0, 0],
      [0, 0.00003],
      [0.00001, 0.00006],
      [0.00003, 0.00008],
      [0.00006, 0.00009],
      [0.00009, 0.00009],
    ]
    const result = deriveActivityMetricSeries({
      courseSeries,
      distanceSeries: [0, 3.3, 6.8, 10.0, 13.4, 16.7],
      elevationBaseSeries: courseSeries.map(() => 0),
      elapsedSeries: [0, 1, 2, 3, 4, 5],
      normalizedRawSamples: makeNormalizedSamples(courseSeries.map(() => ({}))),
      useLegacyGpxDerivations: false,
      helpers: makeHelpers({ calculateBearingDegrees }),
    })

    const finiteHeadings = result.metricSeriesMap.heading.series.filter((value) => value !== null)
    expect(finiteHeadings.length).toBeGreaterThan(2)
    const isAscendingTurn = finiteHeadings[finiteHeadings.length - 1] >= finiteHeadings[0]

    for (let index = 1; index < finiteHeadings.length; index += 1) {
      const delta = finiteHeadings[index] - finiteHeadings[index - 1]
      if (isAscendingTurn) {
        expect(delta).toBeGreaterThanOrEqual(-0.5)
      } else {
        expect(delta).toBeLessThanOrEqual(0.5)
      }
      expect(Math.abs(delta)).toBeLessThan(25)
    }
  })
})
