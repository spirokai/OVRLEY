import { describe, expect, test } from 'vitest'
import { orderVideoParts, validateVideoParts, videoPartAtTime } from '@/features/video-preview/utils/videoParts'

describe('camera chapter ordering', () => {
  const parts = [
    { path: 'a.mp4', startSeconds: 0, duration: 10 },
    { path: 'b.mp4', startSeconds: 10, duration: 20 },
    { path: 'c.mp4', startSeconds: 30, duration: 5 },
  ]

  test('resolves exact boundaries, backward scrubs and outside times on the group clock', () => {
    expect(videoPartAtTime(parts, -1)).toBe(parts[0])
    expect(videoPartAtTime(parts, 9.999)).toBe(parts[0])
    expect(videoPartAtTime(parts, 10)).toBe(parts[1])
    expect(videoPartAtTime(parts, 30)).toBe(parts[2])
    expect(videoPartAtTime(parts, 15)).toBe(parts[1])
    expect(videoPartAtTime(parts, 100)).toBe(parts[2])
  })

  test('rejects malformed group timing at ingress', () => {
    expect(validateVideoParts(parts, 35)).toBe(parts)
    expect(() => validateVideoParts(parts, 34)).toThrow()
    expect(() => validateVideoParts([parts[0], { ...parts[1], startSeconds: 11 }], 30)).toThrow()
    expect(() => validateVideoParts([], 0)).toThrow()
  })
  test('sorts numbered chapters naturally without mutating the selection', () => {
    const selection = ['C:/DJI/part10.mp4', 'C:/DJI/part2.MP4', 'C:/DJI/part1.mp4']
    expect(orderVideoParts(selection)).toEqual([selection[2], selection[1], selection[0]])
    expect(selection[0]).toBe('C:/DJI/part10.mp4')
  })

  test.each([[], ['a.mp4'], ['a.mp4', 'b.mov'], ['a.mp4', { name: 'b.mp4' }], ['a.mp4', 'a.mp4']])('rejects invalid selections: %j', (...paths) =>
    expect(() => orderVideoParts(paths)).toThrow(),
  )
})
