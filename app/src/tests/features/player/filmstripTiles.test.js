import { describe, expect, test } from 'vite-plus/test'
import { filmstripTileWidthPx, listFilmstripTiles, listWantedFilmstripWindows } from '@/features/player/utils/filmstripTiles'

// A base strip of four frames 20 s apart, and a zoom window of four frames 5 s apart from 20 s on.
const base = { spriteUrl: 'blob:base', startSeconds: 0, spacingSeconds: 20, sampleCount: 4 }
const window = { spriteUrl: 'blob:window', startSeconds: 20, spacingSeconds: 5, sampleCount: 4 }

function tiles(overrides) {
  return listFilmstripTiles({
    strips: [base],
    sourceStartSecond: 0,
    sourceEndSecond: 60,
    leftPx: 0,
    pxPerSecond: 17,
    tileWidthPx: 85,
    widthPx: 500,
    ...overrides,
  })
}

function frames(list) {
  return list.map((tile) => `${tile.style.backgroundImage} ${tile.style.backgroundPosition}`)
}

describe('listFilmstripTiles', () => {
  test('zoomed in past the spacing, neighbouring tiles repeat the frame of the stretch they start in', () => {
    // Each 85 px tile covers 5 s: tiles start at 0, 5, 10, 15, 20 and 25 s.
    const list = tiles()

    expect(list.map((tile) => tile.style.left)).toEqual(['0px', '85px', '170px', '255px', '340px', '425px'])
    expect(list.map((tile) => tile.style.backgroundPosition)).toEqual([
      '0% 0%',
      '0% 0%',
      '0% 0%',
      '0% 0%',
      '33.33333333333333% 0%',
      '33.33333333333333% 0%',
    ])
    expect(list[0].style).toMatchObject({ width: '85px', backgroundImage: 'url("blob:base")', backgroundSize: '400% 100%' })
  })

  test('zoomed out, tiles skip frames', () => {
    // Each tile covers 40 s: tiles start at 0 and 40 s and show samples 0 and 2.
    expect(tiles({ pxPerSecond: 85 / 40 }).map((tile) => tile.style.backgroundPosition)).toEqual(['0% 0%', '66.66666666666666% 0%'])
  })

  test('each tile takes the finest loaded strip that covers it, and the base strip elsewhere', () => {
    expect(frames(tiles({ strips: [base, window] }))).toEqual([
      'url("blob:base") 0% 0%',
      'url("blob:base") 0% 0%',
      'url("blob:base") 0% 0%',
      'url("blob:base") 0% 0%',
      'url("blob:window") 0% 0%',
      'url("blob:window") 33.33333333333333% 0%',
    ])
  })

  test('only tiles inside the viewport are listed', () => {
    const list = tiles({ leftPx: -200, widthPx: 100 })

    expect(list.map((tile) => tile.key)).toEqual([2, 3])
    expect(list.map((tile) => tile.style.left)).toEqual(['170px', '255px'])
  })

  test('tiles past the last sample show the last frame', () => {
    expect(tiles({ sourceStartSecond: 85, sourceEndSecond: 90 }).map((tile) => tile.style.backgroundPosition)).toEqual(['100% 0%'])
  })
})

describe('listWantedFilmstripWindows', () => {
  // A 2 h 16:9 video at 0 s; 48 px tall tiles are 85 px wide.
  const clip = { path: '/v/long.mp4', missing: false, offsetSeconds: 0, durationSeconds: 7200, resolution: { width: 1920, height: 1080 } }
  const wanted = (overrides) =>
    listWantedFilmstripWindows({ clips: [clip], viewStart: 0, viewEnd: 7200, widthPx: 1000, tileHeightPx: 48, ...overrides })

  test('tiles are the clip height at the video aspect ratio', () => {
    expect(filmstripTileWidthPx({ width: 1920, height: 1080 }, 48)).toBe(85)
  })

  test('the whole video in view needs only the base strip', () => {
    // 7200 s over 1000 px: a tile covers 612 s, wider than the 300 s base spacing.
    expect(wanted()).toEqual({})
  })

  test('zoomed in, asks for the windows of the matching level under the visible part', () => {
    // 1000 s over 1000 px: a tile covers 85 s, so level 3 (80 s spacing, 1920 s windows).
    expect(wanted({ viewStart: 1500, viewEnd: 2500 })).toEqual({
      '/v/long.mp4': [
        { kind: 'zoom', level: 3, window: 0 },
        { kind: 'zoom', level: 3, window: 1 },
      ],
    })
  })

  test('a clip later on the timeline maps the view to its own source time', () => {
    // 100 s over 1000 px: 8.5 s per tile, level 0 (240 s windows); the view is source 100–200 s.
    expect(wanted({ clips: [{ ...clip, offsetSeconds: 1000 }], viewStart: 1100, viewEnd: 1200 })).toEqual({
      '/v/long.mp4': [{ kind: 'zoom', level: 0, window: 0 }],
    })
  })

  test('a video on two clips asks once per window, and a missing video asks for none', () => {
    const clips = [clip, { ...clip, offsetSeconds: 50 }, { ...clip, path: '/v/missing.mp4', missing: true }]
    expect(wanted({ clips, viewStart: 100, viewEnd: 200 })).toEqual({ '/v/long.mp4': [{ kind: 'zoom', level: 0, window: 0 }] })
  })

  test('an unmeasured timeline asks for nothing', () => {
    expect(wanted({ widthPx: 0, viewStart: 100, viewEnd: 200 })).toEqual({})
  })
})
