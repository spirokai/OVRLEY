import { act, renderHook } from '@testing-library/react'
import { beforeEach, describe, expect, test, vi } from 'vitest'
import useRenderWorkflow from '@/features/render-video/hooks/useRenderWorkflow'
import useStore from '@/store/useStore'
import { DEFAULT_CONFIG, DEFAULT_RENDER_PROGRESS } from '@/store/store-utils'
import * as renderOutput from '@/lib/file-dialog'
import * as backend from '@/api/backend'
import { createTestVideo, videoStoreState } from '@/tests/helpers/videoFixtures'

vi.mock('@/api/backend', () => ({
  renderVideo: vi.fn(),
  submitBatchRender: vi.fn(),
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
const submitBatchMock = vi.mocked(backend.submitBatchRender)

function finishedBatch() {
  return {
    batchId: 'batch-1',
    snapshot: {
      batchId: 'batch-1',
      revision: 1,
      snapshotRevision: 1,
      rendererBusy: false,
      phase: 'completed',
      activeItemId: null,
      items: [],
      plannedFrames: 360,
      processedFrames: 360,
      renderedFrames: 360,
      encodedFrames: 360,
      elapsedSeconds: 1,
      estimatedSecondsRemaining: null,
      currentItemProgress: null,
      outputs: [],
    },
  }
}

// The dialog's review of the imported clip; its inputs were not changed afterwards.
const batchReview = {
  inspectionId: 'inspection',
  outputs: [{ id: 'C:\\renders\\video.mp4', pieces: [{ sourceId: 'source', startFrame: 0, endFrame: 360, skipOverlay: false }], transitions: [] }],
  offsets: { source: 0 },
  reviewedInputs: {},
}

function importVideo({ offsetSeconds = 0, telemetry } = {}) {
  useStore.setState(
    videoStoreState([createTestVideo({ id: 'clip', path: 'C:\\video.mp4', durationSeconds: 12, telemetry })], { clip: offsetSeconds }),
  )
}

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
    submitBatchMock.mockReset().mockResolvedValue(finishedBatch())
    vi.mocked(backend.getBatchRenderSnapshot).mockResolvedValue(finishedBatch().snapshot)
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
    importVideo()
    const { result } = renderHook(() => useRenderWorkflow({ backendStatus: 'connected' }))
    await act(async () => result.current.openRenderDialog())
    expect(result.current.renderSettingsDraft).toMatchObject({ qualityType: 'quality', qualityValue: 21 })
    act(() => result.current.updateRenderSettingsDraft({ codec: 'libx264', qualityType, qualityValue }))
    await act(async () => result.current.handleRenderVideoConfirm(batchReview))
    expect(submitBatchMock).toHaveBeenCalledWith(expect.objectContaining({ encoding: expect.objectContaining({ qualityType, qualityValue }) }))
    expect(useStore.getState().renderSettings).toMatchObject({ qualityType, qualityValue })
    act(() => result.current.closeRenderDialog())
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

  test('renders the activity overlay without videos and persists its settings', async () => {
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
      expect.objectContaining({ outputPath: 'C:\\renders\\overlay.mov' }),
    )
    expect(useStore.getState().renderSettings).toMatchObject({
      exportMode: 'transparent',
      codec: 'prores_ks',
      range: { type: 'custom', from: 5.25, to: 15.75 },
    })
  })

  test('defaults to composite with a video and to transparent without one', async () => {
    importVideo()
    const { result } = renderHook(() => useRenderWorkflow({ backendStatus: 'connected' }))

    await act(async () => result.current.openRenderDialog())
    expect(result.current.renderSettingsDraft.exportMode).toBe('composite')

    act(() => result.current.closeRenderDialog())
    act(() => useStore.setState(videoStoreState([])))
    await act(async () => result.current.openRenderDialog())
    expect(result.current.renderSettingsDraft.exportMode).toBe('transparent')
  })

  test('opens targeting the current video and stays open while a batch is running', async () => {
    const { result } = renderHook(() => useRenderWorkflow({ backendStatus: 'connected' }))

    await act(async () => result.current.openRenderDialog())
    expect(result.current.renderSettingsDraft.outputMode).toBe('combined')

    act(() => result.current.updateRenderSettingsDraft({ outputMode: 'separate' }))
    expect(useStore.getState().renderSettings.outputMode).toBe('separate')
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
    expect(result.current.renderSettingsDraft.outputMode).toBe('separate')
    expect(result.current.renderSettingsDraft.exportMode).toBe('composite')
    expect(() => useStore.getState().setOutputMode('invalid')).toThrow('Invalid output mode')
  })

  test('preserves an explicit transparent project setting when a video is present', async () => {
    importVideo()
    useStore.setState({ renderSettings: { ...useStore.getState().renderSettings, exportMode: 'transparent' } })
    const { result } = renderHook(() => useRenderWorkflow({ backendStatus: 'connected' }))

    await act(async () => result.current.openRenderDialog())

    expect(result.current.renderSettingsDraft.exportMode).toBe('transparent')
  })

  test('renders videos as one combined file and asks before replacing it', async () => {
    importVideo()
    useStore.setState({ renderSettings: { ...useStore.getState().renderSettings, exportMode: 'composite', codec: 'libx264' } })
    const existsError = Object.assign(new Error('Output already exists'), { code: 'already_exists' })
    submitBatchMock.mockRejectedValueOnce(existsError)
    const { result } = renderHook(() => useRenderWorkflow({ backendStatus: 'connected' }))
    await act(async () => result.current.openRenderDialog())

    // Videos render only from the dialog's review.
    await act(async () => result.current.handleRenderVideoConfirm(null))
    expect(submitBatchMock).not.toHaveBeenCalled()
    await act(async () => result.current.handleRenderVideoConfirm(batchReview))
    expect(renderVideoMock).not.toHaveBeenCalled()
    expect(submitBatchMock).toHaveBeenLastCalledWith(
      expect.objectContaining({
        inspectionId: 'inspection',
        outputs: batchReview.outputs,
        destination: { kind: 'file', path: 'C:\\renders\\video.mp4', overwrite: false },
      }),
    )
    expect(result.current.overwriteOpen).toBe(true)
    expect(result.current.pendingOverwritePath).toBe('C:\\renders\\video.mp4')

    await act(async () => result.current.handleOverwriteConfirm())
    expect(submitBatchMock).toHaveBeenCalledTimes(2)
    expect(submitBatchMock.mock.calls[1][0]).toMatchObject({
      outputs: batchReview.outputs,
      destination: { kind: 'file', path: 'C:\\renders\\video.mp4', overwrite: true },
    })
    expect(result.current.overwriteOpen).toBe(false)
  })

  test('enables an MP4-only render from ready clip telemetry and renders it with its embedded telemetry', async () => {
    const telemetryActivity = { sample_elapsed_seconds: [0, 12], trim_end_seconds: 12 }
    useStore.setState({ activitySummary: null, parsedActivity: null, startSecond: 0, endSecond: 0 })
    importVideo({ offsetSeconds: 40, telemetry: { status: 'absent', activity: null } })
    const { result } = renderHook(() => useRenderWorkflow({ backendStatus: 'connected' }))

    expect(result.current.renderDisabled).toBe(true)
    expect(result.current.renderTooltipContent).toBe('Load a GPX/FIT activity first')

    act(() => importVideo({ offsetSeconds: 40, telemetry: { status: 'ready', activity: telemetryActivity } }))
    expect(result.current.renderDisabled).toBe(false)
    expect(result.current.renderTooltipContent).toBe(null)

    await act(async () => result.current.openRenderDialog())
    expect(result.current.renderSettingsDraft.exportMode).toBe('composite')
    act(() => result.current.updateRenderSettingsDraft({ codec: 'libx264' }))
    await act(async () => result.current.handleRenderVideoConfirm(batchReview))

    expect(submitBatchMock).toHaveBeenCalledWith(expect.objectContaining({ activity: { mode: 'embeddedActivity' } }))
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

    expect(useStore.getState().renderSubmissionTarget).toBe('single')
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
