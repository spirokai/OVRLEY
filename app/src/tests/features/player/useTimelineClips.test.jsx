import { act, renderHook } from '@testing-library/react'
import { describe, expect, test, vi } from 'vitest'
import useTimelineClips from '@/features/player/hooks/useTimelineClips'

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
      getLaneDragProps: () => ({}),
      hasActivity: true,
      hasVideo: true,
      importedVideoDuration: 20,
      importedVideoPath: 'C:\\clips\\ride.mp4',
      videoSyncOffsetSeconds: 5,
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

    expect(result.current.map((lane) => lane.id)).toEqual(['video', 'activity'])
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
      availableMetrics: ['Barometric Altitude', 'Location'],
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
      selectedClipId: 'video',
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

    expect(nudgeClip).toHaveBeenCalledWith('video', 1)
    expect(commitClipNudge).toHaveBeenCalledOnce()
  })
})
