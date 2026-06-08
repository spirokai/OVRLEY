import { describe, expect, test } from 'vitest'
import { parseSrtActivityFile } from '@/lib/activity/srt-parser'

const FORMAT_A_SRT = [
  '1',
  '00:00:02,001 --> 00:00:02,035',
  '<font size="28">FrameCnt: 61, DiffTime: 34ms',
  '2025-07-23 10:21:41.694',
  '[iso: 200] [shutter: 1/3200.0] [fnum: 1.7] [ev: 0] [focal_len: 24.00] [latitude: 51.118062] [longitude: 88.083302] [rel_alt: 20.000 abs_alt: 864.309] [ct: 5491] </font>',
  '',
  '2',
  '00:00:03,002 --> 00:00:03,036',
  '<font size="28">FrameCnt: 91, DiffTime: 34ms',
  '2025-07-23 10:21:42.695',
  '[iso: 400] [shutter: 1/1600.0] [fnum: 2.8] [ev: -1] [focal_len: 50.00] [latitude: 51.118063] [longitude: 88.083303] [rel_alt: 20.500 abs_alt: 864.809] [ct: 5500] </font>',
  '',
].join('\n')

describe('parseSrtActivityFile (Format A — bracketed telemetry)', () => {
  test('produces parsedActivity and debugPayload shapes matching FIT/GPX', () => {
    const result = parseSrtActivityFile(FORMAT_A_SRT, 'test-format-a.SRT')
    expect(result).toHaveProperty('parsedActivity')
    expect(result).toHaveProperty('debugPayload')
    expect(result.parsedActivity.file_format).toBe('srt')
    expect(result.parsedActivity.file_name).toBe('test-format-a.SRT')
  })

  test('populates sample_elapsed_seconds with millisecond precision', () => {
    const result = parseSrtActivityFile(FORMAT_A_SRT, 'test-format-a.SRT')
    const elapsed = result.parsedActivity.sample_elapsed_seconds
    expect(elapsed[0]).toBeCloseTo(2.001, 3)
    expect(elapsed[1]).toBeCloseTo(3.002, 3)
  })

  test('populates abs_alt into both altitude and elevation', () => {
    const result = parseSrtActivityFile(FORMAT_A_SRT, 'test-format-a.SRT')
    expect(result.parsedActivity.altitude[0]).toBe(864.309)
    expect(result.parsedActivity.elevation[0]).toBe(864.309)
  })

  test('populates camera telemetry fields from bracketed keys', () => {
    const result = parseSrtActivityFile(FORMAT_A_SRT, 'test-format-a.SRT')
    expect(result.parsedActivity.iso[0]).toBe(200)
    expect(result.parsedActivity.iso[1]).toBe(400)
    expect(result.parsedActivity.aperture[0]).toBe(1.7)
    expect(result.parsedActivity.aperture[1]).toBe(2.8)
    expect(result.parsedActivity.shutter_speed[0]).toBeCloseTo(1 / 3200.0, 10)
    expect(result.parsedActivity.shutter_speed[1]).toBeCloseTo(1 / 1600.0, 10)
    expect(result.parsedActivity.focal_length[0]).toBe(24)
    expect(result.parsedActivity.focal_length[1]).toBe(50)
    expect(result.parsedActivity.ev[0]).toBe(0)
    expect(result.parsedActivity.ev[1]).toBe(-1)
    expect(result.parsedActivity.color_temperature[0]).toBe(5491)
    expect(result.parsedActivity.color_temperature[1]).toBe(5500)
  })

  test('bypasses idle-gap insertion for SRT telemetry', () => {
    const result = parseSrtActivityFile(FORMAT_A_SRT, 'test-format-a.SRT')
    expect(result.parsedActivity.metadata.inserted_idle_sample_count).toBe(0)
    expect(result.parsedActivity.metadata.sample_count).toBe(2)
  })

  test('tolerates missing bracket fields in some cues', () => {
    const partialSrt = [
      '1',
      '00:00:01,000 --> 00:00:01,033',
      '2025-07-23 10:21:40.000',
      '[iso: 100] [shutter: 1/2500.0]',
      '',
      '2',
      '00:00:02,000 --> 00:00:02,033',
      '2025-07-23 10:21:41.000',
      '[iso: 200]',
      '',
    ].join('\n')
    const result = parseSrtActivityFile(partialSrt, 'partial.SRT')
    expect(result.parsedActivity.iso[0]).toBe(100)
    expect(result.parsedActivity.iso[1]).toBe(200)
    expect(result.parsedActivity.shutter_speed[0]).toBeCloseTo(1 / 2500.0, 10)
    expect(result.parsedActivity.shutter_speed[1]).toBeNull()
  })
})

