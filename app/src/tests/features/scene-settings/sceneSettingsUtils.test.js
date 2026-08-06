import { describe, expect, test } from 'vitest'
import { formatVideoCreationTime } from '@/features/scene-settings/utils/sceneSettingsUtils'

describe('formatVideoCreationTime', () => {
  test('converts GPS time into the recording timezone', () => {
    expect(formatVideoCreationTime('2026-07-28T01:46:15+00:00', 'gps', 'Europe/Prague')).toBe('2026-07-28 03:46:15')
  })

  test('keeps ffprobe clock text without a UTC suffix', () => {
    expect(formatVideoCreationTime('2026-07-27T17:46:07.000000Z', 'ffprobe', null)).toBe('2026-07-27 17:46:07')
  })
})
