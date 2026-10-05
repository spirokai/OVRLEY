import { act, renderHook, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, test, vi } from 'vitest'
import useBatchRenderWorkflow from '@/features/render-video/hooks/useBatchRenderWorkflow'
import { DEFAULT_EXPORT_RANGE } from '@/features/template-manager'
import useStore from '@/store/useStore'
import { DEFAULT_CONFIG } from '@/store/store-utils'
import { listDirectoryVideoFiles } from '@/api/backend'
import { prepareVideoPath } from '@/features/video-preview/hooks/useVideoImport'
import { openDirectoryPath } from '@/lib/file-dialog'

const { renderVideoMock, loadVideoPathMock, clearImportedVideoMock } = vi.hoisted(() => ({
  renderVideoMock: vi.fn(),
  loadVideoPathMock: vi.fn(),
  clearImportedVideoMock: vi.fn(),
}))

vi.mock('@/api/backend', () => ({
  cancelRender: vi.fn(),
  getRenderProgress: vi.fn().mockResolvedValue({
    render_id: 'render-1',
    status: 'complete',
    current: 600,
    total: 600,
    encoded: 600,
    rendering_fps: null,
    estimated_seconds_remaining: null,
  }),
  listAvailableFonts: vi.fn().mockResolvedValue({ recommendedFonts: [], systemFonts: [] }),
  listDirectoryVideoFiles: vi.fn(),
  subscribeRenderProgress: vi.fn().mockResolvedValue(vi.fn()),
}))

vi.mock('@/features/render-video/utils/render-video', () => ({
  default: renderVideoMock,
}))

vi.mock('@/lib/file-dialog', () => ({ openDirectoryPath: vi.fn() }))

vi.mock('@/features/video-preview/hooks/useVideoImport', () => ({
  default: () => ({ loadVideoPath: loadVideoPathMock, clearImportedVideo: clearImportedVideoMock }),
  prepareVideoPath: vi.fn(),
}))

const batchSettings = {
  renderTarget: 'batch',
  fps: 24,
  updateRate: 2,
  exportMode: 'composite',
  exportCodec: 'libx264',
  exportAcceleration: 'cpu',
  qualityType: 'bitrate',
  qualityValue: 35,
  exportRange: { ...DEFAULT_EXPORT_RANGE },
}