describe('parseSrtActivityFile (shutter parsing)', () => {
  test('parses reciprocal forms to numeric seconds', () => {
    const srt = [
      '1',
      '00:00:01,000 --> 00:00:01,033',
      '2025-07-23 10:21:40.000',
      '[shutter: 1/3200.0]',
      '',
      '2',
      '00:00:02,000 --> 00:00:02,033',
      '2025-07-23 10:21:41.000',
      '[shutter: 1/50]',
      '',
    ].join('\n')
    const result = parseSrtActivityFile(srt, 'shutter.SRT')
    expect(result.parsedActivity.shutter_speed[0]).toBeCloseTo(0.0003125, 10)
    expect(result.parsedActivity.shutter_speed[1]).toBeCloseTo(0.02, 10)
  })

  test('parses decimal-second shutter forms', () => {
    const srt = ['1', '00:00:01,000 --> 00:00:01,033', '2025-07-23 10:21:40.000', '[shutter: 0.5]', ''].join('\n')
    const result = parseSrtActivityFile(srt, 'shutter-decimal.SRT')
    expect(result.parsedActivity.shutter_speed[0]).toBeCloseTo(0.5, 10)
  })

  test('returns null for unsupported shutter forms', () => {
    const srt = ['1', '00:00:01,000 --> 00:00:01,033', '2025-07-23 10:21:40.000', '[shutter: 2"]', ''].join('\n')
    const result = parseSrtActivityFile(srt, 'shutter-bad.SRT')
    expect(result.parsedActivity.shutter_speed[0]).toBeNull()
  })

  test('Format B parses shutter as denominator of reciprocal fraction', () => {
    const srt = [
      '1',
      '00:00:00,000 --> 00:00:01,000',
      'HOME(149.0251,-20.2532) 2017.08.05 14:11:51',
      'GPS(149.0251,-20.2532,14) BAROMETER:1.9',
      'ISO:100 Shutter:60 EV:0 Fnum:2.2',
      '',
      '2',
      '00:00:01,000 --> 00:00:02,000',
      'HOME(149.0251,-20.2532) 2017.08.05 14:11:52',
      'GPS(149.0251,-20.2532,16) BAROMETER:2.0',
      'ISO:200 Shutter:3200 EV: Fnum:2.8',
      '',
    ].join('\n')
    const result = parseSrtActivityFile(srt, 'format-b.SRT')
    expect(result.parsedActivity.shutter_speed[0]).toBeCloseTo(1 / 60, 6)
    expect(result.parsedActivity.shutter_speed[1]).toBeCloseTo(1 / 3200, 10)
    expect(result.parsedActivity.iso[0]).toBe(100)
    expect(result.parsedActivity.iso[1]).toBe(200)
    expect(result.parsedActivity.aperture[0]).toBe(2.2)
    expect(result.parsedActivity.aperture[1]).toBe(2.8)
    expect(result.parsedActivity.ev[0]).toBe(0)
    expect(result.parsedActivity.ev[1]).toBeNull()
    expect(result.parsedActivity.altitude[0]).toBe(14)
    expect(result.parsedActivity.altitude[1]).toBe(16)
  })
})
