import { describe, expect, test } from 'vite-plus/test'
import { hasActivityCoverage, resolveActivityContext, resolveAvailableMetrics } from '@/lib/activity/activity-context'
import { buildTimelineProgram } from '@/lib/video-program/timeline'
import { DEFAULT_TRANSITION_SETTINGS } from '@/lib/video-program/transition-catalog'
import { createTestVideo, videoStoreState } from '@/tests/helpers/videoFixtures'

const telemetry = { metadata: { duration_seconds: 20 }, sample_elapsed_seconds: [0, 20] }

function timeline(videos, offsets) {
  const { importedVideos, videoEdits } = videoStoreState(videos, offsets)
  return buildTimelineProgram({ videos: importedVideos, videoEdits, dragPreview: null, transitionDefaults: DEFAULT_TRANSITION_SETTINGS })
}

describe('hasActivityCoverage', () => {
  test('an external activity covers exactly its own duration', () => {
    const { clips, program } = timeline([createTestVideo({ durationSeconds: 60 })], { 'video-1': -10 })
    const covered = (second) => hasActivityCoverage({ parsedActivity: {}, activitySummary: { durationSeconds: 30 }, program, clips, second })
    expect([covered(-5), covered(0), covered(30), covered(31)]).toEqual([false, true, true, false])
  })

  test('without an external activity the clip at the playhead covers its own telemetry time', () => {
    const { clips, program } = timeline(
      [
        createTestVideo({ id: 'with', durationSeconds: 30, telemetry: { status: 'ready', activity: telemetry } }),
        createTestVideo({
          id: 'without',
          path: '/videos/b.mp4',
          durationSeconds: 30,
          importOrder: 1,
          telemetry: { status: 'absent', activity: null },
        }),
      ],
      { with: 0, without: 40 },
    )
    const covered = (second) => hasActivityCoverage({ parsedActivity: null, activitySummary: null, program, clips, second })
    expect(covered(10)).toBe(true)
    expect(covered(25)).toBe(false) // past the telemetry end of its clip
    expect(covered(35)).toBe(false) // gap
    expect(covered(50)).toBe(false) // clip without telemetry
  })

  test('counts everything as covered without any activity or clip', () => {
    expect(hasActivityCoverage({ parsedActivity: null, activitySummary: null, program: { pieces: [], joins: [] }, clips: [], second: 5 })).toBe(true)
  })
})

function clipTelemetry(attributes = ['speed']) {
  return {
    coverage: Object.fromEntries(attributes.map((attribute) => [attribute, { source: 'direct' }])),
    extended_attributes: [],
    metadata: { duration_seconds: 120, sample_count: 240 },
    file_format: 'mp4-telemetry',
    lap_start_elapsed_seconds: [],
    lap_durations_seconds: [],
    valid_attributes: attributes,
  }
}

describe('resolveActivityContext', () => {
  test('an external activity drives every timeline second from zero', () => {
    const activityFile = { metadata: { duration_seconds: 120 } }
    const context = resolveActivityContext({ parsedActivity: activityFile, activitySummary: { durationSeconds: 120 }, clip: null })
    expect(context).toEqual({ activity: activityFile, summary: { durationSeconds: 120 }, originSecond: 0, videoId: null })
  })

  test('without an external activity the clip supplies its own telemetry from its start', () => {
    const activity = clipTelemetry()
    const clip = { ...createTestVideo({ telemetry: { status: 'ready', activity } }), offsetSeconds: 300 }
    const context = resolveActivityContext({ parsedActivity: null, activitySummary: null, clip })
    expect(context).toMatchObject({ activity, originSecond: 300, videoId: clip.id })
    expect(context.summary.availableMetrics).toEqual([{ attribute: 'speed', source: 'direct' }])

    const pending = { ...createTestVideo(), offsetSeconds: 0 }
    expect(resolveActivityContext({ parsedActivity: null, activitySummary: null, clip: pending })).toMatchObject({ activity: null, summary: null })
    expect(resolveActivityContext({ parsedActivity: null, activitySummary: null, clip: null })).toBeNull()
  })

  test('available metrics come from the external activity or from every clip telemetry', () => {
    const videos = [
      createTestVideo({ id: 'a', telemetry: { status: 'ready', activity: clipTelemetry(['speed']) } }),
      createTestVideo({ id: 'b', telemetry: { status: 'ready', activity: clipTelemetry(['speed', 'cadence']) } }),
      createTestVideo({ id: 'c' }),
    ]
    expect(resolveAvailableMetrics(null, videos).map(({ attribute }) => attribute)).toEqual(['speed', 'cadence'])
    const externalSummary = { availableMetrics: [{ attribute: 'heartrate', source: 'direct' }] }
    expect(resolveAvailableMetrics(externalSummary, videos)).toBe(externalSummary.availableMetrics)
  })
})
