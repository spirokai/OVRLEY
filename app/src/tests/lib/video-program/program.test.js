import { describe, expect, test } from 'vite-plus/test'

import { buildProgram, findInvalidTransitions, resolveProgramPosition } from '@/lib/video-program/program'
import { DEFAULT_TRANSITION_SETTINGS, HARD_CUT } from '@/lib/video-program/transition-catalog'
import { cutEdits } from '@/tests/helpers/videoFixtures'

const FPS = { num: 30, den: 1 }
const NTSC = { num: 30000, den: 1001 }
const ALL = { type: 'all', from: 0, to: 0 }
const WIPE = { type: 'wipeleft', durationSeconds: 2 }
const WIPE_DEFAULTS = { ...DEFAULT_TRANSITION_SETTINGS, ...WIPE }

function video(id, { offsetSeconds = 0, durationSeconds = 60, importOrder = 0, cuts = [], start = {}, fps = FPS } = {}) {
  const frameCount = Math.round((durationSeconds * fps.num) / fps.den)
  return { id, offsetSeconds, durationSeconds: (frameCount * fps.den) / fps.num, frameCount, fps, importOrder, ...cutEdits(cuts, start) }
}

function program(videos, { exportRange = ALL, transitionDefaults = WIPE_DEFAULTS } = {}) {
  return buildProgram({ videos, exportRange, transitionDefaults })
}

function pieceSpans(result) {
  return result.pieces.map(({ videoId, sourceStartSecond, sourceEndSecond, timelineStartSecond, timelineEndSecond }) => [
    videoId,
    sourceStartSecond,
    sourceEndSecond,
    timelineStartSecond,
    timelineEndSecond,
  ])
}

describe('buildProgram joins and default transitions', () => {
  test.each([
    {
      name: 'camera split without gap is a hard cut',
      videos: [video('a'), video('b', { offsetSeconds: 60, importOrder: 1 })],
      transition: HARD_CUT,
      duration: 120,
    },
    {
      name: 'gap at the split threshold is a hard cut',
      videos: [video('a'), video('b', { offsetSeconds: 62, importOrder: 1 })],
      transition: HARD_CUT,
      duration: 120,
    },
    {
      name: 'gap above the threshold gets the default transition',
      videos: [video('a'), video('b', { offsetSeconds: 70, importOrder: 1 })],
      transition: WIPE,
      duration: 118,
    },
    {
      name: '"No transition", the hard default, keeps hard cuts',
      videos: [video('a'), video('b', { offsetSeconds: 70, importOrder: 1 })],
      defaults: DEFAULT_TRANSITION_SETTINGS,
      transition: HARD_CUT,
      duration: 120,
    },
    {
      name: 'a larger threshold treats the gap as continuous',
      videos: [video('a'), video('b', { offsetSeconds: 70, importOrder: 1 })],
      defaults: { ...WIPE_DEFAULTS, splitGapThresholdSeconds: 15 },
      transition: HARD_CUT,
      duration: 120,
    },
    {
      name: 'default that does not fit a short piece falls back to a hard cut',
      videos: [video('a'), video('b', { offsetSeconds: 70, durationSeconds: 2, importOrder: 1 })],
      transition: HARD_CUT,
      duration: 62,
    },
  ])('$name', ({ videos, defaults, transition, duration }) => {
    const result = program(videos, { transitionDefaults: defaults })
    expect(result.joins).toHaveLength(1)
    expect(result.joins[0]).toMatchObject({ key: 'b:start', videoId: 'b', segmentKey: 'start', transition, isDefault: true })
    expect(result.durationSeconds).toBeCloseTo(duration, 9)
  })

  test('a stored override wins over defaults and changes the output duration (Σ pieces − Σ transitions)', () => {
    const fade = { type: 'fade', durationSeconds: 3 }
    const result = program([video('a'), video('b', { offsetSeconds: 60, importOrder: 1, start: { transitionIn: fade } })])
    expect(result.joins[0]).toMatchObject({ transition: fade, isDefault: false, gapSeconds: 0 })
    expect(result.durationSeconds).toBe(117)
  })

  test('default durations snap to the frames of the pieces they join, as the export counts them', () => {
    const videos = [video('a', { fps: NTSC }), video('b', { offsetSeconds: 70, importOrder: 1, fps: NTSC })]
    const result = program(videos, { transitionDefaults: { ...WIPE_DEFAULTS, durationSeconds: 1.01 } })
    expect(result.joins[0].transition).toEqual({ type: 'wipeleft', durationSeconds: (30 * 1001) / 30000 })
  })
})

