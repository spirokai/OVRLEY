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
    activityTrim: null,
    clips: createClips([createTestVideo({ path: 'C:\\clips\\ride.mp4', durationSeconds: 20 })], { 'video-1': 5 }),
    draggedCut: null,
    filmstrips: {},
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
  test('computes clip visibility and text visibility', () => {
    const { result } = renderClips()
    const clip = clipModel(result, 'video:video-1')

    expect(clip.isVisible).toBe(true)
    expect(clip.showText).toBe(true)
  })

  test('the activity clip takes the video clip colours', () => {
    const { result } = renderClips()
    const activity = clipModel(result, 'activity')

    expect(activity.clipClassName).toBe('bg-accent/70')
    expect(activity.textClassName).toBe('text-accent-foreground')
  })

  test('a video clip with a ready filmstrip shows tiles as tall as the clip, repeating a frame when zoomed in', () => {
    // 5 px per second: the 20 s clip at 5 s is 100 px wide and holds two 85 px tiles (48 px tall at 16:9).
    const { result } = renderClips({
      filmstrips: { 'C:\\clips\\ride.mp4': { status: 'ready', spriteUrl: 'blob:ride', columns: 1, rows: 1, intervalSeconds: 20, sampleCount: 1 } },
    })
    const [segment] = clipModel(result, 'video:video-1').segments

    expect(segment.tiles).toEqual([
      {
        key: 0,
        style: { left: '0px', width: '85px', backgroundImage: 'url("blob:ride")', backgroundSize: '100% 100%', backgroundPosition: '0% 0%' },
      },
      {
        key: 1,
        style: { left: '85px', width: '85px', backgroundImage: 'url("blob:ride")', backgroundSize: '100% 100%', backgroundPosition: '0% 0%' },
      },
    ])
  })

  test('a video clip shows no tiles until its filmstrip is ready', () => {
    const { result } = renderClips({ filmstrips: { 'C:\\clips\\ride.mp4': { status: 'generating', jobId: 1 } } })

    expect(clipModel(result, 'video:video-1').segments[0].tiles).toEqual([])
  })

  test("a video clip's badge sits at the left edge of the clip's visible part", () => {
    // The clip spans 5-25 s; the viewport starts at 10 s, 25 px into the clip, and shows 75 px of it.
    const { result } = renderClips({ viewStart: 10, viewEnd: 110 })

    expect(clipModel(result, 'video:video-1').badgeStyle).toEqual({ left: '25px', maxWidth: '75px' })
  })

  test('shades the trimmed-off activity and puts a trim handle on each edge of its trim', () => {
    const getEdgeDragProps = vi.fn((edge) => ({ 'data-edge': edge }))
    const { result } = renderClips({
      activityTrim: { window: { startSecond: 10, endSecond: 80, isTrimmed: true }, draggedEdge: 'end', getEdgeDragProps },
    })
    const activityLane = result.current.find((lane) => lane.id === 'activity')

    expect(activityLane.clips[0].trimMasks).toEqual([
      { key: 'start', style: { left: '0%', width: '10%' } },
      { key: 'end', style: { left: '80%', width: '20%' } },
    ])
    // The activity's text sits over its untrimmed part.
    expect(activityLane.clips[0].textStyle).toEqual({ left: '10%', width: '70%' })
    expect(
      activityLane.cutMarkers.map(({ key, align, isDragging, dragProps, onRemove }) => ({ key, align, isDragging, dragProps, onRemove })),
    ).toEqual([
      { key: 'activity:start', align: 'center', isDragging: false, dragProps: { 'data-edge': 'start' }, onRemove: null },
      { key: 'activity:end', align: 'center', isDragging: true, dragProps: { 'data-edge': 'end' }, onRemove: null },
    ])
  })

  test('zoomed in, the shade covers only the visible part of the trimmed-off activity', () => {
    const { result } = renderClips({
      activityTrim: { window: { startSecond: 10, endSecond: 80, isTrimmed: true }, draggedEdge: null, getEdgeDragProps: () => ({}) },
      viewStart: 5,
      viewEnd: 15,
    })
    const activityLane = result.current.find((lane) => lane.id === 'activity')

    expect(activityLane.clips[0].trimMasks).toEqual([{ key: 'start', style: { left: '5%', width: '5%' } }])
  })

  test('an untrimmed activity has no shade and its handles sit inside its edges', () => {
    const { result } = renderClips({
      activityTrim: { window: { startSecond: 0, endSecond: 100, isTrimmed: false }, draggedEdge: null, getEdgeDragProps: () => ({}) },
    })
    const activityLane = result.current.find((lane) => lane.id === 'activity')

    expect(activityLane.clips[0].trimMasks).toEqual([])
    expect(activityLane.cutMarkers.map((marker) => marker.align)).toEqual(['start', 'end'])
    expect(activityLane.clips[0].textStyle).toEqual({ left: '0%', width: '100%' })
  })

  test('the trim is not edited without its handle model, as in video sync', () => {
    const { result } = renderClips()
    const activityLane = result.current.find((lane) => lane.id === 'activity')

    expect(activityLane.clips[0].trimMasks).toEqual([])
    expect(activityLane.cutMarkers).toEqual([])
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
