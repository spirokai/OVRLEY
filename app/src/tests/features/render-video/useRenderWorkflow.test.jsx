import { act, renderHook } from '@testing-library/react'
import { beforeEach, describe, expect, test, vi } from 'vitest'
import useRenderWorkflow from '@/features/render-video/hooks/useRenderWorkflow'
import useStore from '@/store/useStore'
import { DEFAULT_CONFIG, DEFAULT_RENDER_PROGRESS } from '@/store/store-utils'
import * as renderOutput from '@/lib/file-dialog'
import * as backend from '@/api/backend'

vi.mock('@/api/backend', () => ({
  renderVideo: vi.fn(),
  subscribeBatchRenderProgress: vi.fn().mockResolvedValue(vi.fn()),
  getBatchRenderSnapshot: vi.fn(),
  getRenderProgress: vi.fn().mockResolvedValue({
    render_id: null,
    current: 0,
    total: 0,
    encoded: 0,
    status: 'rendering',
    message: '',
  }),
  subscribeRenderProgress: vi.fn().mockResolvedValue(vi.fn()),
  listAvailableFonts: vi.fn().mockResolvedValue({
    recommendedFonts: [],
    systemFonts: [],
  }),
  openVideo: vi.fn().mockResolvedValue(undefined),
  renderPreviewFrame: vi.fn(),
  suggestRenderOutputPath: vi.fn((outputKind) => Promise.resolve(outputKind === 'composite' ? 'C:\\renders\\video.mp4' : 'C:\\renders\\overlay.mov')),
}))

const renderVideoMock = vi.mocked(backend.renderVideo)

vi.mock('@/lib/file-dialog', async () => {
  const actual = await vi.importActual('@/lib/file-dialog')
  return {
    ...actual,
    loadRememberedRenderDirectory: vi.fn().mockResolvedValue(undefined),
    rememberAcceptedRenderOutput: vi.fn().mockResolvedValue(undefined),
  }
})

