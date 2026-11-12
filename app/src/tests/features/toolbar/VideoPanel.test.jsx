import { act, fireEvent, render, screen, within } from '@testing-library/react'
import { beforeEach, describe, expect, test, vi } from 'vite-plus/test'
import { VideoPanel } from '@/features/toolbar'
import { createMissingVideo, createVideoMetadataSnapshot } from '@/lib/video-program/imported-video'
import useStore from '@/store/useStore'
import { createTestVideo, videoStoreState } from '@/tests/helpers/videoFixtures'

const ride = createTestVideo({ id: 'ride', path: '/videos/ride.mp4', durationSeconds: 90, thumbnailUrl: 'data:image/png;base64,AA==' })
const climb = createTestVideo({ id: 'climb', path: '/videos/climb.mp4', durationSeconds: 65, importOrder: 1 })

function cards() {
  return within(screen.getByRole('list', { name: 'Videos' })).getAllByRole('listitem')
}

describe('VideoPanel', () => {
  beforeEach(() => {
    useStore.setState(useStore.getInitialState(), true)
  })

  test('lists the videos as cards in timeline order; a card selects its timeline clip and the trash button or Delete removes it', () => {
    useStore.setState(videoStoreState([ride, climb], { ride: 30, climb: 0 }))
    const onRemoveVideo = vi.fn()
    render(<VideoPanel onDeleteVideo={vi.fn()} onRemoveVideo={onRemoveVideo} />)

    expect(cards().map((card) => within(card).getByText(/\.mp4$/).textContent)).toEqual(['climb.mp4', 'ride.mp4'])
    expect(cards()[1].querySelector('img')).toHaveAttribute('src', 'data:image/png;base64,AA==')

    const rideCard = within(cards()[1]).getByRole('button', { pressed: false })
    fireEvent.click(rideCard)
    expect(useStore.getState().selectedClip).toEqual({ laneId: 'video:ride', segmentKey: null })
    expect(within(cards()[1]).getByRole('button', { pressed: true })).toBe(rideCard)

    fireEvent.keyDown(rideCard, { key: 'Delete' })
    fireEvent.click(screen.getByRole('button', { name: 'Remove climb.mp4 from project' }))
    expect(onRemoveVideo.mock.calls).toEqual([['ride'], ['climb']])
  })

  test('syncs every video at once: their start, automatic sync and timezone', () => {
    const camera = { creationTime: '2026-07-18T08:00:30Z', timeSource: 'ffprobe', durationSeconds: 20 }
    useStore.setState({
      ...videoStoreState(
        [
          { ...ride, ...camera },
          { ...climb, ...camera, creationTime: '2026-07-18T08:01:00Z' },
        ],
        { ride: 30, climb: 60 },
      ),
      parsedActivity: { metadata: { timezone: 'Europe/Sofia' } },
      activitySummary: { timezone: 'Europe/Sofia', syncTime: '2026-07-18T08:00:00Z', endTime: '2026-07-18T08:02:00Z' },
    })
    render(<VideoPanel onDeleteVideo={vi.fn()} onRemoveVideo={vi.fn()} />)
    expect(screen.getByRole('heading', { name: 'Sync all videos' })).toBeInTheDocument()

    act(() => useStore.getState().setVideoOffset('ride', 500))
    fireEvent.click(screen.getByRole('switch', { name: 'Apply Timezone' }))
    expect(Object.values(useStore.getState().videoEdits).map((edits) => edits.timezoneMode)).toEqual(['utc', 'utc'])
    // The video set by hand keeps its start until every video is synced.
    expect(useStore.getState().videoEdits.ride.offsetSeconds).toBe(500)

    fireEvent.click(screen.getByRole('button', { name: 'Sync all videos' }))
    expect(useStore.getState().videoEdits.ride).toMatchObject({ offsetSeconds: 30, offsetSource: 'auto' })
    expect(useStore.getState().videoEdits.climb.offsetSeconds).toBe(60)
  })

  test('a missing video says that its file was not found instead of showing a thumbnail', () => {
    const missing = createMissingVideo(createVideoMetadataSnapshot(ride), { id: 'ride', path: '/videos/ride.mp4', importOrder: 0 })
    useStore.setState(videoStoreState([missing]))
    render(<VideoPanel onDeleteVideo={vi.fn()} onRemoveVideo={vi.fn()} />)

    expect(within(cards()[0]).getByText('Video not found')).toBeInTheDocument()
    expect(cards()[0].querySelector('img')).toBeNull()
  })

  test('holds the default transitions above the imported videos, which can all be removed at once', () => {
    useStore.setState(videoStoreState([ride]))
    const onDeleteVideo = vi.fn()
    render(<VideoPanel onDeleteVideo={onDeleteVideo} onRemoveVideo={vi.fn()} />)

    expect(screen.getByRole('region', { name: 'Default transitions' })).toBeInTheDocument()
    fireEvent.click(within(screen.getByRole('region', { name: 'Imported videos' })).getByRole('button', { name: 'Remove all' }))

    expect(onDeleteVideo).toHaveBeenCalledOnce()
  })
})
