import { describe, expect, test } from 'vite-plus/test'

import { buildProgram, collectVideoPieces, findInvalidTransitions, resolveProgramPosition } from '@/lib/video-program/program'
import { DEFAULT_TRANSITION_SETTINGS, HARD_CUT } from '@/lib/video-program/transition-catalog'

const FPS = { num: 30, den: 1 }
const ALL = { type: 'all', from: 0, to: 0 }
const WIPE = { type: 'wipeleft', durationSeconds: 2 }
const WIPE_DEFAULTS = { ...DEFAULT_TRANSITION_SETTINGS, ...WIPE }

function video(id, { offsetSeconds = 0, durationSeconds = 60, importOrder = 0, deletedParts = [], transitionIn = null, fps = FPS } = {}) {
  const frameCount = Math.round((durationSeconds * fps.num) / fps.den)
  return { id, offsetSeconds, durationSeconds: (frameCount * fps.den) / fps.num, frameCount, fps, importOrder, deletedParts, transitionIn }
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
      videos: [video('a'), video('b', { offsetSeconds: 70, durationSeconds: 4, importOrder: 1 })],
      transition: HARD_CUT,
      duration: 64,
    },
  ])('$name', ({ videos, defaults, transition, duration }) => {
    const result = program(videos, { transitionDefaults: defaults })
    expect(result.joins).toHaveLength(1)
    expect(result.joins[0]).toMatchObject({ key: 'clip:b', kind: 'clip', ownerId: 'b', transition, isDefault: true })
    expect(result.durationSeconds).toBeCloseTo(duration, 9)
  })

  test('a stored override wins over defaults and changes the output duration (Σ pieces − Σ transitions)', () => {
    const fade = { type: 'fade', durationSeconds: 3 }
    const result = program([video('a'), video('b', { offsetSeconds: 60, importOrder: 1, transitionIn: fade })])
    expect(result.joins[0]).toMatchObject({ transition: fade, isDefault: false, gapSeconds: 0 })
    expect(result.durationSeconds).toBe(117)
  })

  test('changing defaults re-derives only joins still on their default', () => {
    const videos = [
      video('a'),
      video('b', { offsetSeconds: 70, importOrder: 1, transitionIn: { type: 'fade', durationSeconds: 1 } }),
      video('c', { offsetSeconds: 140, importOrder: 2 }),
    ]
    const result = program(videos, { transitionDefaults: { ...WIPE_DEFAULTS, type: 'dissolve', durationSeconds: 3 } })
    expect(result.joins.map(({ transition }) => transition)).toEqual([
      { type: 'fade', durationSeconds: 1 },
      { type: 'dissolve', durationSeconds: 3 },
    ])
  })

  test('default durations snap to the frames of the pieces they join, as the export counts them', () => {
    const ntsc = { num: 30000, den: 1001 }
    const videos = [video('a', { fps: ntsc }), video('b', { offsetSeconds: 70, importOrder: 1, fps: ntsc })]
    const result = program(videos, { transitionDefaults: { ...WIPE_DEFAULTS, durationSeconds: 1.01 } })
    expect(result.joins[0].transition).toEqual({ type: 'wipeleft', durationSeconds: (30 * 1001) / 30000 })
  })
})

describe('buildProgram deleted parts', () => {
  test('a deleted middle part splits one video into two pieces joined at the deleted part', () => {
    const result = program([video('a', { durationSeconds: 1200, deletedParts: [{ id: 'd1', fromSecond: 300, toSecond: 900, transition: null }] })])
    expect(pieceSpans(result)).toEqual([
      ['a', 0, 300, 0, 300],
      ['a', 900, 1200, 900, 1200],
    ])
    expect(result.joins[0]).toMatchObject({ key: 'deletedPart:d1', kind: 'deletedPart', ownerId: 'd1', gapSeconds: 600, transition: WIPE })
    expect(result.durationSeconds).toBe(598)
  })

  test('a short deleted part defaults to a hard cut and a stored transition overrides it', () => {
    const parts = [
      { id: 'd1', fromSecond: 90, toSecond: 91, transition: null },
      { id: 'd2', fromSecond: 200, toSecond: 210, transition: HARD_CUT },
    ]
    const result = program([video('a', { durationSeconds: 300, deletedParts: parts })])
    expect(result.joins.map(({ key, transition, isDefault }) => [key, transition, isDefault])).toEqual([
      ['deletedPart:d1', HARD_CUT, true],
      ['deletedPart:d2', HARD_CUT, false],
    ])
    expect(result.durationSeconds).toBe(289)
  })

  test('leading and trailing deleted parts trim without creating joins', () => {
    const parts = [
      { id: 'head', fromSecond: 0, toSecond: 5, transition: null },
      { id: 'tail', fromSecond: 55, toSecond: 60, transition: null },
    ]
    const result = program([video('a', { offsetSeconds: 10, deletedParts: parts })])
    expect(pieceSpans(result)).toEqual([['a', 5, 55, 15, 65]])
    expect(result.joins).toEqual([])
  })

  test('join keys stay unique when another clip sits inside a deleted part', () => {
    const result = program([
      video('b', { durationSeconds: 20, deletedParts: [{ id: 'd1', fromSecond: 5, toSecond: 15, transition: null }] }),
      video('a', { offsetSeconds: 8, durationSeconds: 4, importOrder: 1 }),
    ])
    expect(result.pieces.map(({ videoId }) => videoId)).toEqual(['b', 'a', 'b'])
    expect(result.joins.map(({ key }) => key)).toEqual(['clip:a', 'deletedPart:d1'])
    expect(result.overlaps).toEqual([])
  })

  test('deleted part boundaries snap to the source frame grid', () => {
    const ntsc = { num: 30000, den: 1001 }
    const result = program([
      video('a', { durationSeconds: 10.01, fps: ntsc, deletedParts: [{ id: 'd1', fromSecond: 1.0, toSecond: 2.0, transition: null }] }),
    ])
    expect(result.pieces.map(({ sourceStartFrame, sourceEndFrame }) => [sourceStartFrame, sourceEndFrame])).toEqual([
      [0, 30],
      [60, 300],
    ])
  })
})

