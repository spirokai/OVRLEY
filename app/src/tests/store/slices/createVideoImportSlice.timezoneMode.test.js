import { describe, expect, test } from 'vitest'
import { resolveVideoSyncState } from '@/store/slices/createVideoImportSlice'

// 5:47:13 activity ending 17:29 Sofia time (UTC+3).
const activitySummary = {
  syncTime: '2026-07-18T08:41:47.000Z',
  endTime: '2026-07-18T14:29:00.000Z',
  timezone: 'Europe/Sofia',
}

function ffprobeVideo(creationTime, videoSyncTimezoneMode) {
  return { importedVideoCreationTime: creationTime, importedVideoTimeSource: 'ffprobe', importedVideoDuration: 60, videoSyncTimezoneMode }
}

describe('resolveVideoSyncState with an explicit timezone mode', () => {
  // 14:30Z is 17:30 Sofia (after the activity) with the timezone applied, 14:30 Sofia (inside it) without.
  const creationTime = '2026-07-18T14:30:00Z'

  test("'utc' rejects a clip recorded after the activity", () => {
    expect(resolveVideoSyncState(ffprobeVideo(creationTime, 'utc'), activitySummary).videoSyncWarning).not.toBeNull()
  })

  test("'local' keeps the clock-text reading", () => {
    expect(resolveVideoSyncState(ffprobeVideo(creationTime, 'local'), activitySummary).videoSyncWarning).toBeNull()
  })
})
