import { describe, expect, test } from 'vite-plus/test'
import {
  baseFilmstripSpacingSeconds,
  filmstripFrameSize,
  filmstripHwaccel,
  filmstripWindowKey,
  filmstripZoomLevel,
  filmstripZoomWindows,
} from '@/features/video-preview/utils/filmstrip'

describe('filmstripFrameSize', () => {
  test('frames are 96 px tall at the video aspect ratio', () => {
    expect(filmstripFrameSize({ resolution: { width: 3840, height: 2160 } })).toEqual({ frameWidth: 171, frameHeight: 96 })
    expect(filmstripFrameSize({ resolution: { width: 1080, height: 1920 } })).toEqual({ frameWidth: 54, frameHeight: 96 })
  })
})

describe('filmstrip zoom grid', () => {
  test('the base strip holds at most 24 frames, never closer than 10 s', () => {
    expect(baseFilmstripSpacingSeconds(7200)).toBe(300)
    expect(baseFilmstripSpacingSeconds(237)).toBe(10)
  })

  test('picks the coarsest level that still gives every tile its own frame', () => {
    // A 2 h video's base strip spaces frames 300 s apart.
    expect(filmstripZoomLevel(400, 7200)).toBeNull()
    expect(filmstripZoomLevel(300, 7200)).toBeNull()
    // 299 s per tile: level 4 (160 s) is the coarsest at or under it.
    expect(filmstripZoomLevel(299, 7200)).toBe(4)
    expect(filmstripZoomLevel(25, 7200)).toBe(1)
    // Past the finest spacing, tiles repeat level 0's frames.
    expect(filmstripZoomLevel(3, 7200)).toBe(0)
  })

  test('a level must be finer than the base strip', () => {
    // A 600 s video's base strip spaces frames 25 s apart: level 1 (20 s) is the coarsest finer one.
    expect(filmstripZoomLevel(24, 600)).toBe(1)
    // A video up to 240 s long has 10 s base spacing: no window is ever finer.
    expect(filmstripZoomLevel(1, 237)).toBeNull()
  })

  test('lists the windows covering a stretch of the video', () => {
    // Level 0 windows are 240 s long.
    expect(filmstripZoomWindows(0, 200, 500, 7200).map((window) => window.window)).toEqual([0, 1, 2])
    // A stretch ending on a window edge stays out of the next window; one past the end is cut off.
    expect(filmstripZoomWindows(0, 240, 480, 7200).map((window) => window.window)).toEqual([1])
    expect(filmstripZoomWindows(0, 7100, 9000, 7200).map((window) => window.window)).toEqual([29])
    expect(filmstripZoomWindows(2, -50, 100, 7200)).toEqual([{ kind: 'zoom', level: 2, window: 0 }])
    // A stretch outside the video has no windows.
    expect(filmstripZoomWindows(0, 8000, 9000, 7200)).toEqual([])
  })

  test('keys strips by their window', () => {
    expect(filmstripWindowKey({ kind: 'base' })).toBe('base')
    expect(filmstripWindowKey({ kind: 'zoom', level: 2, window: 7 })).toBe('z2-7')
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
