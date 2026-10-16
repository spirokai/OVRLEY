import { describe, expect, test } from 'vite-plus/test'
import { interpolateNumericSeries, MISSING_SAMPLE_POLICY } from '@/lib/interpolation'

describe('interpolateNumericSeries', () => {
  const elapsed = [0, 1, 2, 3]
  const values = [0, null, 4, 6]

  test('bridges missing samples by default', () => {
    expect(interpolateNumericSeries(elapsed, values, 1)).toBe(2)
  })

  test('preserves exact missing samples and interpolates around zero', () => {
    expect(interpolateNumericSeries(elapsed, values, 1, MISSING_SAMPLE_POLICY.PRESERVE)).toBeNull()
    expect(interpolateNumericSeries(elapsed, values, 1.5, MISSING_SAMPLE_POLICY.PRESERVE)).toBe(2)
  })

  test('ramps down to and up from a preserved zero without bridging across it', () => {
    const sparseValues = [10, null, 30]
    expect(interpolateNumericSeries([0, 1, 2], sparseValues, 0.5, MISSING_SAMPLE_POLICY.PRESERVE)).toBe(5)
    expect(interpolateNumericSeries([0, 1, 2], sparseValues, 1.5, MISSING_SAMPLE_POLICY.PRESERVE)).toBe(15)
  })

  test('interpolates adjacent present samples with the preserve policy', () => {
    expect(interpolateNumericSeries(elapsed, values, 2.5, MISSING_SAMPLE_POLICY.PRESERVE)).toBe(5)
  })

  test('clamps to endpoint values with the preserve policy', () => {
    expect(interpolateNumericSeries(elapsed, values, -1, MISSING_SAMPLE_POLICY.PRESERVE)).toBe(0)
    expect(interpolateNumericSeries(elapsed, values, 4, MISSING_SAMPLE_POLICY.PRESERVE)).toBe(6)
  })

  test('rejects an unknown missing-sample policy', () => {
    expect(() => interpolateNumericSeries(elapsed, values, 1, 'unknown')).toThrow('Unknown missing sample policy: unknown')
  })
})
