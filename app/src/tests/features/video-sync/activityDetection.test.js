import { describe, expect, test } from 'vitest'
import { createActivitySyncInput } from '@/features/video-sync/utils/activitySyncInput'
import { detectActivityEvents } from '@/features/video-sync/utils/detectActivityEvents'
import { detectStops } from '@/features/video-sync/utils/detectStops'
import { detectTurns, detectTurnsFromDerivedSeries, deriveTurningSeries } from '@/features/video-sync/utils/detectTurns'

const SETTINGS = { speedThresholdKmh: 5, turnThresholdDegrees: 90 }

function makeActivity({ sampleRate = 1, durationSeconds, speedAt = () => 3, headingAt = () => 0, course = true }) {
  const sampleCount = Math.round(durationSeconds * sampleRate)
  const elapsedSeconds = Array.from({ length: sampleCount + 1 }, (_, index) => index / sampleRate)
  return {
    sample_elapsed_seconds: elapsedSeconds,
    speed: elapsedSeconds.map(speedAt),
    heading: elapsedSeconds.map(headingAt),
    sample_course_points: course ? elapsedSeconds.map(() => [50, 14]) : [],
  }
}

function makeStopActivity(sampleRate) {
  return makeActivity({
    sampleRate,
    durationSeconds: 18,
    speedAt: (time) => {
      if (time < 4) return 0
      if (time < 7) return 0.5
      if (time < 11) return 3
      return 0.5
    },
  })
}

describe('manual video-sync activity detection', () => {
  test('detects equivalent stop and directional-turn times at 1 Hz and 40 Hz', () => {
    const makeCombinedActivity = (sampleRate) =>
      makeActivity({
        sampleRate,
        durationSeconds: 16,
        speedAt: (time) => {
          if (time < 4) return 3
          if (time < 5) return 3 - 2.5 * (time - 4)
          if (time < 8) return 0.5
          return 3
        },
        headingAt: (time) => Math.min(120, time * 30),
      })

    const oneHz = detectActivityEvents(makeCombinedActivity(1), SETTINGS)
    const fortyHz = detectActivityEvents(makeCombinedActivity(40), SETTINGS)

    expect(oneHz.stops).toHaveLength(1)
    expect(fortyHz.stops).toHaveLength(1)
    expect(oneHz.turns).toHaveLength(1)
    expect(fortyHz.turns).toHaveLength(1)
    expect(fortyHz.stops[0].time).toBeCloseTo(oneHz.stops[0].time, 1)
    expect(fortyHz.turns[0].representativeTime).toBeCloseTo(oneHz.turns[0].representativeTime, 0)
    expect(oneHz.turns[0].type).toBe('rightTurn')
  })

  test('uses dwell and hysteresis for one stop and ignores a stationary start', () => {
    const drift = detectStops(createActivitySyncInput(makeStopActivity(1)), SETTINGS)
    const stationaryStart = detectStops(
      createActivitySyncInput(
        makeActivity({
          sampleRate: 1,
          durationSeconds: 12,
          speedAt: (time) => (time < 4 ? 0.5 : 3),
        }),
      ),
      SETTINGS,
    )

    expect(drift).toHaveLength(1)
    expect(drift[0].time).toBeGreaterThan(3)
    expect(drift[0].lowSpeedInterval.end - drift[0].lowSpeedInterval.start).toBeGreaterThanOrEqual(2)
    expect(stationaryStart).toEqual([])
  })

  test('classifies wraparound and meaningful direction reversal', () => {
    const wraparound = detectActivityEvents(
      makeActivity({
        sampleRate: 1,
        durationSeconds: 6,
        headingAt: (time) => (350 + time * 35) % 360,
      }),
      SETTINGS,
    )
    const reversal = detectActivityEvents(
      makeActivity({
        sampleRate: 1,
        durationSeconds: 12,
        headingAt: (time) => {
          if (time <= 4) return time * 30
          return 120 - (time - 4) * 30
        },
      }),
      SETTINGS,
    )

    expect(wraparound.turns.map((event) => event.type)).toContain('rightTurn')
    expect(reversal.turns.map((event) => event.type)).toEqual(['rightTurn', 'leftTurn'])
  })

  test('keeps a turn candidate through a minor opposite-direction correction', () => {
    const input = {
      elapsedSeconds: [0, 1, 2, 3, 4, 5],
      segments: [{ startIndex: 0, endIndex: 6 }],
    }
    const turningSeries = [
      { time: 0, value: null },
      { time: 1, value: 40 },
      { time: 2, value: 40 },
      { time: 3, value: -15 },
      { time: 4, value: 25 },
      { time: 5, value: 0 },
    ]

    const derivedResult = detectTurnsFromDerivedSeries(input, turningSeries, { ...SETTINGS, nearStopIntervals: [] })

    expect(derivedResult).toHaveLength(1)
    expect(derivedResult[0].type).toBe('rightTurn')
  })

  test('rejects a gradual bend that exceeds the ten-second qualifying duration', () => {
    const result = detectActivityEvents(
      makeActivity({
        sampleRate: 1,
        durationSeconds: 25,
        headingAt: (time) => time * 8,
      }),
      SETTINGS,
    )

    expect(result.turns).toEqual([])
  })

  test('finds a qualifying turn in a sliding time window and ends it with the heading change', () => {
    const makeDelayedTurn = (sampleRate) =>
      makeActivity({
        sampleRate,
        durationSeconds: 20,
        headingAt: (time) => {
          if (time < 8) return time
          if (time < 11) return 8 + (time - 8) * 40
          return 128
        },
      })

    const oneHz = detectActivityEvents(makeDelayedTurn(1), SETTINGS)
    const fortyHz = detectActivityEvents(makeDelayedTurn(40), SETTINGS)

    expect(oneHz.turns).toHaveLength(1)
    expect(fortyHz.turns).toHaveLength(1)
    expect(oneHz.turns[0].type).toBe('rightTurn')
    expect(oneHz.turns[0].end).toBeLessThan(14)
    expect(fortyHz.turns[0].end).toBeLessThan(14)
  })

  test('does not bridge a significant gap and reports missing channel availability', () => {
    const activity = makeActivity({ sampleRate: 1, durationSeconds: 8, course: false })
    activity.sample_elapsed_seconds = [0, 1, 2, 3, 4, 14, 15, 16]
    activity.speed = [3, 3, 3, 3, 3, 0.5, 0.5, 0.5]
    activity.heading = [null, null, null, null, null, null, null, null]

    const input = createActivitySyncInput(activity)
    const result = detectActivityEvents(activity, SETTINGS)

    expect(input.segments).toEqual([
      { startIndex: 0, endIndex: 5 },
      { startIndex: 5, endIndex: 8 },
    ])
    expect(result.stops).toEqual([])
    expect(result.turns).toEqual([])
    expect(result.availability).toEqual({ speed: true, heading: false, course: false })
    expect(result.graphSeries.turning.every((point) => point.value === null)).toBe(true)
  })

  test('returns the same turning series for direct and combined detection', () => {
    const activity = makeActivity({ sampleRate: 10, durationSeconds: 6, headingAt: (time) => time * 30 })
    const input = createActivitySyncInput(activity)
    const directSeries = deriveTurningSeries(input)
    const combined = detectActivityEvents(activity, SETTINGS)
    const directTurns = detectTurns(input, { ...SETTINGS, nearStopIntervals: [] })

    expect(combined.graphSeries.turning).toEqual(directSeries)
    expect(combined.turns).toEqual(directTurns)
  })
})
