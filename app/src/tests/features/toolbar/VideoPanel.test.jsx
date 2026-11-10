import { fireEvent, render, screen, within } from '@testing-library/react'
import { beforeEach, describe, expect, test, vi } from 'vite-plus/test'
import { VideoPanel } from '@/features/toolbar'
import { createMissingVideo, createVideoMetadataSnapshot } from '@/lib/video-program/imported-video'
import useStore from '@/store/useStore'
import { createTestVideo, videoStoreState } from '@/tests/helpers/videoFixtures'

const ride = createTestVideo({ id: 'ride', path: '/videos/ride.mp4', durationSeconds: 90, thumbnailUrl: 'data:image/png;base64,AA==' })
const climb = createTestVideo({ id: 'climb', path: '/videos/climb.mp4', durationSeconds: 65, importOrder: 1 })

const videoSync = {
  activitySummary: { timezone: 'Europe/Zurich', syncTime: '2026-07-18T07:20:03.000Z' },
  canResetCreationTime: false,
  filenameCreationTimeAvailable: true,
  sharedOffsetSeconds: 0,
  sharedOffsetFps: { num: 30, den: 1 },
  timezone: 'Europe/Zurich',
  timezoneDisabled: false,
  videoSyncTimezoneMode: 'local',
  videoSyncWarning: null,
  computeVideoSync: vi.fn(),
  commitSharedOffset: vi.fn(() => null),
  resetVideoCreationTimes: vi.fn(),
  setVideoCreationTimesFromFilenames: vi.fn(),
  setVideoSyncTimezoneMode: vi.fn(),
}

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
    render(<VideoPanel onDeleteVideo={vi.fn()} onRemoveVideo={onRemoveVideo} videoSync={videoSync} />)

    expect(cards().map((card) => within(card).getByTitle(/\.mp4$/).textContent)).toEqual(['climb.mp4', 'ride.mp4'])
    expect(cards()[1].querySelector('img')).toHaveAttribute('src', 'data:image/png;base64,AA==')

    const rideCard = within(cards()[1]).getByRole('button', { pressed: false })
    fireEvent.click(rideCard)
    expect(useStore.getState().selectedClip).toEqual({ laneId: 'video:ride', segmentKey: null })
    expect(within(cards()[1]).getByRole('button', { pressed: true })).toBe(rideCard)

    fireEvent.keyDown(rideCard, { key: 'Delete' })
    fireEvent.click(screen.getByRole('button', { name: 'Remove climb.mp4 from project' }))
    expect(onRemoveVideo.mock.calls).toEqual([['ride'], ['climb']])
  })

  test('a missing video says that its file was not found instead of showing a thumbnail', () => {
    const missing = createMissingVideo(createVideoMetadataSnapshot(ride), { id: 'ride', path: '/videos/ride.mp4', importOrder: 0 })
    useStore.setState(videoStoreState([missing]))
    render(<VideoPanel onDeleteVideo={vi.fn()} onRemoveVideo={vi.fn()} videoSync={videoSync} />)

    expect(within(cards()[0]).getByText('Video not found')).toBeInTheDocument()
    expect(cards()[0].querySelector('img')).toBeNull()
  })

  test('holds the project video settings above the cards', () => {
    useStore.setState(videoStoreState([ride]))
    const onDeleteVideo = vi.fn()
    const openManualVideoSync = vi.fn()
    render(
      <VideoPanel
        onDeleteVideo={onDeleteVideo}
        onRemoveVideo={vi.fn()}
        videoSync={{ ...videoSync, openManualVideoSync, videoSyncWarning: 'Video could not be synced with activity' }}
      />,
    )

    expect(screen.getByRole('heading', { name: 'Sync Doctor' })).toBeInTheDocument()
    expect(screen.getByRole('region', { name: 'Default transitions' })).toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: 'Open Manual Sync' }))
    fireEvent.click(screen.getByRole('button', { name: 'Remove all' }))

    expect(openManualVideoSync).toHaveBeenCalledOnce()
    expect(onDeleteVideo).toHaveBeenCalledOnce()
  })
})
