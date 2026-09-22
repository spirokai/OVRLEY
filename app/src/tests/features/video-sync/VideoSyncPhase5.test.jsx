import { fireEvent, render, screen } from '@testing-library/react'
import { describe, expect, test, vi } from 'vitest'
import { VideoSyncLandmarkList } from '@/features/video-sync/components/VideoSyncLandmarkList'
import { VideoSyncMarkControls } from '@/features/video-sync/components/VideoSyncMarkControls'
import { buildVideoSyncDrawerPresentation } from '@/features/video-sync/utils/videoSyncPresentation'

const landmarks = [
  { id: 'late', type: 'stop', videoSecond: 12 },
  { id: 'early', type: 'leftTurn', videoSecond: 4 },
]

describe('manual video-sync Phase 5 controls', () => {
  test('keeps all typed mark actions visible and disables them outside the video', () => {
    render(
      <VideoSyncMarkControls
        canMark={false}
        canMarkLocation={false}
        locationDisabledReason="Move the playhead inside the video to mark a landmark"
        markDisabledReason="Move the playhead inside the video to mark a landmark"
        onMarkLeftTurn={vi.fn()}
        onMarkLocation={vi.fn()}
        onMarkRightTurn={vi.fn()}
        onMarkStop={vi.fn()}
      />,
    )

    const buttons = screen.getAllByRole('button')
    expect(buttons).toHaveLength(4)
    expect(buttons.every((button) => button.disabled)).toBe(true)
  })

  test('sorts landmark cards and routes delete actions', () => {
    const onDelete = vi.fn()
    const { landmarkCards } = buildVideoSyncDrawerPresentation(landmarks, null, { speedThresholdDraftKmh: 5, turnThresholdDraftDegrees: 90 })
    render(
      <VideoSyncLandmarkList
        hasLocationLandmark={false}
        landmarkCards={landmarkCards}
        onChangeType={vi.fn()}
        onClear={vi.fn()}
        onDelete={onDelete}
      />,
    )

    const listItems = screen.getAllByRole('listitem')
    expect(listItems[0]).toHaveTextContent('Left Turn')
    expect(listItems[1]).toHaveTextContent('Stop')
    expect(screen.getAllByRole('combobox')).toHaveLength(2)

    fireEvent.click(screen.getByRole('button', { name: 'Delete Stop landmark' }))

    expect(onDelete).toHaveBeenCalledWith('late')
  })

  test('orders mark controls left, stop, location, and right in one row', () => {
    render(
      <VideoSyncMarkControls
        canMark
        canMarkLocation
        locationDisabledReason={null}
        markDisabledReason={null}
        onMarkLeftTurn={vi.fn()}
        onMarkLocation={vi.fn()}
        onMarkRightTurn={vi.fn()}
        onMarkStop={vi.fn()}
      />,
    )

    const controls = screen.getByTestId('video-sync-mark-controls')
    expect(controls.className).toContain('flex-row')
    expect([...controls.querySelectorAll('button')].map((button) => button.getAttribute('aria-label'))).toEqual([
      'Mark Left Turn',
      'Mark Stop',
      'Mark Location',
      'Mark Right Turn',
    ])
  })
})
