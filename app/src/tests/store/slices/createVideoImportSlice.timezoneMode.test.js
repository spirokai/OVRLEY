import { describe, expect, test } from 'vitest'
import { createBatchCalibration, resolveBatchVideoTiming, resolveVideoSyncState } from '@/lib/video-sync'
import { DEFAULT_GLOBAL_DEFAULTS } from '@/lib/template/template-constants'
import { captureBatchSync, createBatchRenderRequest } from '@/features/render-video/utils/renderRequest'

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
    const sync = resolveVideoSyncState(ffprobeVideo(creationTime, 'utc'), activitySummary)
    expect(sync.videoSyncWarning).not.toBeNull()
    expect(sync.videoSyncOffsetSeconds).toBe(0)
  })

  test("'local' keeps the clock-text reading", () => {
    expect(resolveVideoSyncState(ffprobeVideo(creationTime, 'local'), activitySummary).videoSyncWarning).toBeNull()
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

  test('effective reference baseline -12 and committed -9 capture +3 once, ignoring detected time and transient preview', () => {
    const reference = source('reference', '2026-07-18T07:59:00Z')
    const queued = source('queued', '2026-07-18T08:00:40Z')
    const editorSnapshot = {
      config: { scene: { width: 1920, height: 1080 }, backdrops: [], rasters: [], labels: [], values: [], plots: [] },
      globalDefaults: DEFAULT_GLOBAL_DEFAULTS,
      parsedActivitySource: 'activity-file',
      parsedActivity: { sync_time: summary.syncTime, metadata: { timezone: summary.timezone }, sample_elapsed_seconds: [0, 120] },
      activitySummary: summary,
      importedVideoPath: reference.metadata.path,
      importedVideoCreationTime: '2026-07-18T07:59:48Z',
      importedVideoTimeSource: 'filename',
      importedVideoDuration: reference.metadata.duration,
      videoSyncTimezoneMode: 'utc',
      videoSyncOffsetSeconds: -9,
      videoSyncOffsetPreviewSeconds: 87,
    }
    expect(resolveVideoSyncState(editorSnapshot, summary).videoSyncOffsetSeconds).toBe(-12)
    const sync = captureBatchSync(editorSnapshot)
    const request = createBatchRenderRequest({
      editorSnapshot,
      sync,
      settings: { exportMode: 'composite', codec: 'libx264', fps: 30, widgetUpdateRate: 1, qualityType: 'quality', qualityValue: 20 },
      inspectionId: 'inspection',
      calibrationSource: reference,
      outputDirectory: 'C:/output',
      jobs: [reference, queued].map((video) => ({
        id: video.sourceId,
        source: video,
        timing: resolveBatchVideoTiming(video.metadata, summary, sync.calibration, video.sourceId === reference.sourceId).timing,
        skipOverlay: false,
      })),
    })
    const { reference: captured, automaticOffsets } = request.activity
    const correction = captured.committedOffsetSeconds - captured.automaticOffsetSeconds
    expect(correction).toBe(3)
    expect(request.jobs.map((job) => automaticOffsets[job.sourceId] + correction)).toEqual([-9, 43])
    // A different live checkbox value cannot reinterpret this calibration.
    editorSnapshot.videoSyncTimezoneMode = 'local'
    expect(request.activity.timezoneMode).toBe('utc')
    expect(resolveVideoSyncState(editorSnapshot, summary).videoSyncOffsetSeconds).toBe(0)
  })

  test.each([
    ['2026-05-10T07:36:51Z', 0, true], // Reported -5876649-second failed sync.
    ['2026-07-18T07:59:30Z', 0, true], // Video ends before activity.
    ['2026-07-18T07:59:40Z', 0, true], // Video ends exactly at activity start.
    ['2026-07-18T07:59:48Z', -12, false], // Video starts before activity and overlaps.
    ['2026-07-18T08:01:59Z', 119, false], // Video starts just before activity ends.
    ['2026-07-18T08:02:00Z', 0, true], // Video starts exactly at activity end.
    ['2026-07-18T08:02:01Z', 0, true], // Video starts after activity.
    [null, 0, true],
    ['invalid-timestamp', 0, true],
  ])('resolves creation time %s to offset %s with warning=%s', (creationTime, offset, warning) => {
    const sync = resolveVideoSyncState(
      {
        importedVideoCreationTime: creationTime,
        importedVideoTimeSource: 'gps',
        importedVideoDuration: 20,
        videoSyncTimezoneMode: null,
      },
      summary,
    )
    expect(sync.videoSyncOffsetSeconds).toBe(offset)
    if (warning) expect(sync.videoSyncWarning).not.toBeNull()
    else expect(sync.videoSyncWarning).toBeNull()
  })

  test('resets failed interactive sync to zero while retaining the raw baseline for batch calibration', () => {
    const video = { creationTime: '2026-07-18T07:59:30Z', timeSource: 'gps', duration: 20 }
    const interactive = resolveVideoSyncState(
      {
        importedVideoCreationTime: video.creationTime,
        importedVideoTimeSource: video.timeSource,
        importedVideoDuration: video.duration,
        videoSyncTimezoneMode: null,
      },
      summary,
    )
    expect(interactive.videoSyncOffsetSeconds).toBe(0)
    expect(interactive.videoSyncWarning).not.toBeNull()
    const calibration = createBatchCalibration({
      activitySummary: summary,
      referenceVideo: { ...video, path: 'C:/reference.mp4', committedOffsetSeconds: -10 },
      timezoneMode: null,
    })
    expect(resolveBatchVideoTiming(video, summary, calibration)).toEqual({
      timing: { mode: 'externalActivity', automaticOffsetSeconds: -30, offsetSeconds: -10 },
      hasPositiveOverlap: true,
    })
    const noCorrection = createBatchCalibration({ activitySummary: summary, referenceVideo: null, timezoneMode: null })
    expect(resolveBatchVideoTiming({ ...video, creationTime: '2026-07-18T07:59:40Z' }, summary, noCorrection).hasPositiveOverlap).toBe(false)
  })

  test('rejects an unresolved reference baseline and uses local zero for per-video embedded telemetry', () => {
    expect(() =>
      createBatchCalibration({
        activitySummary: summary,
        referenceVideo: { path: 'C:/reference.mp4', creationTime: null, timeSource: null, committedOffsetSeconds: 5 },
        timezoneMode: 'utc',
      }),
    ).toThrow('Could not calibrate reference video')
    const calibration = createBatchCalibration({ activitySummary: null, referenceVideo: null, timezoneMode: 'utc' })
    expect(resolveBatchVideoTiming({ creationTime: null }, null, calibration)).toEqual({
      timing: { mode: 'embeddedActivity', offsetSeconds: 0 },
      hasPositiveOverlap: null,
    })
  })
})
