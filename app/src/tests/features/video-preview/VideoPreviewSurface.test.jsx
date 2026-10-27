import { render, screen } from '@testing-library/react'
import { beforeEach, describe, expect, test, vi } from 'vite-plus/test'
import VideoPreviewSurface from '@/features/video-preview/components/VideoPreviewSurface'
import useStore from '@/store/useStore'
import { createTestVideo, videoStoreState } from '@/tests/helpers/videoFixtures'

vi.mock('@/features/video-preview/hooks/useVideoPlaybackClock', () => ({ useVideoPlaybackClock: vi.fn() }))
vi.mock('@/features/video-preview/hooks/useVideoPreviewWarnings', () => ({
  useVideoPreviewWarnings: vi.fn(() => ({ hevcPlaybackWarning: '', metadataStatusMessage: '', nativeVideoError: '', seekWarning: '' })),
}))

describe('VideoPreviewSurface', () => {
  beforeEach(() => {
    useStore.setState(useStore.getInitialState(), true)
    useStore.setState({
      ...videoStoreState(
        [
          createTestVideo({ id: 'video-1', path: 'C:\\clips\\first.mp4', durationSeconds: 10 }),
          createTestVideo({ id: 'video-2', path: 'C:\\clips\\second.mp4', durationSeconds: 10, importOrder: 1 }),
        ],
        { 'video-1': 0, 'video-2': 8 },
      ),
      selectedSecond: 2,
    })
  })

  test('keeps the next clip parked in a hidden, silent second element', () => {
    const { container } = render(<VideoPreviewSurface displayScale={1} isActive />)

    const [active, parked] = container.querySelectorAll('video')
    expect(active).toHaveAttribute('data-slot', 'active')
    expect(active).toHaveAttribute('src', 'http://127.0.0.1:4321/video/import-video-1')
    expect(active).toHaveClass('opacity-100')
    expect(parked).toHaveAttribute('data-slot', 'parked')
    expect(parked).toHaveAttribute('src', 'http://127.0.0.1:4321/video/import-video-2')
    expect(parked).toHaveClass('opacity-0')
    expect(parked.muted).toBe(true)
    expect(screen.queryByRole('alert')).not.toBeInTheDocument()
  })

  test('shows the overlap warning while the playhead is inside an overlap', () => {
    useStore.setState({ selectedSecond: 9 })
    render(<VideoPreviewSurface displayScale={1} isActive />)

    expect(screen.getByRole('alert')).toHaveTextContent('Videos overlap here.')
  })
})
