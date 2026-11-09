import { fireEvent, render, screen } from '@testing-library/react'
import { beforeEach, describe, expect, test, vi } from 'vite-plus/test'

vi.mock('@/features/toolbar/hooks/useFileDropZone', () => ({
  useFileDropZone: () => ({
    dragPosition: null,
    dropZoneRef: { current: null },
    isDraggingFile: false,
    isOverDropZone: false,
    dropZoneProps: {},
  }),
}))

import { VideoDrawerContent } from '@/features/toolbar'
import useStore from '@/store/useStore'
import { createTestVideo, videoStoreState } from '@/tests/helpers/videoFixtures'

const ride = {
  ...createTestVideo({
    id: 'video-1',
    path: '/videos/ride.mp4',
    durationSeconds: 3723,
    fps: { num: 60000, den: 1001 },
    creationTime: '2026-07-18T07:20:03.000Z',
    timeSource: 'ffprobe',
    telemetry: { status: 'ready', activity: {} },
  }),
  cameraModel: 'GoPro Hero 12',
}

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

describe('VideoDrawerContent', () => {
  beforeEach(() => {
    useStore.setState(useStore.getInitialState(), true)
    useStore.setState(videoStoreState([ride, createTestVideo({ id: 'video-2', path: '/videos/climb.mp4', durationSeconds: 65, importOrder: 1 })]))
  })

  test('offers browse and drop import controls', () => {
    useStore.setState(videoStoreState([]))
    const onBrowseVideo = vi.fn()
    render(<VideoDrawerContent onBrowseVideo={onBrowseVideo} onDeleteVideo={vi.fn()} onDropVideoFiles={vi.fn()} videoSync={videoSync} />)

    fireEvent.click(screen.getByRole('button', { name: 'Load videos' }))

    expect(onBrowseVideo).toHaveBeenCalledOnce()
    expect(screen.getByText('Drop video files')).toBeInTheDocument()
    expect(screen.queryByText('Details')).not.toBeInTheDocument()
  })

  test('renders imported video metadata and sync controls', () => {
    useStore.setState(videoStoreState([ride]))
    const onDeleteVideo = vi.fn()
    render(<VideoDrawerContent onBrowseVideo={vi.fn()} onDeleteVideo={onDeleteVideo} onDropVideoFiles={vi.fn()} videoSync={videoSync} />)

    expect(screen.getByText('ride.mp4')).toBeInTheDocument()
    expect(screen.getByText('1:02:03')).toBeInTheDocument()
    expect(screen.getByText('59.94 fps')).toBeInTheDocument()
    expect(screen.getByText('1920×1080')).toBeInTheDocument()
    expect(screen.getByText('GoPro Hero 12')).toBeInTheDocument()
    expect(screen.getByText('Available')).toBeInTheDocument()
    expect(screen.getByRole('heading', { name: 'Sync Doctor' })).toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: 'Remove all videos' }))
    expect(onDeleteVideo).toHaveBeenCalledOnce()
  })

  test('offers manual sync when automatic sync cannot find an offset', () => {
    const openManualVideoSync = vi.fn()
    render(
      <VideoDrawerContent
        onBrowseVideo={vi.fn()}
        onDeleteVideo={vi.fn()}
        onDropVideoFiles={vi.fn()}
        videoSync={{ ...videoSync, openManualVideoSync, videoSyncWarning: 'Video could not be synced with activity' }}
      />,
    )

    fireEvent.click(screen.getByRole('button', { name: 'Open Manual Sync' }))

    expect(openManualVideoSync).toHaveBeenCalledOnce()
  })
})
