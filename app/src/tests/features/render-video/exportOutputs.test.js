import { describe, expect, test } from 'vite-plus/test'
import { reviewExport } from '@/features/render-video/utils/exportOutputs'
import { joinVideoEdits } from '@/lib/video-program/video-collection'
import { createTestVideo, videoStoreState } from '@/tests/helpers/videoFixtures'

const defaults = { type: 'wipeleft', durationSeconds: 1, splitGapThresholdSeconds: 2 }
const all = { type: 'all', from: 0, to: 0 }
const sync = { activitySummary: null, timezoneMode: 'utc', error: null }

function clipsOf(videos, offsets, edit = () => {}) {
  const { importedVideos, videoEdits } = videoStoreState(videos, offsets)
  edit(videoEdits)
  return joinVideoEdits(importedVideos, videoEdits)
}

function inspected(clips, failing = []) {
  return {
    complete: true,
    error: null,
    sources: new Map(
      clips.map((clip) => [
        clip.path,
        failing.includes(clip.id) ? { source: null, error: new Error('Unreadable') } : { source: { sourceId: `source-${clip.id}` }, error: null },
      ]),
    ),
  }
}

function review(overrides) {
  const clips = overrides.clips
  return reviewExport({
    outputMode: 'combined',
    exportMode: 'composite',
    exportRange: all,
    transitionDefaults: defaults,
    skipOverlay: {},
    inspection: inspected(clips),
    sync,
    locale: 'en',
    outputPath: 'C:/renders/joined.mp4',
    ...overrides,
  })
}

const ride = createTestVideo({ id: 'ride', path: '/videos/ride.mp4', durationSeconds: 60 })
const climb = createTestVideo({ id: 'climb', path: '/videos/climb.mp4', durationSeconds: 30, importOrder: 1 })

describe('reviewExport', () => {
  test('combines the program into one output joined by its transitions', () => {
    const clips = clipsOf([ride, climb], { ride: 0, climb: 70 })
    const { outputs, offsets, blockers } = review({ clips, skipOverlay: { climb: true } })
    expect(blockers).toEqual([])
    expect(outputs).toEqual([
      {
        id: 'C:/renders/joined.mp4',
        pieces: [
          { sourceId: 'source-ride', startFrame: 0, endFrame: 1800, skipOverlay: false },
          { sourceId: 'source-climb', startFrame: 0, endFrame: 900, skipOverlay: true },
        ],
        // A 10 s gap is above the continuity threshold.
        transitions: [{ type: 'wipeleft', durationSeconds: 1 }],
      },
    ])
    expect(offsets).toEqual({ 'source-ride': 0, 'source-climb': 70 })
    // Transparent output joins with hard cuts.
    expect(review({ clips, exportMode: 'transparent' }).outputs[0].transitions).toEqual([{ type: 'none', durationSeconds: 0 }])
  })

  test('renders separate files joined at their deleted parts', () => {
    const clips = clipsOf([ride, climb], { ride: 0, climb: 30 }, (edits) => {
      edits.ride.deletedParts = [{ id: 'part', fromSecond: 10, toSecond: 20, transition: { type: 'fade', durationSeconds: 0.5 } }]
    })
    // Overlapping videos render as separate files without trimming each other.
    const { outputs, blockers } = review({ clips, outputMode: 'separate' })
    expect(blockers).toEqual([])
    expect(outputs).toEqual([
      {
        id: '/videos/ride.mp4',
        pieces: [
          { sourceId: 'source-ride', startFrame: 0, endFrame: 300, skipOverlay: false },
          { sourceId: 'source-ride', startFrame: 600, endFrame: 1800, skipOverlay: false },
        ],
        transitions: [{ type: 'fade', durationSeconds: 0.5 }],
      },
      { id: '/videos/climb.mp4', pieces: [{ sourceId: 'source-climb', startFrame: 0, endFrame: 900, skipOverlay: false }], transitions: [] },
    ])
  })

  test('explains every reason one combined file cannot be rendered', () => {
    const wide = createTestVideo({ id: 'wide', path: '/videos/wide.mp4', durationSeconds: 30, width: 3840, height: 2160, importOrder: 2 })
    // Climb overlaps the end of ride and continues after it.
    const clips = clipsOf([ride, climb, wide], { ride: 0, climb: 40, wide: 200 })
    const { blockers, outputs } = review({ clips, inspection: inspected(clips, ['climb']) })
    expect(outputs).toBeNull()
    expect(blockers).toEqual([
      'Videos overlap (ride.mp4, climb.mp4). Move a video or delete the overlapping part.',
      'Videos differ in resolution or frame rate (wide.mp4). Remove them or export separate files.',
      'Some videos cannot be exported (climb.mp4).',
    ])
    expect(review({ clips: clipsOf([ride], { ride: 0 }), exportRange: { type: 'custom', from: 100, to: 120 } }).blockers).toEqual([
      'No video lies inside the export range.',
    ])
  })

  test('blocks transitions longer than the pieces they join', () => {
    const clips = clipsOf([ride, climb], { ride: 0, climb: 70 }, (edits) => {
      edits.climb.transitionIn = { type: 'fade', durationSeconds: 40 }
      edits.ride.deletedParts = [{ id: 'part', fromSecond: 10, toSecond: 20, transition: { type: 'fade', durationSeconds: 12 } }]
    })
    expect(review({ clips }).blockers).toEqual(['Some transitions are longer than the video pieces they join.'])
    const separate = review({ clips, outputMode: 'separate' })
    expect(separate.rows.find((row) => row.id === 'ride')).toMatchObject({
      status: 'blocked',
      error: 'A transition is longer than the video pieces it joins',
    })
    expect(separate.outputs.map((output) => output.id)).toEqual(['/videos/climb.mp4'])
  })

  test('builds no outputs while videos are still being inspected', () => {
    const clips = clipsOf([ride], { ride: 0 })
    expect(review({ clips, inspection: null }).outputs).toBeNull()
  })
})
