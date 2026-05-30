/**
 * Tests for the shared FPS resolver used by playback engine and video
 * playback clock. Covers the same behavior previously tested through
 * getEffectivePreviewFps before it was inlined.
 */

import { describe, expect, test } from 'vitest'
import { getContainerFps } from '@/lib/update-rate'

describe('getContainerFps', () => {
  test('returns a number for common FPS and update rate combinations', () => {
    expect(typeof getContainerFps(30, 1)).toBe('number')
    expect(getContainerFps(30, 1)).toBeGreaterThan(0)
  })

  test('handles 60 FPS with update rate 2', () => {
    const result = getContainerFps(60, 2)
    expect(typeof result).toBe('number')
    expect(result).toBeGreaterThan(0)
  })

  test('handles 24 FPS with update rate 1', () => {
    const result = getContainerFps(24, 1)
    expect(typeof result).toBe('number')
    expect(result).toBeGreaterThan(0)
  })

  test('handles edge case with FPS 0', () => {
    const result = getContainerFps(0, 1)
    expect(typeof result).toBe('number')
  })
})
