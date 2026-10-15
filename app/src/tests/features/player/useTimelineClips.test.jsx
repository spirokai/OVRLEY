import { act, renderHook } from '@testing-library/react'
import { describe, expect, test, vi } from 'vitest'
import useTimelineClips from '@/features/player/hooks/useTimelineClips'
import { joinVideoEdits } from '@/lib/video-program/video-collection'
import { createTestVideo, videoStoreState } from '@/tests/helpers/videoFixtures'

function createClips(videos, offsets) {
  const { importedVideos, videoEdits } = videoStoreState(videos, offsets)
  return joinVideoEdits(importedVideos, videoEdits)
}

function renderClips(overrides = {}) {
  return renderHook((props) => useTimelineClips(props), {
    initialProps: {
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
      exportHighlightRange: { fromSecond: 10, toSecond: 20 },
      clips: createClips([createTestVideo({ path: 'C:\\clips\\ride.mp4', durationSeconds: 20 })], { 'video-1': 5 }),
      getLaneDragProps: () => ({}),
      hasActivity: true,
      viewEnd: 100,
      viewStart: 0,
      widthPx: 500,
      ...overrides,
    },
  })
}

describe('useTimelineClips', () => {
  test('builds video and activity lane models from canonical second fields', () => {
    const { result } = renderClips()

    expect(result.current.map((lane) => lane.id)).toEqual(['video:video-1', 'activity'])
    expect(result.current[0]).toMatchObject({
      durationLabel: '00:20',
      durationSeconds: 20,
      label: 'ride.mp4',
      startSecond: 5,
    })
    expect(result.current[1]).toMatchObject({
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

    expect(result.current[0].isVisible).toBe(true)
    expect(result.current[0].showText).toBe(true)
    expect(result.current[0].highlightStyle).toEqual({
      left: '25%',
      width: '50%',
    })
  })

  test('owns tooltip state and exposes hover handlers in the lane model', () => {
    const { result } = renderClips()

    expect(result.current[0].tooltip.isVisible).toBe(false)

    act(() => {
      result.current[0].clipProps.onMouseEnter()
    })

    expect(result.current[0].tooltip.isVisible).toBe(true)

    act(() => {
      result.current[0].clipProps.onMouseLeave()
    })

    expect(result.current[0].tooltip.isVisible).toBe(false)
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
      result.current[0].clipProps.onKeyDown(event)
      result.current[0].clipProps.onKeyUp(event)
    })

    expect(nudgeClip).toHaveBeenCalledWith('video:video-1', 1)
    expect(commitClipNudge).toHaveBeenCalledOnce()
  })

  test('builds one lane per clip in timeline order before the activity lane', () => {
    const getLaneDragProps = vi.fn(() => ({}))
    const clips = createClips(
      [
        createTestVideo({ id: 'late', path: '/videos/late.mp4', durationSeconds: 10 }),
        createTestVideo({ id: 'early', path: '/videos/early.mp4', durationSeconds: 30, importOrder: 1 }),
      ],
      { late: 40, early: -5 },
    )
    const { result } = renderClips({ clips, getLaneDragProps })

    expect(result.current.map((lane) => lane.id)).toEqual(['video:early', 'video:late', 'activity'])
    expect(result.current[0]).toMatchObject({ isVideo: true, label: 'early.mp4', startSecond: -5, durationSeconds: 30 })
    expect(result.current[1]).toMatchObject({ isVideo: true, label: 'late.mp4', startSecond: 40, durationSeconds: 10 })
    expect(getLaneDragProps.mock.calls.map(([laneId]) => laneId)).toEqual(['video:early', 'video:late', 'activity'])
  })

  test('selects the pressed clip lane by its lane id', () => {
    const setSelectedClipId = vi.fn()
    const onPointerDown = vi.fn()
    const clips = createClips(
      [createTestVideo({ id: 'first', durationSeconds: 10 }), createTestVideo({ id: 'second', path: '/videos/second.mp4', importOrder: 1 })],
      { first: 0, second: 20 },
    )
    const { result } = renderClips({ canSelect: true, clips, getLaneDragProps: () => ({ onPointerDown }), setSelectedClipId })
    const event = { button: 0, currentTarget: { focus: vi.fn() } }

    act(() => {
      result.current[1].clipProps.onPointerDown(event)
    })

    expect(setSelectedClipId).toHaveBeenCalledWith('video:second')
    expect(onPointerDown).toHaveBeenCalledWith(event)
  })

  test('omits the activity lane without external activity', () => {
    const { result } = renderClips({ activitySummary: null, hasActivity: false })

    expect(result.current.map((lane) => lane.id)).toEqual(['video:video-1'])
  })
})
