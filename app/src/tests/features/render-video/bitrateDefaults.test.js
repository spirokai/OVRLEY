import { describe, expect, test } from 'vitest'
import { getDefaultBitrate, BITRATE_BINS, BITRATE_FALLBACK } from '@/features/render-video/data/bitrateDefaults'

describe('getDefaultBitrate', () => {
  test('returns 20 Mbps for 1080p h264 at 30 fps', () => {
    expect(getDefaultBitrate(1920, 1080, 30, 'h264')).toBe(20)
  })

  test('returns 35 Mbps for 1080p h264 at 60 fps (high frame rate)', () => {
    expect(getDefaultBitrate(1920, 1080, 60, 'h264')).toBe(35)
  })

  test('returns 20 Mbps for 1080p h265 at 30 fps', () => {
    expect(getDefaultBitrate(1920, 1080, 30, 'h265')).toBe(20)
  })

  test('returns 35 Mbps for 1080p h265 at 60 fps', () => {
    expect(getDefaultBitrate(1920, 1080, 60, 'h265')).toBe(35)
  })

  test('returns 60 Mbps for 4K h264 at 30 fps', () => {
    expect(getDefaultBitrate(3840, 2160, 30, 'h264')).toBe(60)
  })

  test('returns 100 Mbps for 4K h264 at 60 fps', () => {
    expect(getDefaultBitrate(3840, 2160, 60, 'h264')).toBe(100)
  })

  test('uses fallback for resolution exceeding all bins', () => {
    expect(getDefaultBitrate(7680, 4320, 30, 'h264')).toBe(80)
  })

  test('recognizes hevc codec alias as h265', () => {
    expect(getDefaultBitrate(1920, 1080, 30, 'hevc')).toBe(20)
  })

  test('recognizes x265 codec alias as h265', () => {
    expect(getDefaultBitrate(1920, 1080, 60, 'x265')).toBe(35)
  })

  test('handles missing dimensions gracefully by treating as zero pixels', () => {
    expect(getDefaultBitrate(0, 0, 30, 'h264')).toBe(20)
  })

  test('handles undefined codec by defaulting to h264', () => {
    expect(getDefaultBitrate(1920, 1080, 30, undefined)).toBe(20)
  })

  test('hfr threshold is strictly > 30 fps', () => {
    expect(getDefaultBitrate(1920, 1080, 30, 'h264')).toBe(20)
    expect(getDefaultBitrate(1920, 1080, 31, 'h264')).toBe(35)
  })
})

describe('BITRATE_BINS', () => {
  test('has exactly 3 bins: 1080p, 1440p, 4K', () => {
    expect(BITRATE_BINS).toHaveLength(3)
    expect(BITRATE_BINS.map((b) => b.label)).toEqual(['1080p', '1440p', '4K'])
  })
})

describe('BITRATE_FALLBACK', () => {
  test('has all required fields', () => {
    expect(BITRATE_FALLBACK).toHaveProperty('h264')
    expect(BITRATE_FALLBACK).toHaveProperty('h265')
    expect(BITRATE_FALLBACK).toHaveProperty('h264Hfr')
    expect(BITRATE_FALLBACK).toHaveProperty('h265Hfr')
  })
})
