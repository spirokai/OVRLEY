import { act, renderHook } from '@testing-library/react'
import { describe, expect, test, vi } from 'vitest'
import useTimelineClips from '@/features/player/hooks/useTimelineClips'
import { joinVideoEdits } from '@/lib/video-program/video-collection'
import { createTestVideo, videoStoreState } from '@/tests/helpers/videoFixtures'

function createClips(videos, offsets) {
  const { importedVideos, videoEdits } = videoStoreState(videos, offsets)
  return joinVideoEdits(importedVideos, videoEdits)
}

function renderClipsProps(overrides = {}) {
  return {
    activityFilename: 'ride.fit',
    activitySummary: {
      availableMetrics: [
        { attribute: 'gps_coordinates', source: 'direct' },
        { attribute: 'barometric_altitude', source: 'direct' },
      ],
      durationSeconds: 100,
      fileFormat: 'fit',
      fileName: 'activity.fit',
    },
    deletedPartDrag: {
      preview: null,
      onPointerDown: () => false,
      onPointerMove: () => false,
      onPointerUp: () => false,
      onPointerCancel: () => false,
    },
    deletedPartMark: null,
    exportHighlightRange: { fromSecond: 10, toSecond: 20 },
    clips: createClips([createTestVideo({ path: 'C:\\clips\\ride.mp4', durationSeconds: 20 })], { 'video-1': 5 }),
    getLaneDragProps: () => ({}),
    hasActivity: true,
    viewEnd: 100,
    viewStart: 0,
    widthPx: 500,
    ...overrides,
  }
}

function renderClips(overrides = {}) {
  return renderHook((props) => useTimelineClips(props), { initialProps: renderClipsProps(overrides) })
}

function clipModel(result, clipId) {
  return result.current.flatMap((lane) => lane.clips).find((clip) => clip.id === clipId)
}

