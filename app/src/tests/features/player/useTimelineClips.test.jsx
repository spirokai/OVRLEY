import { act, renderHook } from '@testing-library/react'
import { describe, expect, test, vi } from 'vite-plus/test'
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
    exportHighlightRange: { fromSecond: 10, toSecond: 20 },
    clips: createClips([createTestVideo({ path: 'C:\\clips\\ride.mp4', durationSeconds: 20 })], { 'video-1': 5 }),
    draggedCut: null,
    getCutDragProps: () => ({}),
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

  test('nudges a selected clip by one second with Shift+Arrow', () => {
    const nudgeClip = vi.fn()
    const commitClipNudge = vi.fn()
    const { result } = renderClips({
      commitClipNudge,
      nudgeClip,
      selectedClip: { laneId: 'video:video-1', segmentKey: null },
      setSelectedClip: vi.fn(),
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
    const { result, rerender } = renderClips({ onRemoveVideo, selectedClip: { laneId: 'video:video-1', segmentKey: null }, setSelectedClip: vi.fn() })

    act(() => clipModel(result, 'video:video-1').clipProps.onKeyDown(keyEvent('Delete')))
    act(() => clipModel(result, 'video:video-1').clipProps.onKeyDown(keyEvent('Backspace')))
    expect(onRemoveVideo.mock.calls).toEqual([['video-1'], ['video-1']])
    expect(clipModel(result, 'video:video-1').clipProps['aria-keyshortcuts']).toContain('Delete Backspace')

    rerender(renderClipsProps({ onRemoveVideo, selectedClip: { laneId: 'activity', segmentKey: null }, setSelectedClip: vi.fn() }))
    act(() => clipModel(result, 'activity').clipProps.onKeyDown(keyEvent('Delete')))
    expect(onRemoveVideo).toHaveBeenCalledTimes(2)
  })
})