describe('buildProgram cuts and excluded segments', () => {
  test('an excluded middle segment leaves two pieces joined by the transition of the segment after it', () => {
    const cuts = [
      { id: 'c1', frame: 9000, excluded: true },
      { id: 'c2', frame: 27000 },
    ]
    const result = program([video('a', { durationSeconds: 1200, cuts })])
    expect(pieceSpans(result)).toEqual([
      ['a', 0, 300, 0, 300],
      ['a', 900, 1200, 900, 1200],
    ])
    expect(result.joins).toHaveLength(1)
    expect(result.joins[0]).toMatchObject({ key: 'a:c2', segmentKey: 'c2', gapSeconds: 600, transition: WIPE })
    expect(result.durationSeconds).toBe(598)
    expect(
      result.excludedSegments.map(({ videoId, segmentKey, timelineStartSecond, timelineEndSecond }) => [
        videoId,
        segmentKey,
        timelineStartSecond,
        timelineEndSecond,
      ]),
    ).toEqual([['a', 'c1', 300, 900]])
  })

  test('excluded segments at the edges trim without creating joins', () => {
    const cuts = [
      { id: 'c1', frame: 150 },
      { id: 'c2', frame: 1650, excluded: true },
    ]
    const result = program([video('a', { offsetSeconds: 10, cuts, start: { excluded: true } })])
    expect(pieceSpans(result)).toEqual([['a', 5, 55, 15, 65]])
    expect(result.joins).toEqual([])
  })

  test('excluding a whole video leaves no piece, keeps it listed, and joins its neighbours', () => {
    const result = program([
      video('a'),
      video('b', { offsetSeconds: 60, durationSeconds: 10, importOrder: 1, start: { excluded: true } }),
      video('c', { offsetSeconds: 70, importOrder: 2 }),
    ])
    expect(result.pieces.map(({ videoId }) => videoId)).toEqual(['a', 'c'])
    expect(result.joins.map(({ key, gapSeconds, transition }) => [key, gapSeconds, transition])).toEqual([['c:start', 10, WIPE]])
    expect(result.excludedSegments.map(({ videoId, segmentKey }) => [videoId, segmentKey])).toEqual([['b', 'start']])
  })

  test('the join into a video whose first segment is excluded belongs to its first kept segment', () => {
    const result = program([
      video('a'),
      video('b', {
        offsetSeconds: 60,
        importOrder: 1,
        start: { excluded: true, transitionIn: { type: 'fade', durationSeconds: 1 } },
        cuts: [{ id: 'c1', frame: 300 }],
      }),
    ])
    expect(result.joins).toHaveLength(1)
    expect(result.joins[0]).toMatchObject({ key: 'b:c1', isDefault: true })
  })

  test('join keys stay unique when another clip sits inside an excluded segment', () => {
    const cuts = [
      { id: 'c1', frame: 150, excluded: true },
      { id: 'c2', frame: 450 },
    ]
    const result = program([video('b', { durationSeconds: 20, cuts }), video('a', { offsetSeconds: 8, durationSeconds: 4, importOrder: 1 })])
    expect(result.pieces.map(({ videoId }) => videoId)).toEqual(['b', 'a', 'b'])
    expect(result.joins.map(({ key }) => key)).toEqual(['a:start', 'b:c2'])
    expect(result.overlaps).toEqual([])
  })
})

