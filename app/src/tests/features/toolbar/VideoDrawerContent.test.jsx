import { act, fireEvent, render, screen } from '@testing-library/react'
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
import { createMissingVideo, createVideoMetadataSnapshot } from '@/lib/video-program/imported-video'
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

const videoSync = { timezone: 'Europe/Zurich', videoSyncTimezoneMode: 'local' }

function renderDrawer(props = {}) {
  return render(
    <VideoDrawerContent
      hasVideos
      onBrowseVideo={vi.fn()}
      onRemoveVideo={vi.fn()}
      onLocateVideo={vi.fn()}
      onDropVideoFiles={vi.fn()}
      videoSync={videoSync}
      {...props}
    />,
  )
}

describe('VideoDrawerContent', () => {
  beforeEach(() => {
    useStore.setState(useStore.getInitialState(), true)
  })

  test('offers browse and drop import controls', () => {
    const onBrowseVideo = vi.fn()
    renderDrawer({ hasVideos: false, onBrowseVideo })

    fireEvent.click(screen.getByRole('button', { name: 'Load videos' }))

    expect(onBrowseVideo).toHaveBeenCalledOnce()
    expect(screen.getByText('Drop video files')).toBeInTheDocument()
    expect(screen.queryByText('Details')).not.toBeInTheDocument()
  })

  test('shows the details of the selected video clip', () => {
    useStore.setState(videoStoreState([ride, createTestVideo({ id: 'video-2', path: '/videos/climb.mp4', durationSeconds: 65, importOrder: 1 })]))
    renderDrawer()
    expect(screen.getByText('Select a video to see its details.')).toBeInTheDocument()

    // A selected segment selects its video as well.
    act(() => useStore.getState().selectClip({ laneId: 'video:video-1', segmentKey: 'start' }))

    expect(screen.getByText('ride.mp4')).toBeInTheDocument()
    expect(screen.queryByText('climb.mp4')).not.toBeInTheDocument()
    expect(screen.getByText('1:02:03')).toBeInTheDocument()
    expect(screen.getByText('59.94 fps')).toBeInTheDocument()
    expect(screen.getByText('1920×1080')).toBeInTheDocument()
    expect(screen.getByText('GoPro Hero 12')).toBeInTheDocument()
    expect(screen.getByText('Available')).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Locate video…' })).not.toBeInTheDocument()
  })

  test('offers to locate the file of a selected missing video', () => {
    const missing = createMissingVideo(createVideoMetadataSnapshot(ride), { id: 'video-1', path: '/videos/ride.mp4', importOrder: 0 })
    useStore.setState({ ...videoStoreState([missing]), selectedClip: { laneId: 'video:video-1', segmentKey: null } })
    const onLocateVideo = vi.fn()
    renderDrawer({ onLocateVideo })

    expect(screen.getByText('Video file not found. Locate it or remove the video.')).toBeInTheDocument()
    expect(screen.getByText('/videos/ride.mp4')).toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: 'Locate video…' }))

    expect(onLocateVideo).toHaveBeenCalledExactlyOnceWith('video-1')
  })
})
