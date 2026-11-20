import { describe, expect, test } from 'vite-plus/test'
import { formatVideoCreationTime } from '@/features/scene-settings/utils/sceneSettingsUtils'

describe('formatVideoCreationTime', () => {
  test('converts GPS time into the recording timezone', () => {
    expect(formatVideoCreationTime('2026-07-28T01:46:15+00:00', 'gps', 'Europe/Prague', null, 'en')).toBe('07/28/2026, 03:46:15')
  })

  test('converts timecode-resolved UTC time into the recording timezone', () => {
    expect(formatVideoCreationTime('2026-11-14T13:40:02.050Z', 'timecode', 'Europe/Berlin', null, 'en')).toBe('11/14/2026, 14:40:02')
  })

  test('keeps ffprobe clock text without a UTC suffix', () => {
    expect(formatVideoCreationTime('2026-07-27T17:46:07.000000Z', 'ffprobe', null, null, 'en')).toBe('07/27/2026, 17:46:07')
  })
})
