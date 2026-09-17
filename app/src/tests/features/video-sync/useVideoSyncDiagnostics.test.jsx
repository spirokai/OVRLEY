import { renderHook } from '@testing-library/react'
import { describe, expect, test } from 'vitest'
import useVideoSyncDiagnostics from '@/features/video-sync/hooks/useVideoSyncDiagnostics'

function makeActivity() {
  return {
    trim_end_seconds: 11,
    sample_elapsed_seconds: [0, 1, 10, 11],
    speed: [4, 6, 8, 10],
    heading: [0, 0, 0, 0],
    sample_course_points: [
      [50, 14],
      [50.001, 14.001],
      [50.01, 14.01],
      [50.011, 14.011],
    ],
  }
}

function renderDiagnostics(activity, timelineSecond = 0.5, markControls = null) {
  return renderHook(() =>
    useVideoSyncDiagnostics({
      activity,
      timelineSecond,
      markControls,
      globalScale: 1,
      exportStartSecond: 0,
      sceneSize: { width: 1920, height: 1080 },
      enabled: true,
    }),
  )
}

describe('useVideoSyncDiagnostics', () => {
  test('uses fixed ephemeral widgets and the existing speed preview model', () => {
    const markControls = { canMark: true }
    const { result } = renderDiagnostics(makeActivity(), 0.5, markControls)

    expect(result.current.speed.available).toBe(true)
    expect(result.current.speed.previewModel.content.valueText).toBe('18.0')
    expect(result.current.speed.widget).toMatchObject({ type: 'speed', category: 'values' })
    expect(result.current.route).toMatchObject({ available: true, widget: { type: 'course', category: 'plots' } })
    expect(result.current.route.widget.data).toMatchObject({
      completed_line_color: '#ffffff',
      completed_line_opacity: 100,
      marker_color: '#ff1f1f',
      remaining_line_color: '#ffffff',
      remaining_line_opacity: 100,
      show_full_activity: true,
      width: 806,
      height: 594,
    })
    expect(result.current.markControls).toBe(markControls)
  })

  test('reports unavailable diagnostics when the source channels are absent', () => {
    const activity = {
      trim_end_seconds: 1,
      sample_elapsed_seconds: [0, 1],
      speed: [null, null],
      heading: [0, 0],
      sample_course_points: [null, null],
    }
    const { result } = renderDiagnostics(activity)

    expect(result.current.speed.available).toBe(false)
    expect(result.current.route.available).toBe(false)
  })
})
