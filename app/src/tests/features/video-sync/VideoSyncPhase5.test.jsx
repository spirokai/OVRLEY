import { fireEvent, render, screen } from '@testing-library/react'
import { describe, expect, test, vi } from 'vitest'
import { VideoSyncLandmarkList } from '@/features/video-sync/components/VideoSyncLandmarkList'
import { VideoSyncMarkControls } from '@/features/video-sync/components/VideoSyncMarkControls'

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

  test('sorts landmark cards and routes scrub and delete actions', () => {
    const onDelete = vi.fn()
    const onScrub = vi.fn()
    render(<VideoSyncLandmarkList landmarks={landmarks} onClear={vi.fn()} onDelete={onDelete} onScrub={onScrub} />)

    const listItems = screen.getAllByRole('listitem')
    expect(listItems[0]).toHaveTextContent('Left Turn')
    expect(listItems[1]).toHaveTextContent('Stop')

    fireEvent.click(screen.getByRole('button', { name: 'Go to Left Turn at 00:04' }))
    fireEvent.click(screen.getByRole('button', { name: 'Delete Stop landmark' }))

    expect(onScrub).toHaveBeenCalledWith(landmarks[1])
    expect(onDelete).toHaveBeenCalledWith('late')
  })
})
