import { describe, expect, test } from 'vitest'
import { formatSecondsText, normalizeSecondsDraft, parseSecondsDraft, snapSeconds } from '@/lib/seconds-input'
import { formatSecondsClock } from '@/lib/time-format'

const PAL = { num: 25, den: 1 }
const NTSC = { num: 30000, den: 1001 }

describe('seconds input helpers', () => {
  test('accept only a number being typed, with a sign only where allowed', () => {
    for (const draft of ['', '3', '330', '330.', '330.5', '.5']) expect(normalizeSecondsDraft(draft, false)).toBe(draft)
    expect(normalizeSecondsDraft('330,5', false)).toBe('330.5')
    for (const text of ['1:30', '5garbage', 'abc', '1.2.3', '-5', '1e3', ' 5']) expect(normalizeSecondsDraft(text, false)).toBeNull()
    expect(normalizeSecondsDraft('-', true)).toBe('-')
    expect(normalizeSecondsDraft('-12.5', true)).toBe('-12.5')
    expect(normalizeSecondsDraft('1-2', true)).toBeNull()
  })

  test('parse complete numbers only', () => {
    expect(parseSecondsDraft('330')).toBe(330)
    expect(parseSecondsDraft('330.')).toBe(330)
    expect(parseSecondsDraft('.5')).toBe(0.5)
    expect(parseSecondsDraft('-12.5')).toBe(-12.5)
    for (const draft of ['', '-', '.', '-.']) expect(parseSecondsDraft(draft)).toBeNull()
  })

  test('show up to three decimals without trailing zeros', () => {
    expect(formatSecondsText(330)).toBe('330')
    expect(formatSecondsText(300.04)).toBe('300.04')
    expect(formatSecondsText(1 / 30)).toBe('0.033')
    expect(formatSecondsText(-0.0001)).toBe('0')
  })

  test('snap times to the nearest frame and durations to at least one frame', () => {
    expect(snapSeconds(300.03, PAL, 'time')).toBe(300.04)
    expect(snapSeconds(0.01, PAL, 'time')).toBe(0)
    expect(snapSeconds(0.01, PAL, 'duration')).toBe(0.04)
    expect(snapSeconds(2, NTSC, 'duration')).toBe((60 * 1001) / 30000)
    expect(snapSeconds(-1.01, PAL, 'time')).toBe(-1)
    expect(snapSeconds(2.345, null, 'time')).toBe(2.345)
  })

  test('read seconds as a clock, hours only from one hour on', () => {
    expect(formatSecondsClock(330)).toBe('05:30')
    expect(formatSecondsClock(3700)).toBe('01:01:40')
    expect(formatSecondsClock(-330)).toBe('-05:30')
    expect(formatSecondsClock(330.5)).toBe('05:30.5')
    expect(formatSecondsClock(300.04)).toBe('05:00.04')
    expect(formatSecondsClock(1 / 30)).toBe('00:00.033')
    expect(formatSecondsClock(0)).toBe('00:00')
  })
})
