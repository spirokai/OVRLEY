import { describe, expect, test } from 'vitest'

import { deriveActivityMetricSeries } from '@/lib/activity/metric-series'

function makeHelpers() {
  return {
    calculateBearingDegrees: () => null,
    haversineDistanceMeters: () => 0,
    isFiniteNumber: (v) => typeof v === 'number' && Number.isFinite(v),
    roundValue: (v) => v,
    safeNumber: (v) => (v === null || v === undefined ? null : Number(v)),
    safeTimestamp: (v) => v,
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
})
