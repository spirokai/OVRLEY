import { fireEvent, render, screen } from '@testing-library/react'
import { describe, expect, test, vi } from 'vitest'

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

const videoSummary = {
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
}

const videoSync = {
  activitySummary: { timezone: 'Europe/Zurich', syncTime: '2026-07-18T07:20:03.000Z' },
  canResetCreationTime: false,
  filenameCreationTimeAvailable: true,
  offsetInput: '0',
  timezone: 'Europe/Zurich',
  videoSyncTimezoneMode: 'local',
  videoSyncWarning: null,
  computeVideoSync: vi.fn(),
  incrementOffset: vi.fn(),
  submitOffsetInput: vi.fn(),
  setOffsetInput: vi.fn(),
  resetVideoCreationTime: vi.fn(),
  setVideoCreationTimeFromFilename: vi.fn(),
  setVideoSyncTimezoneMode: vi.fn(),
}

describe('VideoDrawerContent', () => {
  test('offers browse and drop import controls', () => {
    const onBrowseVideo = vi.fn()
    render(
      <VideoDrawerContent
        videoSummary={null}
        onBrowseVideo={onBrowseVideo}
        onDeleteVideo={vi.fn()}
        onDropVideoFiles={vi.fn()}
        videoSync={videoSync}
      />,
    )

    fireEvent.click(screen.getByRole('button', { name: 'Load video' }))

    expect(onBrowseVideo).toHaveBeenCalledOnce()
    expect(screen.getByText('Drop video file')).toBeInTheDocument()
    expect(screen.queryByText('Details')).not.toBeInTheDocument()
  })

  test('renders imported video metadata and sync controls', () => {
    const onDeleteVideo = vi.fn()
    render(
      <VideoDrawerContent
        videoSummary={videoSummary}
        onBrowseVideo={vi.fn()}
        onDeleteVideo={onDeleteVideo}
        onDropVideoFiles={vi.fn()}
        videoSync={videoSync}
      />,
    )

    expect(screen.getByText('ride.mp4')).toBeInTheDocument()
    expect(screen.getByText('H264')).toBeInTheDocument()
    expect(screen.getByText('1:02:03')).toBeInTheDocument()
    expect(screen.getByText('59.94 fps')).toBeInTheDocument()
    expect(screen.getByText('1920×1080')).toBeInTheDocument()
    expect(screen.getByText('GoPro Hero 12')).toBeInTheDocument()
    expect(screen.getByRole('heading', { name: 'Video Sync' })).toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: 'Delete video' }))
    expect(onDeleteVideo).toHaveBeenCalledOnce()
  })
})
