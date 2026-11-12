import { describe, expect, test } from 'vite-plus/test'
import { filmstripHwaccel, planFilmstrip } from '@/features/video-preview/utils/filmstrip'

describe('planFilmstrip', () => {
  test('samples every 20 s at the video aspect ratio', () => {
    expect(planFilmstrip({ durationSeconds: 237, resolution: { width: 3840, height: 2160 } })).toEqual({
      intervalSeconds: 20,
      sampleCount: 12,
      frameWidth: 171,
      frameHeight: 96,
    })
  })

  test('spaces the frames of a long video wider so it stays within 200', () => {
    expect(planFilmstrip({ durationSeconds: 7200, resolution: { width: 1920, height: 1080 } })).toMatchObject({
      intervalSeconds: 36,
      sampleCount: 200,
    })
  })

  test('a vertical video has narrow frames and a short one a single sample', () => {
    expect(planFilmstrip({ durationSeconds: 8, resolution: { width: 1080, height: 1920 } })).toEqual({
      intervalSeconds: 20,
      sampleCount: 1,
      frameWidth: 54,
      frameHeight: 96,
    })
  })
})

describe('filmstripHwaccel', () => {
  test('decodes on the GPU an export can use', () => {
    expect(filmstripHwaccel({ nvdec: true, vaapi: true })).toBe('cuda')
    expect(filmstripHwaccel({ nvdec: false, vaapi: true })).toBe('vaapi')
    expect(filmstripHwaccel({ qsv: true })).toBe('d3d11va')
    expect(filmstripHwaccel({ hevcAmf: true })).toBe('d3d11va')
    expect(filmstripHwaccel({ videotoolbox: true })).toBe('videotoolbox')
  })

  test('decodes in software without a detected GPU', () => {
    expect(filmstripHwaccel({ nvdec: false, vaapi: false, qsv: false, h264Amf: false, hevcAmf: false, videotoolbox: false })).toBeNull()
    expect(filmstripHwaccel(null)).toBeNull()
  })
})
