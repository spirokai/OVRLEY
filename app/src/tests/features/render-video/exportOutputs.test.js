import { describe, expect, test } from 'vite-plus/test'
import { reviewExport } from '@/features/render-video/utils/exportOutputs'
import { joinVideoEdits } from '@/lib/video-program/video-collection'
import { createTestVideo, cutEdits, videoStoreState } from '@/tests/helpers/videoFixtures'

const defaults = { type: 'wipeleft', durationSeconds: 1, splitGapThresholdSeconds: 2 }
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
    codec: 'libx264',
    transitionDefaults: defaults,
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
  test('combines the program into one output joined by its transitions, each piece keeping its overlay switch', () => {
    const clips = clipsOf([ride, climb], { ride: 0, climb: 70 }).map((clip) =>
      clip.id === 'climb' ? { ...clip, segments: { start: { ...clip.segments.start, skipOverlay: true } } } : clip,
    )
    const { outputs, offsets, blockers } = review({ clips })
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

  test('renders separate files joined across their excluded segments', () => {
    const clips = clipsOf([ride, climb], { ride: 0, climb: 30 }, (edits) => {
      Object.assign(
        edits.ride,
        cutEdits([
          { id: 'c1', frame: 300, excluded: true },
          { id: 'c2', frame: 600, transitionIn: { type: 'fade', durationSeconds: 0.5 } },
        ]),
      )
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

  test('an encoder that keeps the display rotation joins only videos of one rotation', () => {
    // Coded portrait and rotated: the same display resolution as ride.
    const turned = createTestVideo({
      id: 'turned',
      path: '/videos/turned.mp4',
      durationSeconds: 30,
      width: 1080,
      height: 1920,
      rotationDegrees: 90,
      importOrder: 1,
    })
    const clips = clipsOf([ride, turned], { ride: 0, turned: 70 })
    expect(review({ clips }).blockers).toEqual([])
    expect(review({ clips, codec: 'qsv_full_h264' }).blockers).toEqual([
      'Videos are rotated differently (ride.mp4, turned.mp4), which this encoder cannot join into one file. Export separate files.',
    ])
    expect(review({ clips, codec: 'qsv_full_h264', outputMode: 'separate' }).blockers).toEqual([])
  })

  test('explains every reason one combined file cannot be rendered', () => {
    const wide = createTestVideo({ id: 'wide', path: '/videos/wide.mp4', durationSeconds: 30, width: 3840, height: 2160, importOrder: 2 })
    // Climb overlaps the end of ride and continues after it.
    const clips = clipsOf([ride, climb, wide], { ride: 0, climb: 40, wide: 200 })
    const { blockers, outputs } = review({ clips, inspection: inspected(clips, ['climb']) })
    expect(outputs).toBeNull()
    expect(blockers).toEqual([
      'Videos overlap (ride.mp4, climb.mp4). Move a video or exclude the overlapping part from export.',
      'Videos differ in resolution or frame rate (wide.mp4). Remove them or export separate files.',
      'Some videos cannot be exported (climb.mp4).',
    ])
    const excluded = clipsOf([ride], { ride: 0 }, (edits) => {
      edits.ride.segments.start.excluded = true
    })
    expect(review({ clips: excluded }).blockers).toEqual(['Every video part is excluded from export.'])
  })

  test('a video wholly behind an earlier overlapping video has nothing of its own to render', () => {
    const { rows } = review({ clips: clipsOf([ride, climb], { ride: 0, climb: 10 }) })
    expect(rows.find((row) => row.id === 'climb')).toMatchObject({ status: 'blocked', error: 'Hidden behind an overlapping video' })
  })

  test('blocks transitions longer than the pieces they join', () => {
    const clips = clipsOf([ride, climb], { ride: 0, climb: 70 }, (edits) => {
      edits.climb.segments.start.transitionIn = { type: 'fade', durationSeconds: 40 }
      Object.assign(
        edits.ride,
        cutEdits([
          { id: 'c1', frame: 300, excluded: true },
          { id: 'c2', frame: 600, transitionIn: { type: 'fade', durationSeconds: 12 } },
        ]),
      )
    })
    expect(review({ clips }).blockers).toEqual(['Some transitions are longer than the video pieces they join.'])
    const separate = review({ clips, outputMode: 'separate' })
    expect(separate.rows.find((row) => row.id === 'ride')).toMatchObject({
      status: 'blocked',
      error: 'A transition is longer than the video pieces it joins',
    })
    expect(separate.outputs.map((output) => output.id)).toEqual(['/videos/climb.mp4'])
  })

  test('a video excluded as a whole renders nothing and says so', () => {
    const clips = clipsOf([ride, climb], { ride: 0, climb: 70 }, (edits) => {
      edits.climb.segments.start.excluded = true
    })
    const { rows, outputs } = review({ clips, outputMode: 'separate' })
    expect(rows.find((row) => row.id === 'climb')).toMatchObject({ status: 'blocked', error: 'Excluded from export' })
    expect(outputs.map((output) => output.id)).toEqual(['/videos/ride.mp4'])
    expect(review({ clips }).outputs[0].pieces.map((piece) => piece.sourceId)).toEqual(['source-ride'])
  })

  test('builds no outputs while videos are still being inspected', () => {
    const clips = clipsOf([ride], { ride: 0 })
    expect(review({ clips, inspection: null }).outputs).toBeNull()
  })
})