describe('useBatchRenderWorkflow', () => {
  beforeEach(() => {
    vi.mocked(listDirectoryVideoFiles).mockReset().mockResolvedValue([])
    vi.mocked(prepareVideoPath).mockReset()
    vi.mocked(openDirectoryPath).mockReset()
    renderVideoMock.mockReset().mockResolvedValue({ started: true, render_id: 'render-1', outputPath: 'C:\\renders\\ride.mp4' })
    clearImportedVideoMock.mockReset().mockResolvedValue(undefined)
    loadVideoPathMock.mockReset().mockImplementation(async (path) => {
      useStore.setState({
        importedVideoPath: path,
        importedVideoFps: 60,
        importedVideoDuration: 10,
        importedVideoResolution: { width: 1920, height: 1080 },
      })
    })
    useStore.setState(useStore.getInitialState(), true)
    useStore.setState({
      config: { ...DEFAULT_CONFIG, scene: { ...DEFAULT_CONFIG.scene, fps: 30 } },
      parsedActivity: { samples: [] },
    })
    useStore.getState().setBatchQueueFromPaths(['C:\\videos\\ride.mp4'])
    useStore.getState().setBatchItemMetadata(useStore.getState().batchQueue[0].id, { duration: 10, fps: 60, activityDuration: 10 })
    useStore.getState().setBatchOutputFolder('C:\\renders')
  })

  test('renders each queued video with the dialog draft settings and commits them', async () => {
    const { result } = renderHook(() => useBatchRenderWorkflow({ phase: 'confirm', settings: batchSettings }))

    await act(async () => {
      await result.current.runBatch()
    })

    expect(loadVideoPathMock).toHaveBeenCalledWith('C:\\videos\\ride.mp4')
    expect(renderVideoMock).toHaveBeenCalledWith(
      expect.objectContaining({
        config: expect.objectContaining({ scene: expect.objectContaining({ fps: 24 }) }),
        exportMode: 'composite',
        exportCodec: 'libx264',
        qualityType: 'bitrate',
        qualityValue: 35,
        exportRange: DEFAULT_EXPORT_RANGE,
        importedVideoPath: 'C:\\videos\\ride.mp4',
        outputPath: 'C:\\renders\\ride.mp4',
        overwrite: true,
      }),
    )
    expect(useStore.getState().batchQueue[0].status).toBe('done')
    expect(useStore.getState().batchRunning).toBe(false)
    expect(useStore.getState().renderSettings).toMatchObject({
      fps: 24,
      widgetUpdateRate: 2,
      exportMode: 'composite',
      codec: 'libx264',
      qualityType: 'bitrate',
      qualityValue: 35,
    })
  })

  test('scans a restored folder on opening batch mode and checks sync before allowing rendering', async () => {
    const paths = ['C:\\videos\\ride.mp4', 'C:\\videos\\outside.mp4']
    useStore.getState().clearBatchQueue()
    useStore.getState().setBatchVideoFolder('C:\\videos')
    useStore.setState({
      activitySummary: { syncTime: '2026-10-05T12:00:00Z', endTime: '2026-10-05T13:00:00Z', timezone: 'UTC' },
    })
    vi.mocked(listDirectoryVideoFiles).mockResolvedValue(paths)
    let finishChecks
    const checks = new Promise((resolve) => {
      finishChecks = resolve
    })
    vi.mocked(prepareVideoPath).mockImplementation(async (path) => {
      await checks
      return {
        importedVideoState: {
          importedVideoCreationTime: path === paths[0] ? '2026-10-05T12:10:00Z' : '2026-10-05T15:00:00Z',
          importedVideoTimeSource: 'ffprobe',
          importedVideoDuration: 60,
          importedVideoFps: 60,
        },
      }
    })
    const { result, rerender } = renderHook(({ phase }) => useBatchRenderWorkflow({ phase, settings: batchSettings }), {
      initialProps: { phase: 'closed' },
    })
    expect(listDirectoryVideoFiles).not.toHaveBeenCalled()
    rerender({ phase: 'confirm' })
    await waitFor(() => expect(prepareVideoPath).toHaveBeenCalledTimes(2))
    expect(result.current.batchQueue.map((item) => item.status)).toEqual(['checking', 'checking'])
    await act(async () => result.current.runBatch())
    expect(renderVideoMock).not.toHaveBeenCalled()
    await act(async () => {
      finishChecks()
      await checks
    })
    await waitFor(() => expect(result.current.batchQueue.map((item) => item.status)).toEqual(['pending', 'blocked']))
    await act(async () => result.current.runBatch())
    expect(loadVideoPathMock).toHaveBeenCalledOnce()
    expect(loadVideoPathMock).toHaveBeenCalledWith(paths[0])
  })

  test('does not repopulate a cleared folder when directory listing finishes late', async () => {
    useStore.getState().clearBatchQueue()
    useStore.getState().setBatchVideoFolder('C:\\videos')
    let finishListing
    vi.mocked(listDirectoryVideoFiles).mockImplementation(
      () =>
        new Promise((resolve) => {
          finishListing = resolve
        }),
    )
    const { result } = renderHook(() => useBatchRenderWorkflow({ phase: 'confirm', settings: batchSettings }))
    act(() => result.current.clearBatchQueue())
    await act(async () => finishListing(['C:\\videos\\ride.mp4']))
    expect(result.current.batchVideoFolder).toBeNull()
    expect(result.current.batchQueue).toEqual([])
    expect(prepareVideoPath).not.toHaveBeenCalled()
  })

  test('rescans when the user picks the same folder again', async () => {
    useStore.getState().clearBatchQueue()
    useStore.getState().setBatchVideoFolder('C:\\videos')
    vi.mocked(openDirectoryPath).mockResolvedValue('C:\\videos')
    const { result } = renderHook(() => useBatchRenderWorkflow({ phase: 'confirm', settings: batchSettings }))
    await waitFor(() => expect(listDirectoryVideoFiles).toHaveBeenCalledOnce())
    await act(async () => result.current.pickVideoFolder())
    await waitFor(() => expect(listDirectoryVideoFiles).toHaveBeenCalledTimes(2))
  })
})
