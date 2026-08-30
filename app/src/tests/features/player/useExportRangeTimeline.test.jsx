import { act, renderHook } from '@testing-library/react'
import { beforeEach, describe, expect, test } from 'vitest'
import useExportRangeTimeline from '@/features/player/hooks/useExportRangeTimeline'
import useStore from '@/store/useStore'

function resetStore(exportRange = { from: 10, to: 20, type: 'custom' }) {
  useStore.setState(useStore.getInitialState(), true)
  useStore.setState((state) => ({ renderSettings: { ...state.renderSettings, range: exportRange } }))
}

describe('useExportRangeTimeline', () => {
  beforeEach(() => {
    resetStore()
  })

  test('returns custom export markers and highlight range in seconds', () => {
    const { result } = renderHook(() => useExportRangeTimeline({ totalDuration: 60 }))

    expect(result.current.markers).toEqual([
      { label: 'Export from', marker: 'from', second: 10 },
      { label: 'Export to', marker: 'to', second: 20 },
    ])
    expect(result.current.highlightRange).toEqual({ fromSecond: 10, toSecond: 20 })
  })

  test('previews marker movement without writing to the store', () => {
    const { result } = renderHook(() => useExportRangeTimeline({ totalDuration: 60 }))

    act(() => {
      result.current.previewMarker('from', 18.5)
    })

    expect(result.current.highlightRange).toEqual({ fromSecond: 18.5, toSecond: 20 })
    expect(useStore.getState().renderSettings.range.from).toBe(10)
  })

  test('commits snapped marker movement and avoids redundant writes', () => {
    const { result } = renderHook(() => useExportRangeTimeline({ totalDuration: 60 }))

    act(() => {
      result.current.commitMarker('from', 18.5)
    })

    expect(useStore.getState().renderSettings.range.from).toBe(18.5)

    const sameRange = useStore.getState().renderSettings.range
    act(() => {
      result.current.commitMarker('from', 18.5)
    })

    expect(useStore.getState().renderSettings.range).toBe(sameRange)
  })

  test('returns no markers when the export range is not custom', () => {
    resetStore({ from: 0, to: 60, type: 'full' })

    const { result } = renderHook(() => useExportRangeTimeline({ totalDuration: 60 }))

    expect(result.current.markers).toEqual([])
    expect(result.current.highlightRange).toBeNull()
  })

  test('enables a custom range and sets either boundary from the playhead', () => {
    resetStore({ from: 0, to: 0, type: 'all' })
    const { result } = renderHook(() => useExportRangeTimeline({ defaultEndSecond: 45, totalDuration: 60 }))

    act(() => {
      result.current.setBoundary('from', 12.9)
    })

    expect(useStore.getState().renderSettings.range).toEqual(expect.objectContaining({ type: 'custom', from: 12.9, to: 45 }))

    act(() => {
      result.current.setBoundary('to', 42.9)
    })

    expect(useStore.getState().renderSettings.range).toEqual(expect.objectContaining({ type: 'custom', from: 12.9, to: 42.9 }))

    act(() => {
      result.current.setBoundary('to', 5)
    })

    expect(result.current.rangeLabel).toBe('[00:00:12-00:00:13]')
  })
})