describe('useTimelineClips', () => {
  test('builds the video lane and the activity lane from canonical second fields', () => {
    const { result } = renderClips()

    expect(result.current.map((lane) => lane.id)).toEqual(['video', 'activity'])
    expect(result.current.map((lane) => lane.clips.map((clip) => clip.id))).toEqual([['video:video-1'], ['activity']])
    expect(clipModel(result, 'video:video-1')).toMatchObject({
      durationLabel: '00:20',
      durationSeconds: 20,
      label: 'ride.mp4',
      startSecond: 5,
      warnings: [],
    })
    expect(clipModel(result, 'activity')).toMatchObject({
      durationLabel: '01:40',
      durationSeconds: 100,
      formatLabel: 'FIT',
      label: 'ride.fit',
      availableMetrics: ['Barometric Altitude', 'GPS Coordinates'],
      startSecond: 0,
    })
  })

  test('computes clip visibility, text visibility, and export highlight styles', () => {
    const { result } = renderClips()
    const clip = clipModel(result, 'video:video-1')

    expect(clip.isVisible).toBe(true)
    expect(clip.showText).toBe(true)
    expect(clip.highlightStyle).toEqual({
      left: '25%',
      width: '50%',
    })
  })

  test('owns tooltip state and exposes hover handlers in the clip model', () => {
    const { result } = renderClips()

    expect(clipModel(result, 'video:video-1').tooltip.isVisible).toBe(false)

    act(() => {
      clipModel(result, 'video:video-1').clipProps.onMouseEnter()
    })

    expect(clipModel(result, 'video:video-1').tooltip.isVisible).toBe(true)

    act(() => {
      clipModel(result, 'video:video-1').clipProps.onMouseLeave()
    })

    expect(clipModel(result, 'video:video-1').tooltip.isVisible).toBe(false)
  })

  test('nudges a selected clip by one second with Shift+Arrow', () => {
    const nudgeClip = vi.fn()
    const commitClipNudge = vi.fn()
    const { result } = renderClips({
      commitClipNudge,
      nudgeClip,
      selectedClipId: 'video:video-1',
      setSelectedClipId: vi.fn(),
    })
    const event = {
      defaultPrevented: false,
      key: 'ArrowRight',
      preventDefault: vi.fn(),
      shiftKey: true,
      stopPropagation: vi.fn(),
    }

    act(() => {
      clipModel(result, 'video:video-1').clipProps.onKeyDown(event)
      clipModel(result, 'video:video-1').clipProps.onKeyUp(event)
    })

    expect(nudgeClip).toHaveBeenCalledWith('video:video-1', 1)
    expect(commitClipNudge).toHaveBeenCalledOnce()
  })

  test('removes the selected video clip with Delete or Backspace but never the activity', () => {
    const onRemoveVideo = vi.fn()
    const keyEvent = (key) => ({ defaultPrevented: false, key, preventDefault: vi.fn(), stopPropagation: vi.fn() })
    const { result, rerender } = renderClips({ onRemoveVideo, selectedClipId: 'video:video-1', setSelectedClipId: vi.fn() })

    act(() => clipModel(result, 'video:video-1').clipProps.onKeyDown(keyEvent('Delete')))
    act(() => clipModel(result, 'video:video-1').clipProps.onKeyDown(keyEvent('Backspace')))
    expect(onRemoveVideo.mock.calls).toEqual([['video-1'], ['video-1']])
    expect(clipModel(result, 'video:video-1').clipProps['aria-keyshortcuts']).toContain('Delete Backspace')

    rerender(renderClipsProps({ onRemoveVideo, selectedClipId: 'activity', setSelectedClipId: vi.fn() }))
    act(() => clipModel(result, 'activity').clipProps.onKeyDown(keyEvent('Delete')))
    expect(onRemoveVideo).toHaveBeenCalledTimes(2)
  })

  test('puts every clip on the video lane in timeline order before the activity lane', () => {
    const getLaneDragProps = vi.fn(() => ({}))
    const clips = createClips(
      [
        createTestVideo({ id: 'late', path: '/videos/late.mp4', durationSeconds: 10 }),
        createTestVideo({ id: 'early', path: '/videos/early.mp4', durationSeconds: 30, importOrder: 1 }),
      ],
      { late: 40, early: -5 },
    )
    const { result } = renderClips({ clips, getLaneDragProps })

    expect(result.current.map((lane) => lane.clips.map((clip) => clip.id))).toEqual([['video:early', 'video:late'], ['activity']])
    expect(clipModel(result, 'video:early')).toMatchObject({ isVideo: true, label: 'early.mp4', startSecond: -5, durationSeconds: 30 })
    expect(clipModel(result, 'video:late')).toMatchObject({ isVideo: true, label: 'late.mp4', startSecond: 40, durationSeconds: 10 })
    expect(getLaneDragProps.mock.calls.map(([clipId]) => clipId)).toEqual(['video:early', 'video:late', 'activity'])
  })

  test('marks overlaps with bands on the video lane and lists clip warnings', () => {
    const clips = createClips(
      [createTestVideo({ id: 'a', path: '/videos/a.mp4', durationSeconds: 30 }), createTestVideo({ id: 'b', path: '/videos/b.mp4', importOrder: 1 })],
      { a: 0, b: 20 },
    )
    const warnings = new Map([
      ['a', [{ kind: 'overlap', message: 'Overlaps b.mp4' }]],
      ['b', []],
    ])
    const { result } = renderClips({ clips, overlaps: [{ startSecond: 20, endSecond: 30 }], warnings })

    expect(result.current[0].conflictBands).toEqual([{ key: '20:30', style: { left: '20%', width: '10%' } }])
    expect(result.current[1].conflictBands).toEqual([])
    expect(clipModel(result, 'video:a')).toMatchObject({ warnings: ['Overlaps b.mp4'] })
    expect(clipModel(result, 'video:a').clipClassName).toContain('outline-destructive')
    expect(clipModel(result, 'video:b').clipClassName).not.toContain('outline-destructive')
  })

  test('selects the pressed clip by its clip id', () => {
    const setSelectedClipId = vi.fn()
    const onPointerDown = vi.fn()
    const clips = createClips(
      [createTestVideo({ id: 'first', durationSeconds: 10 }), createTestVideo({ id: 'second', path: '/videos/second.mp4', importOrder: 1 })],
      { first: 0, second: 20 },
    )
    const { result } = renderClips({ canSelect: true, clips, getLaneDragProps: () => ({ onPointerDown }), setSelectedClipId })
    const event = { button: 0, currentTarget: { focus: vi.fn() } }

    act(() => {
      clipModel(result, 'video:second').clipProps.onPointerDown(event)
    })

    expect(setSelectedClipId).toHaveBeenCalledWith('video:second')
    expect(onPointerDown).toHaveBeenCalledWith(event)
  })

  test('omits the activity lane without external activity', () => {
    const { result } = renderClips({ activitySummary: null, hasActivity: false })

    expect(result.current.map((lane) => lane.id)).toEqual(['video'])
  })
})
