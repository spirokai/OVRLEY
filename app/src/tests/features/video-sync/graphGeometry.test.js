import { describe, expect, test } from 'vitest'
import { buildEventBands, buildGraphGeometry, buildGraphScales, decimateGraphSamples } from '@/features/video-sync/utils/graphGeometry'

function points(values, start = 0) {
  return values.map((value, index) => ({ time: start + index, value }))
}

describe('manual video-sync timeline graph geometry', () => {
  test('uses fixed zero-based speed and symmetric turning scales', () => {
    const scales = buildGraphScales({
      speed: points([2, 4, 6, 100]),
      turning: points([-8, -2, 3, 12]),
    })

    expect(scales.speed.min).toBe(0)
    expect(scales.speed.max).toBeGreaterThan(6)
    expect(scales.turning.min).toBe(-scales.turning.max)
    expect(scales.turning.max).toBeGreaterThan(8)
  })

  test('keeps a brief minimum and maximum when samples share a pixel bucket', () => {
    const samples = points([4, 4, 0.5, 4, 9, 4])
    const decimated = decimateGraphSamples({ samples, viewStart: 0, viewEnd: 6, widthPx: 1 })

    expect(decimated.map((sample) => sample.value)).toEqual([0.5, 9])
  })

  test('uses the shared time transform for graph path coordinates and preserves gaps', () => {
    const graph = buildGraphGeometry({
      graphSeries: {
        speed: [
          { time: 0, value: 0 },
          { time: 5, value: 5 },
          { time: 10, value: null },
          { time: 15, value: 10 },
        ],
        turning: [],
      },
      viewEnd: 20,
      viewStart: 0,
      widthPx: 400,
    })

    expect(graph.paths.speed).toContain('M 0 64')
    expect(graph.paths.speed).toContain('L 100')
    expect(graph.paths.speed).toContain('M 300')
    expect(graph.paths.speed).not.toContain('L 300')
  })

  test('renders a resolved location as a ten-second tolerance band', () => {
    const bands = buildEventBands({
      detection: { stops: [], turns: [], location: { id: 'map', type: 'location', time: 50 } },
      viewStart: 0,
      viewEnd: 100,
      widthPx: 500,
    })

    expect(bands).toEqual([expect.objectContaining({ id: 'map', type: 'location', tone: 'location', startSecond: 40, endSecond: 60 })])
  })

  test('renders a stop using its detected low-speed duration', () => {
    const bands = buildEventBands({
      detection: {
        stops: [{ id: 'stop-0', type: 'stop', time: 20, lowSpeedInterval: { start: 20, end: 37 } }],
        turns: [],
        location: null,
      },
      viewStart: 0,
      viewEnd: 100,
      widthPx: 500,
    })

    expect(bands).toEqual([expect.objectContaining({ id: 'stop-0', startSecond: 20, endSecond: 37 })])
  })

  test('renders a turn using its detected interval', () => {
    const bands = buildEventBands({
      detection: {
        stops: [],
        turns: [{ id: 'turn-0', type: 'rightTurn', start: 20, end: 24 }],
        location: null,
      },
      viewStart: 0,
      viewEnd: 100,
      widthPx: 500,
    })

    expect(bands).toEqual([expect.objectContaining({ id: 'turn-0', startSecond: 20, endSecond: 24 })])
  })
})
