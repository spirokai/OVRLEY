import { readFile } from 'node:fs/promises'
import path from 'node:path'

import { describe, expect, test } from 'vite-plus/test'

import parseIgcActivityFile from '@/lib/activity/igc-parser'

const fixtureDir = path.resolve('../src-tauri/ovrley_core/tests/fixtures/activity')

const fixtureNames = [
  '1G_77fv6m71.igc',
  '2016-11-08-xcs-aaa-02.igc',
  '20180427.igc',
  '20211015.igc',
  '20241007TZN.igc',
  '654G6NG1.IGC',
  'MD_85ugkjj1.IGC',
  'lad_lod_extensions.igc',
]

function isNumberOrNull(value) {
  return value === null || Number.isFinite(value)
}

function isRfc3339Timestamp(value) {
  if (typeof value !== 'string') return false
  const timestamp = Date.parse(value)
  return Number.isFinite(timestamp) && new Date(timestamp).toISOString() === value
}

describe('parseIgcActivityFile', () => {
  test.each(fixtureNames)('converts %s into RawActivity', async (name) => {
    const text = await readFile(path.join(fixtureDir, name), 'utf8')
    const result = await parseIgcActivityFile({
      name,
      text: () => Promise.resolve(text),
    })

    expect(result.file_name).toBe(name)
    expect(result.file_format).toBe('igc')
    expect(result.raw_samples.length).toBeGreaterThan(0)
    expect(result.options.skip_idle_gap_fill).toBe(false)
    expect(result.options.smoothing.heading).toEqual({
      enabled: true,
      method: 'circular_ema',
      window_seconds: 0.5,
    })

    for (const sample of result.raw_samples) {
      expect(isRfc3339Timestamp(sample.timestamp)).toBe(true)
      expect(isNumberOrNull(sample.latitude)).toBe(true)
      expect(isNumberOrNull(sample.longitude)).toBe(true)
    }

    for (const key of ['activity_name', 'date', 'glider_type', 'timezone', 'logger_manufacturer', 'logger_type', 'parse_errors']) {
      expect(result.metadata).toHaveProperty(key)
    }
  })

  test('normalizes LXNAV extension values into RawActivity units', async () => {
    const text = await readFile(path.join(fixtureDir, '1G_77fv6m71.igc'), 'utf8')
    const result = await parseIgcActivityFile({
      name: '1G_77fv6m71.igc',
      text: () => Promise.resolve(text),
    })

    const speedsKmh = result.raw_samples.map((sample) => (sample.speed === null ? null : sample.speed * 3.6)).filter((value) => value !== null)
    expect(Math.max(...speedsKmh)).toBeCloseTo(174.66, 2)
    expect(speedsKmh).not.toContain(17466)

    // Raw B101903 extension fields: GSP=09437, TRT=331, VAT=00346, OAT=0244.
    expect(result.raw_samples[31].speed).toBeCloseTo(94.37 / 3.6, 10)
    expect(result.raw_samples[31].heading).toBe(331)
    expect(result.raw_samples[31].vertical_speed).toBeCloseTo(3.46, 10)
    expect(result.raw_samples[31].temperature).toBeCloseTo(24.4, 10)
  })
})
