import { act, renderHook } from '@testing-library/react'
import { beforeEach, describe, expect, test, vi } from 'vitest'
import useBatchRenderWorkflow from '@/features/render-video/hooks/useBatchRenderWorkflow'
import { DEFAULT_EXPORT_RANGE } from '@/features/template-manager'
import useStore from '@/store/useStore'
import { DEFAULT_CONFIG } from '@/store/store-utils'

const { renderVideoMock, loadVideoPathMock, clearImportedVideoMock } = vi.hoisted(() => ({
  renderVideoMock: vi.fn(),
  loadVideoPathMock: vi.fn(),
  clearImportedVideoMock: vi.fn(),
}))

vi.mock('@/api/backend', () => ({
  cancelRender: vi.fn(),
  getRenderProgress: vi.fn().mockResolvedValue({ render_id: 'render-1', status: 'complete' }),
  listAvailableFonts: vi.fn().mockResolvedValue({ recommendedFonts: [], systemFonts: [] }),
  listDirectoryVideoFiles: vi.fn(),
  subscribeRenderProgress: vi.fn().mockResolvedValue(vi.fn()),
}))

vi.mock('@/features/render-video/utils/render-video', () => ({
  default: renderVideoMock,
}))

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
  exportBitrate: 35,
  exportRange: { ...DEFAULT_EXPORT_RANGE },
}

describe('useBatchRenderWorkflow', () => {
  beforeEach(() => {
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
    useStore.getState().setBatchOutputFolder('C:\\renders')
  })

  test('renders each queued video with the dialog draft settings and commits them', async () => {
    const { result } = renderHook(() => useBatchRenderWorkflow({ settings: batchSettings }))

    await act(async () => {
      await result.current.runBatch()
    })

    expect(loadVideoPathMock).toHaveBeenCalledWith('C:\\videos\\ride.mp4')
    expect(renderVideoMock).toHaveBeenCalledWith(
      expect.objectContaining({
        config: expect.objectContaining({ scene: expect.objectContaining({ fps: 24 }) }),
        exportMode: 'composite',
        exportCodec: 'libx264',
        exportBitrate: 35,
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
      bitrateMbps: 35,
    })
  })
})