describe('buildProgram export range', () => {
  test('a custom range trims pieces and its boundaries never create joins', () => {
    const result = program([video('a'), video('b', { offsetSeconds: 60, importOrder: 1 })], { exportRange: { type: 'custom', from: 30, to: 90 } })
    expect(pieceSpans(result)).toEqual([
      ['a', 30, 60, 30, 60],
      ['b', 0, 30, 60, 90],
    ])
    expect(result.joins.map(({ key }) => key)).toEqual(['b:start'])
  })
})

describe('buildProgram overlaps', () => {
  test('the earlier clip plays to its end and the later clip continues at the matching position', () => {
    const result = program([video('a'), video('b', { offsetSeconds: 50, importOrder: 1 })])
    expect(result.overlaps).toEqual([{ earlierVideoId: 'a', laterVideoId: 'b', startSecond: 50, endSecond: 60 }])
    expect(pieceSpans(result)).toEqual([
      ['a', 0, 60, 0, 60],
      ['b', 10, 60, 60, 110],
    ])
    expect(result.joins[0]).toMatchObject({ key: 'b:start', gapSeconds: 0, transition: HARD_CUT })
  })

  test('excluding the overlapping segment resolves the overlap', () => {
    const result = program([video('a', { cuts: [{ id: 'c1', frame: 1500, excluded: true }] }), video('b', { offsetSeconds: 50, importOrder: 1 })])
    expect(result.overlaps).toEqual([])
  })

  test('a clip snapped onto the end of a 29.97 fps clip does not overlap it, and a whole frame does', () => {
    const first = video('a', { durationSeconds: 10.01, fps: NTSC })
    const end = first.durationSeconds
    const snapped = program([first, video('b', { offsetSeconds: end, importOrder: 1, fps: NTSC })])
    expect(snapped.overlaps).toEqual([])
    const subFrame = program([first, video('b', { offsetSeconds: end - 0.01, importOrder: 1, fps: NTSC })])
    expect(subFrame.overlaps).toEqual([])
    const wholeFrame = program([first, video('b', { offsetSeconds: end - 1001 / 30000, importOrder: 1, fps: NTSC })])
    expect(wholeFrame.overlaps).toHaveLength(1)
  })
})

describe('findInvalidTransitions', () => {
  test('flags pieces whose incoming and outgoing transitions leave no untouched frame', () => {
    const three = { type: 'fade', durationSeconds: 3 }
    const result = program([
      video('a'),
      video('b', { offsetSeconds: 60, durationSeconds: 5, importOrder: 1, start: { transitionIn: three } }),
      video('c', { offsetSeconds: 65, importOrder: 2, start: { transitionIn: three } }),
    ])
    expect(findInvalidTransitions(result)).toEqual([{ pieceId: result.pieces[1].id, joinKeys: ['b:start', 'c:start'] }])
  })
})

describe('resolveProgramPosition', () => {
  const result = program([video('a'), video('b', { offsetSeconds: 70, importOrder: 1 })])
  const [first, second] = result.pieces

  test.each([
    [-1, { kind: 'before', nextPiece: first }],
    [10, { kind: 'piece', piece: first, sourceSecond: 10, nextPiece: second, join: result.joins[0] }],
    [65, { kind: 'gap', previousPiece: first, nextPiece: second, join: result.joins[0] }],
    [75, { kind: 'piece', piece: second, sourceSecond: 5, nextPiece: null, join: null }],
    [130, { kind: 'after', previousPiece: second }],
  ])('at %s', (secondValue, expected) => {
    expect(resolveProgramPosition(result, secondValue)).toEqual(expected)
  })

  test('inside the outgoing transition window the incoming source advances from its start', () => {
    expect(resolveProgramPosition(result, 59.5)).toEqual({
      kind: 'transition',
      piece: first,
      sourceSecond: 59.5,
      nextPiece: second,
      join: result.joins[0],
      progress: 0.75,
      incomingSourceSecond: 1.5,
    })
  })
})
