import { act, renderHook } from '@testing-library/react'
import { beforeEach, describe, expect, test, vi } from 'vitest'
import useRenderWorkflow from '@/features/render-video/hooks/useRenderWorkflow'
import useStore from '@/store/useStore'
import { DEFAULT_CONFIG, DEFAULT_RENDER_PROGRESS } from '@/store/store-utils'

const renderVideoMock = vi.fn().mockResolvedValue({ started: true })

vi.mock('@/api/backend', () => ({
  getRenderProgress: vi.fn().mockResolvedValue({
    render_id: null,
    current: 0,
    total: 0,
    encoded: 0,
    status: 'rendering',
    message: '',
  }),
  listAvailableFonts: vi.fn().mockResolvedValue({
    recommendedFonts: [],
    systemFonts: [],
  }),
  openVideo: vi.fn().mockResolvedValue(undefined),
  renderPreviewFrame: vi.fn(),
}))

vi.mock('@/features/render-video/utils/render-video', () => ({
  default: renderVideoMock,
}))

describe('useRenderWorkflow', () => {
  beforeEach(() => {
    renderVideoMock.mockClear()
    useStore.setState(useStore.getInitialState(), true)
    useStore.setState({
      activitySummary: { durationSeconds: 73 },
      config: {
        ...DEFAULT_CONFIG,
        scene: {
          ...DEFAULT_CONFIG.scene,
        },
      },
      renderProgress: { ...DEFAULT_RENDER_PROGRESS },
      platformOs: 'windows',
      availableCodecs: {
        proresKs: true,
        libx264: true,
      },
    })
  })

  test('dispatches transparent override without imported-video compositing inputs and persists transparent settings', async () => {
    useStore.setState({
      importedVideoPath: 'C:\\video.mp4',
      importedVideoFps: 30,
      importedVideoDuration: 12,
      importedVideoFpsNum: 30,
      importedVideoFpsDen: 1,
      importedVideoResolution: { width: 1920, height: 1080 },
      videoSyncOffsetSeconds: 5,
    })

    const { result } = renderHook(() => useRenderWorkflow({ backendStatus: 'connected' }))

    act(() => {
      result.current.openRenderDialog()
      result.current.updateRenderSettingsDraft({
        exportMode: 'transparent',
        exportCodec: 'prores_ks',
        exportAcceleration: 'cpu',
        exportRange: {
          type: 'custom',
          from: 0,
          to: 0,
          fromTime: '00:00:05',
          toTime: '00:00:15',
        },
      })
    })

    await act(async () => {
      await result.current.handleRenderVideoConfirm()
    })

    expect(renderVideoMock).toHaveBeenCalledWith(
      expect.objectContaining({
        exportMode: 'transparent',
        exportCodec: 'prores_ks',
        exportRange: expect.objectContaining({
          type: 'custom',
          fromTime: '00:00:05',
          toTime: '00:00:15',
        }),
        importedVideoPath: null,
      }),
    )

    expect(useStore.getState().exportCodec).toBe('prores_ks')
    expect(useStore.getState().exportRange).toEqual(
      expect.objectContaining({
        type: 'custom',
        fromTime: '00:00:05',
        toTime: '00:00:15',
      }),
    )
  })

  test('dispatches composite mode with imported-video compositing inputs and keeps durable transparent settings untouched', async () => {
    useStore.setState({
      importedVideoPath: 'C:\\video.mp4',
      importedVideoFps: 30,
      importedVideoDuration: 12,
      importedVideoFpsNum: 30,
      importedVideoFpsDen: 1,
      importedVideoResolution: { width: 1920, height: 1080 },
      exportCodec: 'prores_ks',
      exportRange: {
        type: 'custom',
        from: 0,
        to: 0,
        fromTime: '00:00:01',
        toTime: '00:00:02',
      },
    })

    const { result } = renderHook(() => useRenderWorkflow({ backendStatus: 'connected' }))

    act(() => {
      result.current.openRenderDialog()
    })

    await act(async () => {
      await result.current.handleRenderVideoConfirm()
    })

    expect(renderVideoMock).toHaveBeenCalledWith(
      expect.objectContaining({
        exportMode: 'composite',
        importedVideoPath: 'C:\\video.mp4',
      }),
    )

    expect(useStore.getState().exportCodec).toBe('prores_ks')
    expect(useStore.getState().exportRange).toEqual(
      expect.objectContaining({
        type: 'custom',
        fromTime: '00:00:01',
        toTime: '00:00:02',
      }),
    )
  })
})
