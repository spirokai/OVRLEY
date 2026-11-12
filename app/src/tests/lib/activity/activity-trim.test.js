import { describe, expect, test } from 'vite-plus/test'
import { resolveActivityWindow, trimActivityEdge } from '@/lib/activity/activity-trim'

describe('activity trim', () => {
  test('an untrimmed activity covers its whole duration', () => {
    expect(resolveActivityWindow(120, null)).toEqual({ startSecond: 0, endSecond: 120, isTrimmed: false })
  })

  test('a trim narrows the window', () => {
    expect(resolveActivityWindow(120, { startSecond: 10, endSecond: 90 })).toEqual({ startSecond: 10, endSecond: 90, isTrimmed: true })
  })

  test('trimming an edge keeps the other one', () => {
    expect(trimActivityEdge(null, 120, 'start', 15)).toEqual({ startSecond: 15, endSecond: 120 })
    expect(trimActivityEdge({ startSecond: 15, endSecond: 120 }, 120, 'end', 60)).toEqual({ startSecond: 15, endSecond: 60 })
  })

  test('edges stay inside the activity and at least a second apart', () => {
    expect(trimActivityEdge(null, 120, 'start', -5)).toBeNull()
    expect(trimActivityEdge({ startSecond: 10, endSecond: 20 }, 120, 'start', 19.9)).toEqual({ startSecond: 19, endSecond: 20 })
    expect(trimActivityEdge({ startSecond: 10, endSecond: 20 }, 120, 'end', 10.2)).toEqual({ startSecond: 10, endSecond: 11 })
    expect(trimActivityEdge({ startSecond: 10, endSecond: 20 }, 120, 'end', 500)).toEqual({ startSecond: 10, endSecond: 120 })
  })

  test('dragging both edges back out removes the trim', () => {
    expect(trimActivityEdge({ startSecond: 0, endSecond: 60 }, 120, 'end', 120)).toBeNull()
  })
})
