import { describe, expect, test } from 'vite-plus/test'
import { listFilmstripTiles } from '@/features/player/utils/filmstripTiles'

// Four samples 20 s apart in a 2×2 sprite sheet.
const filmstrip = { spriteUrl: 'blob:sprite', columns: 2, rows: 2, intervalSeconds: 20, sampleCount: 4 }

function tiles(overrides) {
  return listFilmstripTiles({
    filmstrip,
    sourceStartSecond: 0,
    sourceEndSecond: 60,
    leftPx: 0,
    pxPerSecond: 17,
    tileWidthPx: 85,
    widthPx: 500,
    ...overrides,
  })
}

function positions(list) {
  return list.map((tile) => tile.style.backgroundPosition)
}

describe('listFilmstripTiles', () => {
  test('zoomed in past the sample spacing, neighbouring tiles repeat the nearest frame', () => {
    // Each 85 px tile covers 5 s: tiles start at 0, 5, 10, 15, 20 and 25 s.
    const list = tiles()

    expect(list.map((tile) => tile.style.left)).toEqual(['0px', '85px', '170px', '255px', '340px', '425px'])
    expect(positions(list)).toEqual(['0% 0%', '0% 0%', '100% 0%', '100% 0%', '100% 0%', '100% 0%'])
    expect(list[0].style).toMatchObject({ width: '85px', backgroundImage: 'url("blob:sprite")', backgroundSize: '200% 200%' })
  })

  test('zoomed out, tiles skip frames', () => {
    // Each tile covers 40 s: tiles start at 0 and 40 s and show samples 0 and 2.
    expect(positions(tiles({ pxPerSecond: 85 / 40 }))).toEqual(['0% 0%', '0% 100%'])
  })

  test('only tiles inside the viewport are listed', () => {
    const list = tiles({ leftPx: -200, widthPx: 100 })

    expect(list.map((tile) => tile.key)).toEqual([2, 3])
    expect(list.map((tile) => tile.style.left)).toEqual(['170px', '255px'])
  })

  test('a segment later in the video starts from its own source time', () => {
    // The first tile of a segment starting at 30 s, halfway between samples 1 and 2, shows sample 2.
    expect(positions(tiles({ sourceStartSecond: 30, sourceEndSecond: 32 }))).toEqual(['0% 100%'])
  })

  test('tiles past the last sample show the last frame', () => {
    expect(positions(tiles({ sourceStartSecond: 75, sourceEndSecond: 80 }))).toEqual(['100% 100%'])
  })
})
