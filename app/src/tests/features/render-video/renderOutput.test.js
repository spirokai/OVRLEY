import { describe, expect, test } from 'vitest'
import { normalizeRenderOutputPath } from '@/features/render-video/utils/render-output'

describe('render output path normalization', () => {
  test.each([
    ['transparent', 'C:\\renders\\ride.mov', 'C:\\renders\\ride.mov'],
    ['transparent', 'C:\\renders\\ride.mp4', 'C:\\renders\\ride.mov'],
    ['composite', '/renders/ride.mov', '/renders/ride.mp4'],
    ['composite', '/renders/ride', '/renders/ride.mp4'],
  ])('normalizes %s output extension while preserving the stem', (mode, input, expected) => {
    expect(normalizeRenderOutputPath(input, mode)).toBe(expected)
  })
})