describe('buildProgram export range', () => {
  test('a custom range trims pieces and its boundaries never create joins', () => {
    const result = program([video('a'), video('b', { offsetSeconds: 60, importOrder: 1 })], { exportRange: { type: 'custom', from: 30, to: 90 } })
    expect(pieceSpans(result)).toEqual([
      ['a', 30, 60, 30, 60],
      ['b', 0, 30, 60, 90],
    ])
    expect(result.joins.map(({ key }) => key)).toEqual(['clip:b'])
  })

  test('a range starting inside a deleted part leaves only the later piece', () => {
    const result = program([video('a', { deletedParts: [{ id: 'd1', fromSecond: 10, toSecond: 20, transition: null }] })], {
      exportRange: { type: 'custom', from: 15, to: 40 },
    })
    expect(pieceSpans(result)).toEqual([['a', 20, 40, 20, 40]])
    expect(result.joins).toEqual([])
  })

  test('videos outside the range are excluded', () => {
    const result = program([video('a'), video('b', { offsetSeconds: 200, importOrder: 1 })], { exportRange: { type: 'custom', from: 0, to: 100 } })
    expect(result.pieces.map(({ videoId }) => videoId)).toEqual(['a'])
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
    expect(result.joins[0]).toMatchObject({ key: 'clip:b', gapSeconds: 0, transition: HARD_CUT })
  })

  test('a fully covered clip is dropped from playback but still reported', () => {
    const result = program([video('a'), video('b', { offsetSeconds: 10, durationSeconds: 20, importOrder: 1 })])
    expect(result.overlaps).toEqual([{ earlierVideoId: 'a', laterVideoId: 'b', startSecond: 10, endSecond: 30 }])
    expect(result.pieces.map(({ videoId }) => videoId)).toEqual(['a'])
  })

  test('deleting the overlapping part resolves the overlap', () => {
    const result = program([
      video('a', { deletedParts: [{ id: 'd1', fromSecond: 50, toSecond: 60, transition: null }] }),
      video('b', { offsetSeconds: 50, importOrder: 1 }),
    ])
    expect(result.overlaps).toEqual([])
  })

  test('touching clips with floating-point offsets are not overlaps', () => {
    const result = program([video('a', { offsetSeconds: 0.1 }), video('b', { offsetSeconds: 0.1 + 60 - 1e-9, importOrder: 1 })])
    expect(result.overlaps).toEqual([])
  })
})

describe('findInvalidTransitions', () => {
  test('flags pieces whose incoming and outgoing transitions leave no untouched frame', () => {
    const three = { type: 'fade', durationSeconds: 3 }
    const result = program([
      video('a'),
      video('b', { offsetSeconds: 60, durationSeconds: 5, importOrder: 1, transitionIn: three }),
      video('c', { offsetSeconds: 65, importOrder: 2, transitionIn: three }),
    ])
    expect(findInvalidTransitions(result)).toEqual([{ pieceId: result.pieces[1].id, joinKeys: ['clip:b', 'clip:c'] }])
  })

  test('accepts transitions that fit', () => {
    expect(findInvalidTransitions(program([video('a'), video('b', { offsetSeconds: 70, importOrder: 1 })]))).toEqual([])
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

  test('an empty program has nothing to show', () => {
    expect(resolveProgramPosition(program([]), 0)).toEqual({ kind: 'empty' })
  })
})

describe('collectVideoPieces for separate-file export', () => {
  test('keeps each video whole across overlaps, splits at deleted parts, and applies the export range', () => {
    const videos = [
      video('a', { durationSeconds: 60, deletedParts: [{ id: 'cut', fromSecond: 10, toSecond: 20, transition: null }] }),
      video('b', { offsetSeconds: 50, durationSeconds: 30, importOrder: 1 }),
      video('c', { offsetSeconds: 200, durationSeconds: 10, importOrder: 2 }),
    ]
    const spans = (pieces) => pieces.map(({ sourceStartFrame, sourceEndFrame }) => [sourceStartFrame, sourceEndFrame])

    const all = collectVideoPieces({ videos, exportRange: ALL })
    expect(spans(all.get('a'))).toEqual([
      [0, 300],
      [600, 1800],
    ])
    expect(spans(all.get('b'))).toEqual([[0, 900]])

    const ranged = collectVideoPieces({ videos, exportRange: { type: 'custom', from: 15, to: 70 } })
    expect(spans(ranged.get('a'))).toEqual([[600, 1800]])
    expect(spans(ranged.get('b'))).toEqual([[0, 600]])
    expect(ranged.has('c')).toBe(false)
  })
})