describe('useRenderWorkflow', () => {
  beforeEach(() => {
    renderVideoMock.mockReset().mockResolvedValue({ started: true, render_id: 'render-1', outputPath: 'C:\\renders\\overlay.mov' })
    vi.mocked(backend.openVideo).mockClear()
    vi.mocked(backend.suggestRenderOutputPath).mockImplementation((outputKind) =>
      Promise.resolve(outputKind === 'composite' ? 'C:\\renders\\video.mp4' : 'C:\\renders\\overlay.mov'),
    )
    vi.mocked(renderOutput.rememberAcceptedRenderOutput).mockClear()
    useStore.setState(useStore.getInitialState(), true)
    useStore.setState({
      activitySummary: { durationSeconds: 73 },
      parsedActivity: { sample_elapsed_seconds: [0, 73] },
      endSecond: 73,
      importedVideoFps: 30,
      importedVideoFpsNum: 30,
      importedVideoFpsDen: 1,
      importedVideoDuration: 12,
      importedVideoResolution: { width: 1920, height: 1080 },
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

  test.each([
    ['quality', 18],
    ['bitrate', 35],
  ])('passes and persists %s settings after a successful render', async (qualityType, qualityValue) => {
    useStore.setState({ importedVideoPath: 'C:\\video.mp4' })
    const { result } = renderHook(() => useRenderWorkflow({ backendStatus: 'connected' }))
    await act(async () => result.current.openRenderDialog())
    expect(result.current.renderSettingsDraft).toMatchObject({ qualityType: 'quality', qualityValue: 21 })
    act(() => result.current.updateRenderSettingsDraft({ codec: 'libx264', qualityType, qualityValue }))
    await act(async () => result.current.handleRenderVideoConfirm())
    expect(renderVideoMock).toHaveBeenCalledWith(
      expect.objectContaining({ scene: expect.objectContaining({ qualityType, qualityValue }) }),
      expect.any(Object),
      expect.any(Object),
    )
    expect(useStore.getState().renderSettings).toMatchObject({ qualityType, qualityValue })
    act(() => useStore.getState().clearRenderSession())
    await act(async () => result.current.openRenderDialog())
    expect(result.current.renderSettingsDraft).toMatchObject({ qualityType, qualityValue })
  })

  test('keeps PNG preview enabled without rerendering as the playhead crosses activity boundaries', () => {
    let renderCount = 0
    const { result } = renderHook(() => {
      renderCount += 1
      return useRenderWorkflow({ backendStatus: 'connected' })
    })
    const initialRenderCount = renderCount

    act(() => {
      useStore.getState().setSelectedSecond(-1)
      useStore.getState().setSelectedSecond(74)
    })

    expect(result.current.renderPreviewFrameDisabled).toBe(false)
    expect(renderCount).toBe(initialRenderCount)
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

    await act(async () => {
      await result.current.openRenderDialog()
      result.current.updateRenderSettingsDraft({
        exportMode: 'transparent',
        codec: 'prores_ks',
        exportAcceleration: 'cpu',
        range: {
          type: 'custom',
          from: 5.25,
          to: 15.75,
        },
      })
    })

    await act(async () => {
      await result.current.handleRenderVideoConfirm()
    })

    expect(renderVideoMock).toHaveBeenCalledWith(
      expect.objectContaining({
        scene: expect.objectContaining({ start: 5.25, end: 15.75, ffmpeg: expect.objectContaining({ codec: 'prores_ks' }) }),
      }),
      expect.any(Object),
      expect.objectContaining({ outputPath: 'C:\\renders\\video.mov' }),
    )
    expect(useStore.getState().renderSettings).toMatchObject({
      exportMode: 'transparent',
      codec: 'prores_ks',
      range: { type: 'custom', from: 5.25, to: 15.75 },
    })
  })

  test('defaults to composite with a video and to transparent without one', async () => {
    useStore.setState({ importedVideoPath: 'C:\\video.mp4' })
    const { result } = renderHook(() => useRenderWorkflow({ backendStatus: 'connected' }))

    await act(async () => result.current.openRenderDialog())
    expect(result.current.renderSettingsDraft.exportMode).toBe('composite')

    act(() => result.current.closeRenderDialog())
    act(() => useStore.setState({ importedVideoPath: null }))
    await act(async () => result.current.openRenderDialog())
    expect(result.current.renderSettingsDraft.exportMode).toBe('transparent')
  })

  test('opens targeting the current video and stays open while a batch is running', async () => {
    const { result } = renderHook(() => useRenderWorkflow({ backendStatus: 'connected' }))

    await act(async () => result.current.openRenderDialog())
    expect(result.current.renderSettingsDraft.renderTarget).toBe('current')

    act(() => result.current.updateRenderSettingsDraft({ renderTarget: 'batch' }))
    expect(useStore.getState().renderSettings.renderTarget).toBe('batch')
    const batchSnapshot = {
      batchId: 'batch-1',
      revision: 1,
      rendererBusy: true,
      phase: 'rendering',
      activeItemId: null,
      items: [],
      plannedFrames: 300,
      processedFrames: 0,
      renderedFrames: 0,
      encodedFrames: 0,
      elapsedSeconds: 0,
      estimatedSecondsRemaining: null,
      currentItemProgress: null,
    }
    vi.mocked(backend.getBatchRenderSnapshot).mockImplementation(async () => useStore.getState().batchSnapshot)
    act(() => useStore.setState({ batchSnapshot }))
    act(() => result.current.closeRenderDialog())
    expect(result.current.renderDialogPhase).toBe('confirm')

    act(() => useStore.setState({ batchSnapshot: { ...batchSnapshot, revision: 2, rendererBusy: false, phase: 'completed' } }))
    act(() => result.current.closeRenderDialog())
    expect(result.current.renderDialogPhase).toBe('closed')
    await act(async () => result.current.openRenderDialog())
    expect(result.current.renderSettingsDraft.renderTarget).toBe('batch')
    expect(result.current.renderSettingsDraft.exportMode).toBe('composite')
    expect(() => useStore.getState().setRenderTarget('invalid')).toThrow('Invalid render target')
  })

  test('preserves an explicit transparent project setting when a video is present', async () => {
    useStore.setState({
      importedVideoPath: 'C:\\video.mp4',
      renderSettings: { ...useStore.getState().renderSettings, exportMode: 'transparent' },
    })
    const { result } = renderHook(() => useRenderWorkflow({ backendStatus: 'connected' }))

    await act(async () => result.current.openRenderDialog())

    expect(result.current.renderSettingsDraft.exportMode).toBe('transparent')
  })

  test('dispatches committed composite mode with imported-video compositing inputs', async () => {
    useStore.setState({
      importedVideoPath: 'C:\\video.mp4',
      importedVideoFps: 30,
      importedVideoDuration: 12,
      importedVideoFpsNum: 30,
      importedVideoFpsDen: 1,
      importedVideoResolution: { width: 1920, height: 1080 },
      renderSettings: {
        ...useStore.getState().renderSettings,
        exportMode: 'composite',
        codec: 'libx264',
      },
    })

    const { result } = renderHook(() => useRenderWorkflow({ backendStatus: 'connected' }))

    await act(async () => {
      await result.current.openRenderDialog()
    })

    await act(async () => {
      await result.current.handleRenderVideoConfirm()
    })

    expect(renderVideoMock).toHaveBeenCalledWith(
      expect.objectContaining({
        scene: expect.objectContaining({ composite_video_path: 'C:\\video.mp4' }),
      }),
      expect.any(Object),
      expect.any(Object),
    )
  })

  test('keeps the confirmation draft for output rejection and retries the unchanged path only after overwrite confirmation', async () => {
    const existsError = Object.assign(new Error('Output already exists'), { code: 'already_exists' })
    renderVideoMock.mockRejectedValueOnce(existsError).mockResolvedValueOnce({
      started: true,
      render_id: 'render-overwrite',
      outputPath: 'C:\\renders\\overlay.mov',
    })

    const { result } = renderHook(() => useRenderWorkflow({ backendStatus: 'connected' }))
    await act(async () => {
      await result.current.openRenderDialog()
    })
    const submittedPath = result.current.renderSettingsDraft.outputPath

    await act(async () => {
      await result.current.handleRenderVideoConfirm()
    })

    expect(result.current.renderDialogPhase).toBe('confirm')
    expect(result.current.overwriteOpen).toBe(true)
    expect(result.current.renderSettingsDraft.outputPath).toBe(submittedPath)

    await act(async () => {
      await result.current.handleOverwriteConfirm()
    })

    expect(renderVideoMock).toHaveBeenCalledTimes(2)
    expect(renderVideoMock.mock.calls[1][2]).toMatchObject({ outputPath: submittedPath, overwrite: true })
  })

  test('keeps the dialog open while render acceptance is pending', async () => {
    let acceptRender
    renderVideoMock.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          acceptRender = resolve
        }),
    )

    const { result } = renderHook(() => useRenderWorkflow({ backendStatus: 'connected' }))
    await act(async () => {
      await result.current.openRenderDialog()
    })

    let submission
    await act(async () => {
      submission = result.current.handleRenderVideoConfirm()
      await Promise.resolve()
    })

    expect(useStore.getState().renderSubmissionTarget).toBe('current')
    act(() => result.current.closeRenderDialog())
    expect(result.current.renderDialogPhase).toBe('confirm')

    await act(async () => {
      acceptRender({ started: true, render_id: 'render-1', outputPath: 'C:\\renders\\overlay.mov' })
      await submission
    })
  })

  test('keeps output errors with the draft and routes general render errors globally', async () => {
    const outputError = Object.assign(new Error('The output directory does not exist'), { code: 'output_error' })
    renderVideoMock.mockRejectedValueOnce(outputError)

    const { result } = renderHook(() => useRenderWorkflow({ backendStatus: 'connected' }))
    await act(async () => {
      await result.current.openRenderDialog()
    })
    await act(async () => {
      await result.current.handleRenderVideoConfirm()
    })

    expect(result.current.renderDialogPhase).toBe('confirm')
    expect(result.current.outputPathError).toBe('The output directory does not exist')

    const renderError = Object.assign(new Error('Invalid render configuration'), { code: 'render_error' })
    renderVideoMock.mockRejectedValueOnce(renderError)
    await act(async () => {
      await result.current.handleRenderVideoConfirm()
    })

    expect(result.current.renderDialogPhase).toBe('closed')
    expect(useStore.getState().errorMessage).toBe('Invalid render configuration')
  })

  test('enters progress and remembers the exact accepted output path only after acceptance', async () => {
    const { result } = renderHook(() => useRenderWorkflow({ backendStatus: 'connected' }))
    await act(async () => {
      await result.current.openRenderDialog()
    })
    await act(async () => {
      await result.current.handleRenderVideoConfirm()
    })

    expect(result.current.renderingVideo).toBe(true)
    expect(useStore.getState().activeRenderOutputPath).toBe('C:\\renders\\overlay.mov')
    expect(renderOutput.rememberAcceptedRenderOutput).toHaveBeenCalledWith('C:\\renders\\overlay.mov')

    act(() => {
      useStore.getState().setRenderProgress({
        renderId: 'render-1',
        current: 1,
        total: 1,
        status: 'complete',
        filename: 'overlay.mov',
      })
    })

    await vi.waitFor(() => {
      expect(backend.openVideo).toHaveBeenCalledWith('C:\\renders\\overlay.mov')
    })
    expect(useStore.getState().activeRenderOutputPath).toBe(null)
  })
})
