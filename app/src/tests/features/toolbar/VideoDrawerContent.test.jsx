import { fireEvent, render, screen } from '@testing-library/react'
import { beforeEach, describe, expect, test, vi } from 'vitest'

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

const videoSummary = {
  id: 'video-1',
  path: '/videos/ride.mp4',
  filename: 'ride.mp4',
  duration: 3723,
  fps: 59.94,
  resolution: { width: 1920, height: 1080 },
  creationTime: '2026-07-18T07:20:03.000Z',
  timeSource: 'ffprobe',
  codecName: 'h264',
  codecLongName: 'H.264 / AVC / MPEG-4 AVC / MPEG-4 part 10',
  bitRate: 50_000_000,
  cameraType: null,
  cameraModel: 'GoPro Hero 12',
  offsetSeconds: 0,
  telemetryStatus: 'ready',
}

const videoSync = {
  activitySummary: { timezone: 'Europe/Zurich', syncTime: '2026-07-18T07:20:03.000Z' },
  canResetCreationTime: false,
  filenameCreationTimeAvailable: true,
  offsetInput: '0',
  timezone: 'Europe/Zurich',
  timezoneDisabled: false,
  videoSyncTimezoneMode: 'local',
  videoSyncWarning: null,
  computeVideoSync: vi.fn(),
  incrementOffset: vi.fn(),
  submitOffsetInput: vi.fn(),
  setOffsetInput: vi.fn(),
  resetVideoCreationTimes: vi.fn(),
  setVideoCreationTimesFromFilenames: vi.fn(),
  setVideoSyncTimezoneMode: vi.fn(),
}

describe('VideoDrawerContent', () => {
  beforeEach(() => {
    useStore.setState(useStore.getInitialState(), true)
  })

  test('offers browse and drop import controls', () => {
    const onBrowseVideo = vi.fn()
    render(
      <VideoDrawerContent
        videoSummaries={[]}
        onBrowseVideo={onBrowseVideo}
        onDeleteVideo={vi.fn()}
        onDropVideoFiles={vi.fn()}
        videoSync={videoSync}
      />,
    )

    fireEvent.click(screen.getByRole('button', { name: 'Load videos' }))

    expect(onBrowseVideo).toHaveBeenCalledOnce()
    expect(screen.getByText('Drop video files')).toBeInTheDocument()
    expect(screen.queryByText('Details')).not.toBeInTheDocument()
  })

  test('renders imported video metadata and sync controls', () => {
    const onDeleteVideo = vi.fn()
    render(
      <VideoDrawerContent
        videoSummaries={[videoSummary]}
        onBrowseVideo={vi.fn()}
        onDeleteVideo={onDeleteVideo}
        onDropVideoFiles={vi.fn()}
        videoSync={videoSync}
      />,
    )

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
        videoSummaries={[videoSummary]}
        onBrowseVideo={vi.fn()}
        onDeleteVideo={vi.fn()}
        onDropVideoFiles={vi.fn()}
        videoSync={{ ...videoSync, openManualVideoSync, videoSyncWarning: 'Video could not be synced with activity' }}
      />,
    )

    fireEvent.click(screen.getByRole('button', { name: 'Open Manual Sync' }))

    expect(openManualVideoSync).toHaveBeenCalledOnce()
  })

  test('renders details for every imported video with one shared sync section', () => {
    const secondSummary = { ...videoSummary, id: 'video-2', path: '/videos/climb.mp4', filename: 'climb.mp4', duration: 65, fps: 30 }
    render(
      <VideoDrawerContent
        videoSummaries={[videoSummary, secondSummary]}
        onBrowseVideo={vi.fn()}
        onDeleteVideo={vi.fn()}
        onDropVideoFiles={vi.fn()}
        videoSync={videoSync}
      />,
    )

    expect(screen.getByText('ride.mp4')).toBeInTheDocument()
    expect(screen.getByText('climb.mp4')).toBeInTheDocument()
    expect(screen.getByText('59.94 fps')).toBeInTheDocument()
    expect(screen.getByText('30 fps')).toBeInTheDocument()
    expect(screen.getAllByText('Details')).toHaveLength(2)
    expect(screen.getAllByRole('heading', { name: 'Sync Doctor' })).toHaveLength(1)
  })

  test('each row removes its own video, edits its own offset, and lists its warnings', () => {
    const ride = createTestVideo({ id: 'video-1', path: '/videos/ride.mp4', durationSeconds: 60 })
    const climb = createTestVideo({ id: 'video-2', path: '/videos/climb.mp4', durationSeconds: 60, width: 3840, height: 2160, importOrder: 1 })
    const third = createTestVideo({ id: 'video-3', path: '/videos/third.mp4', durationSeconds: 60, importOrder: 2 })
    useStore.setState(videoStoreState([ride, climb, third], { 'video-1': 0, 'video-2': 100, 'video-3': 50 }))
    const onRemoveVideo = vi.fn()
    const summaries = [
      videoSummary,
      { ...videoSummary, id: 'video-3', path: '/videos/third.mp4', filename: 'third.mp4', offsetSeconds: 50 },
      { ...videoSummary, id: 'video-2', path: '/videos/climb.mp4', filename: 'climb.mp4', offsetSeconds: 100, telemetryStatus: 'pending' },
    ]
    render(
      <VideoDrawerContent
        videoSummaries={summaries}
        onBrowseVideo={vi.fn()}
        onDeleteVideo={vi.fn()}
        onRemoveVideo={onRemoveVideo}
        onDropVideoFiles={vi.fn()}
        videoSync={videoSync}
      />,
    )

    const climbRow = screen.getByRole('region', { name: 'climb.mp4' })
    expect(climbRow).toHaveTextContent('Different resolution or frame rate than the other videos')
    expect(climbRow).toHaveTextContent('Reading…')
    expect(screen.getByRole('region', { name: 'ride.mp4' })).toHaveTextContent('Overlaps third.mp4')

    fireEvent.click(screen.getByRole('button', { name: 'Remove climb.mp4' }))
    expect(onRemoveVideo).toHaveBeenCalledExactlyOnceWith('video-2')

    const [, thirdOffset] = screen.getAllByLabelText('Start on timeline')
    expect(thirdOffset).toHaveValue('50')
    fireEvent.change(thirdOffset, { target: { value: '1:10.5' } })
    fireEvent.blur(thirdOffset)
    expect(useStore.getState().videoEdits).toMatchObject({
      'video-1': { offsetSeconds: 0 },
      'video-2': { offsetSeconds: 100 },
      'video-3': { offsetSeconds: 70.5 },
    })
  })
})
