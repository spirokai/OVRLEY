import { describe, expect, test } from 'vite-plus/test'
import { placeVideosByCreationTime, resolveVideoSync, resolveVideosSync } from '@/lib/video-sync'
import { DEFAULT_GLOBAL_DEFAULTS } from '@/lib/template/template-constants'
import { captureBatchSync, createBatchRenderRequest } from '@/features/render-video/utils/renderRequest'
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

  test("'local' keeps the clock-text reading", () => {
    expect(resolveVideoSync(video, activitySummary, 'local').syncWarning).toBeNull()
  })
})

describe('resolveVideosSync session timezone inference', () => {
  const summary = { syncTime: '2026-07-18T08:00:00Z', endTime: '2026-07-18T08:02:00Z', timezone: 'Europe/Sofia' }

  test('each ambiguous clip infers its own interpretation and the session adopts an agreed one', () => {
    // 08:00:30Z only overlaps when read as UTC; 11:00:30 only when read as Sofia local clock text.
    const utcClip = createTestVideo({ id: 'utc', creationTime: '2026-07-18T08:00:30Z', timeSource: 'ffprobe', durationSeconds: 20 })
    const localClip = createTestVideo({ id: 'local', creationTime: '2026-07-18T11:00:30Z', timeSource: 'ffprobe', durationSeconds: 20 })

    const both = resolveVideosSync([utcClip, localClip], summary, null)
    expect(both.placements).toEqual([
      { offsetSeconds: 30, determined: true, syncWarning: null },
      { offsetSeconds: 30, determined: true, syncWarning: null },
    ])
    // A sole local match stays automatic, so the clips do not agree on an explicit interpretation.
    expect(both.timezoneMode).toBeNull()
    expect(resolveVideosSync([utcClip], summary, null).timezoneMode).toBe('utc')
    expect(resolveVideosSync([localClip], summary, null).timezoneMode).toBeNull()
  })

  test('an explicit session mode applies to every clip', () => {
    const first = createTestVideo({ id: 'a', creationTime: '2026-07-18T08:00:30Z', timeSource: 'ffprobe', durationSeconds: 20 })
    const second = createTestVideo({ id: 'b', creationTime: '2026-07-18T08:01:00Z', timeSource: 'ffprobe', durationSeconds: 20 })
    const result = resolveVideosSync([first, second], summary, 'utc')
    expect(result.placements.map(({ offsetSeconds }) => offsetSeconds)).toEqual([30, 60])
    expect(resolveVideosSync([createTestVideo({ creationTime: null })], summary, 'utc').placements).toEqual([
      { offsetSeconds: 0, determined: false, syncWarning: 'Could not determine video creation time' },
    ])
    expect(result.timezoneMode).toBe('utc')
  })

  test('trusted GPS clips leave the session mode unset', () => {
    const gps = createTestVideo({ creationTime: '2026-07-18T08:00:30Z', timeSource: 'gps', durationSeconds: 20 })
    expect(resolveVideosSync([gps], summary, 'local').timezoneMode).toBeNull()
  })
})

describe('placeVideosByCreationTime without an external activity', () => {
  test('places clips at real time relative to the earliest clip and keeps gaps', () => {
    const videos = [
      createTestVideo({ id: 'b', creationTime: '2026-07-18T08:01:05Z', timeSource: 'ffprobe' }),
      createTestVideo({ id: 'a', creationTime: '2026-07-18T08:00:00Z', timeSource: 'ffprobe' }),
      createTestVideo({ id: 'c', creationTime: '2026-07-18T08:10:00.000Z', timeSource: 'gps' }),
    ]
    expect(placeVideosByCreationTime(videos)).toEqual([
      { offsetSeconds: 65, determined: true, syncWarning: null },
      { offsetSeconds: 0, determined: true, syncWarning: null },
      { offsetSeconds: 600, determined: true, syncWarning: null },
    ])
  })

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

  function source(id, creationTime) {
    return {
      sourceId: id,
      metadata: {
        path: `C:/recordings/${id}.mp4`,
        creationTime,
        timeSource: 'ffprobe',
        duration: 20,
        fps: 30,
        fpsNum: 30,
        fpsDen: 1,
        rotationDegrees: 0,
        hasAudio: true,
        resolution: { width: 1920, height: 1080 },
      },
      stamp: { sizeBytes: 1000, modifiedAtUnixNanos: '1784361600000000000' },
    }
  }

  test('renders reviewed outputs at the committed offsets of their sources, or with embedded telemetry without an activity', () => {
    const first = source('first', '2026-07-18T07:59:00Z')
    const second = source('second', '2026-07-18T08:00:40Z')
    const editorSnapshot = {
      config: { scene: { width: 1920, height: 1080 }, backdrops: [], rasters: [], labels: [], values: [], plots: [] },
      globalDefaults: DEFAULT_GLOBAL_DEFAULTS,
      parsedActivity: { sync_time: summary.syncTime, metadata: { timezone: summary.timezone }, sample_elapsed_seconds: [0, 120] },
      activitySummary: summary,
      videoSyncTimezoneMode: 'utc',
    }
    const settings = { exportMode: 'composite', codec: 'libx264', fps: 30, widgetUpdateRate: 1, qualityType: 'quality', qualityValue: 20 }
    const outputs = [
      { id: first.metadata.path, pieces: [{ sourceId: first.sourceId, startFrame: 0, endFrame: 600, skipOverlay: false }], transitions: [] },
      { id: second.metadata.path, pieces: [{ sourceId: second.sourceId, startFrame: 30, endFrame: 300, skipOverlay: true }], transitions: [] },
    ]
    const offsets = { [first.sourceId]: -9, [second.sourceId]: 43 }
    const destination = { kind: 'sourceNamed', directory: 'C:/output' }
    const request = createBatchRenderRequest({ editorSnapshot, settings, inspectionId: 'inspection', outputs, offsets, destination })

    expect(request.destination).toEqual(destination)

    expect(request.activity).toEqual({ mode: 'externalActivity', activity: editorSnapshot.parsedActivity, offsets: { first: -9, second: 43 } })
    expect(request.outputs).toEqual(outputs)
    expect(Object.isFrozen(request.outputs[0].pieces)).toBe(true)
    expect(Object.isFrozen(request.activity.offsets)).toBe(true)
    expect(captureBatchSync(editorSnapshot)).toEqual({ activitySummary: summary, timezoneMode: 'utc' })

    const embedded = createBatchRenderRequest({
      editorSnapshot: { ...editorSnapshot, parsedActivity: null, activitySummary: null },
      settings,
      inspectionId: 'inspection',
      outputs,
      offsets: {},
      destination,
    })
    expect(embedded.activity).toEqual({ mode: 'embeddedActivity' })
  })

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
