import { describe, expect, test } from 'vite-plus/test'
import { placeVideosByCreationTime, resolveVideoSync } from '@/lib/video-sync'
import { createTestVideo } from '../helpers/videoFixtures'

// 5:47:13 activity ending 17:29 Sofia time (UTC+3).
const activitySummary = {
  syncTime: '2026-07-18T08:41:47.000Z',
  endTime: '2026-07-18T14:29:00.000Z',
  timezone: 'Europe/Sofia',
}

describe('resolveVideoSync with an explicit timezone mode', () => {
  // 14:30Z is 17:30 Sofia (after the activity) with the timezone applied, 14:30 Sofia (inside it) without.
  const video = createTestVideo({ creationTime: '2026-07-18T14:30:00Z', timeSource: 'ffprobe' })

  test("'utc' places a clip recorded after the activity behind it with a warning", () => {
    const sync = resolveVideoSync(video, activitySummary, 'utc')
    expect(sync.syncWarning).toBe('Video is outside the activity')
    expect(sync.offsetSeconds).toBe(Date.parse('2026-07-18T14:30:00Z') / 1000 - Date.parse(activitySummary.syncTime) / 1000)
    expect(sync.timezoneMode).toBe('utc')
  })
})

describe('per-video timezone mode', () => {
  const summary = { syncTime: '2026-07-18T08:00:00Z', endTime: '2026-07-18T08:02:00Z', timezone: 'Europe/Sofia' }

  test('each ambiguous clip infers its own interpretation', () => {
    // 08:00:30Z only overlaps when read as UTC; 11:00:30 only when read as Sofia local clock text.
    const utcClip = createTestVideo({ id: 'utc', creationTime: '2026-07-18T08:00:30Z', timeSource: 'ffprobe', durationSeconds: 20 })
    const localClip = createTestVideo({ id: 'local', creationTime: '2026-07-18T11:00:30Z', timeSource: 'ffprobe', durationSeconds: 20 })

    expect(resolveVideoSync(utcClip, summary, null)).toEqual({ offsetSeconds: 30, determined: true, syncWarning: null, timezoneMode: 'utc' })
    // A sole local match stays automatic.
    expect(resolveVideoSync(localClip, summary, null)).toEqual({ offsetSeconds: 30, determined: true, syncWarning: null, timezoneMode: null })
  })

  test("a clip's explicit mode is kept", () => {
    const clip = createTestVideo({ id: 'a', creationTime: '2026-07-18T08:00:30Z', timeSource: 'ffprobe', durationSeconds: 20 })
    expect(resolveVideoSync(clip, summary, 'utc')).toMatchObject({ offsetSeconds: 30, timezoneMode: 'utc' })
    expect(resolveVideoSync(createTestVideo({ creationTime: null }), summary, 'utc')).toEqual({
      offsetSeconds: 0,
      determined: false,
      syncWarning: 'Could not determine video creation time',
      timezoneMode: null,
    })
  })
})

describe('placeVideosByCreationTime without an external activity', () => {
  test('a clip without a usable time is undetermined and starts at zero with a warning', () => {
    const videos = [createTestVideo({ id: 'a', creationTime: null }), createTestVideo({ id: 'b', creationTime: 'invalid', timeSource: 'ffprobe' })]
    expect(placeVideosByCreationTime(videos)).toEqual([
      { offsetSeconds: 0, determined: false, syncWarning: 'Could not determine video creation time' },
      { offsetSeconds: 0, determined: false, syncWarning: 'Could not determine video creation time' },
    ])
  })
})

describe('signed automatic synchronization and shared calibration', () => {
  const summary = { syncTime: '2026-07-18T08:00:00Z', endTime: '2026-07-18T08:02:00Z', timezone: 'Europe/Sofia' }

  test.each([
    ['2026-05-10T07:36:51Z', true],
    ['2026-07-18T07:59:30Z', true], // Video ends before activity.
    ['2026-07-18T07:59:40Z', true], // Video ends exactly at activity start.
    ['2026-07-18T07:59:48Z', false], // Video starts before activity and overlaps.
    ['2026-07-18T08:01:59Z', false], // Video starts just before activity ends.
    ['2026-07-18T08:02:00Z', true], // Video starts exactly at activity end.
    ['2026-07-18T08:02:01Z', true], // Video starts after activity.
  ])('keeps the signed offset of GPS creation time %s (outside=%s)', (creationTime, outside) => {
    const sync = resolveVideoSync(createTestVideo({ creationTime, timeSource: 'gps', durationSeconds: 20 }), summary, null)
    expect(sync.offsetSeconds).toBe((Date.parse(creationTime) - Date.parse(summary.syncTime)) / 1000)
    expect(sync.syncWarning).toBe(outside ? 'Video is outside the activity' : null)
  })

  test.each([[null], ['invalid-timestamp']])('falls back to zero with a warning for unusable creation time %s', (creationTime) => {
    const sync = resolveVideoSync(createTestVideo({ creationTime, timeSource: 'gps', durationSeconds: 20 }), summary, null)
    expect(sync.offsetSeconds).toBe(0)
    expect(sync.syncWarning).not.toBeNull()
  })

  test('keeps a clip outside the activity at its raw baseline', () => {
    const interactive = resolveVideoSync(
      createTestVideo({ creationTime: '2026-07-18T07:59:30Z', timeSource: 'gps', durationSeconds: 20 }),
      summary,
      null,
    )
    expect(interactive.offsetSeconds).toBe(-30)
    expect(interactive.syncWarning).toBe('Video is outside the activity')
    // Read as Sofia clock text, 07:59:48 is three hours before the 11:00 activity start.
    const local = resolveVideoSync(
      createTestVideo({ creationTime: '2026-07-18T07:59:48Z', timeSource: 'filename', durationSeconds: 20 }),
      summary,
      'local',
    )
    expect(local.offsetSeconds).toBe(-(3 * 3600 + 12))
    expect(local.syncWarning).toBe('Video is outside the activity')
  })
})
