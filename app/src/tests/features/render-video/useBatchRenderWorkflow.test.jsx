import { act, renderHook, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, test, vi } from 'vitest'
import * as backend from '@/api/backend'
import useRenderVideoDialogState from '@/features/render-video/hooks/useRenderVideoDialogState'
import useRenderExecution from '@/features/render-video/hooks/useRenderExecution'
import { DEFAULT_RENDER_SETTINGS } from '@/store/slices/createRenderSettingsSlice'
import useRenderVideoDerivedState from '@/features/render-video/hooks/useRenderVideoDerivedState'
import useProjectDocumentState from '@/features/projects/hooks/useProjectDocumentState'
import useStore from '@/store/useStore'
import { DEFAULT_CONFIG } from '@/store/store-utils'
import { openDirectoryPath } from '@/lib/file-dialog'
import batchTemplate from '../../../../../src-tauri/ovrley_core/tests/fixtures/config/batch-template.json'

vi.mock('@/api/backend', () => ({
  createVideoInspection: vi.fn(),
  inspectVideoSource: vi.fn(),
  disposeVideoInspection: vi.fn(),
  planBatchOutputs: vi.fn(),
  submitBatchRender: vi.fn(),
  subscribeBatchRenderProgress: vi.fn(),
  getBatchRenderSnapshot: vi.fn(),
  cancelBatchRender: vi.fn(),
  listDirectoryVideoFiles: vi.fn(),
  listAvailableFonts: vi.fn().mockResolvedValue({ recommendedFonts: [], systemFonts: [] }),
}))
vi.mock('@/lib/file-dialog', () => ({ openDirectoryPath: vi.fn() }))

const folder = 'C:/videos'
const reference = `${folder}/reference.mp4`
const next = `${folder}/ride.part 2.mp4`
const removed = `${folder}/removed.mp4`
const blocked = `${folder}/outside.mp4`
const settings = {
  ...DEFAULT_RENDER_SETTINGS,
  renderTarget: 'batch',
  fps: 24,
  widgetUpdateRate: 2,
  exportMode: 'composite',
  codec: 'libx264',
  qualityType: 'bitrate',
  qualityValue: 35,
}
let sessionNumber
let observe
let acceptedRequest
let latest
let unlisten

function useBatchRenderWorkflow(options) {
  const execution = useRenderExecution()
  const dialog = useRenderVideoDialogState({
    ...options,
    onSettingsChange: () => {},
    onClose: () => {},
    onConfirm: (batchReview) => execution.submit({ settings: options.settings, batchReview, onAccepted: () => {} }),
    onCancel: execution.cancel,
  })
  return {
    ...dialog,
    runBatch: dialog.onConfirm,
    cancelBatch: dialog.handleCancel,
  }
}

function source(inspectionId, path) {
  const creationTime = path === reference ? '2026-10-05T11:59:48Z' : path === blocked ? '2026-10-05T14:00:00Z' : '2026-10-05T12:00:40Z'
  return {
    sourceId: `${inspectionId}:${path}`,
    metadata: {
      path,
      duration: 60,
      fps: 60,
      fpsNum: 60,
      fpsDen: 1,
      resolution: { width: 1920, height: 1080 },
      rotationDegrees: 0,
      creationTime,
      syncTime: null,
      timeSource: 'ffprobe',
    },
    displayResolution: { width: 1920, height: 1080 },
    stamp: { sizeBytes: 10, modifiedAtUnixNanos: '1' },
  }
}
function snapshot(phase = 'accepted', revision = 1) {
  const terminal = ['completed', 'completedWithErrors', 'failed', 'cancelled'].includes(phase)
  return {
    batchId: 'batch-1',
    revision,
    snapshotRevision: revision,
    phase,
    rendererBusy: !terminal,
    activeItemId: null,
    plannedFrames: acceptedRequest.jobs.length * 3600,
    processedFrames: terminal ? acceptedRequest.jobs.length * 3600 : 0,
    renderedFrames: 0,
    encodedFrames: 0,
    currentItemProgress: null,
    elapsedSeconds: 0,
    estimatedSecondsRemaining: null,
    items: acceptedRequest.jobs.map((job) => ({
      id: job.id,
      phase: terminal ? 'finished' : 'queued',
      plannedFrames: 3600,
      currentFrames: 0,
      renderedFrames: 0,
      encodedFrames: 0,
      outcome: terminal ? { status: 'failed', message: 'Telemetry unavailable' } : null,
    })),
    outputs: [],
    resultCounts: { succeeded: 0, failed: terminal ? acceptedRequest.jobs.length : 0, cancelled: 0, unstarted: 0 },
  }
}
function editorState() {
  const state = useStore.getState()
  return Object.fromEntries(
    [
      'config',
      'globalDefaults',
      'parsedActivity',
      'activitySummary',
      'parsedActivitySource',
      'importedVideoPath',
      'importedVideoPreviewUrl',
      'importedVideoImportId',
      'importedVideoResolution',
      'importedVideoFps',
      'importedVideoDuration',
      'selectedSecond',
      'timelineViewport',
      'videoSyncOffsetSeconds',
      'videoSyncOffsetPreviewSeconds',
      'videoSyncTimezoneMode',
      'manualVideoSync',
      'startSecond',
      'endSecond',
    ].map((key) => [key, state[key]]),
  )
}

beforeEach(() => {
  vi.clearAllMocks()
  sessionNumber = 0
  unlisten = vi.fn()
  observe = null
  vi.mocked(backend.createVideoInspection).mockImplementation(async () => ({ inspectionId: `inspection-${++sessionNumber}` }))
  vi.mocked(backend.inspectVideoSource).mockImplementation(async (id, path) => source(id, path))
  vi.mocked(backend.disposeVideoInspection).mockResolvedValue(undefined)
  vi.mocked(backend.listDirectoryVideoFiles).mockResolvedValue([reference, next, removed, blocked])
  vi.mocked(backend.planBatchOutputs).mockImplementation(async ({ sourceIds }) => ({
    status: 'planned',
    plans: sourceIds.map((sourceId) => ({ sourceId, plannedFrames: 3600 })),
  }))
  vi.mocked(backend.subscribeBatchRenderProgress).mockImplementation(async (handler) => {
    observe = handler
    return unlisten
  })
  vi.mocked(backend.submitBatchRender).mockImplementation(async (request) => {
    acceptedRequest = request
    latest = snapshot()
    observe({ kind: 'snapshot', data: { ...latest, revision: 2, snapshotRevision: 2, phase: 'preparing', activeItemId: request.jobs[0].id } })
    return { batchId: latest.batchId, snapshot: latest }
  })
  vi.mocked(backend.getBatchRenderSnapshot).mockImplementation(async () => latest)
  vi.mocked(backend.cancelBatchRender).mockImplementation(async () => ({ ...latest, revision: 3, phase: 'cancelling' }))
  useStore.setState(useStore.getInitialState(), true)
  useStore.setState({
    config: structuredClone(DEFAULT_CONFIG),
    parsedActivity: { sample_elapsed_seconds: [0, 3600] },
    parsedActivitySource: 'activity-file',
    activitySummary: { syncTime: '2026-10-05T12:00:00Z', endTime: '2026-10-05T13:00:00Z', timezone: 'UTC' },
    importedVideoPath: reference,
    importedVideoCreationTime: '2026-10-05T11:59:48Z',
    importedVideoTimeSource: 'ffprobe',
    importedVideoResolution: { width: 640, height: 480 },
    importedVideoFps: 30,
    importedVideoDuration: 120,
    importedVideoPreviewUrl: 'http://preview/original',
    importedVideoImportId: 'original-preview',
    videoSyncOffsetSeconds: -9,
    videoSyncOffsetPreviewSeconds: 123,
    videoSyncTimezoneMode: 'utc',
    selectedSecond: 27,
    timelineViewport: { viewStart: 20, viewEnd: 40 },
    availableCodecs: { libx264: true },
    platformOs: 'windows',
  })
  useStore.getState().setBatchVideoFolder(folder)
  useStore.getState().setBatchOutputFolder('C:/renders')
})

describe('native batch workflow', () => {
  test('uses native warmup and FPS while compact ticks preserve the queue and ignore stale transitions', async () => {
    vi.mocked(backend.submitBatchRender).mockImplementation(async (request) => {
      acceptedRequest = request
      latest = snapshot()
      // A newer tick may arrive before its queue transition and acceptance.
      observe({
        kind: 'progress',
        data: {
          batchId: 'batch-1',
          revision: 3,
          snapshotRevision: 2,
          processedFrames: 0,
          renderedFrames: 0,
          encodedFrames: 0,
          currentItemProgress: null,
          elapsedSeconds: 1,
          estimatedSecondsRemaining: null,
        },
      })
      observe({ kind: 'snapshot', data: snapshot('preparing', 2) })
      return { batchId: 'batch-1', snapshot: latest }
    })
    const { result } = renderHook(() => useBatchRenderWorkflow({ phase: 'confirm', settings }))
    await waitFor(() => expect(result.current.batchReady).toBe(true))
    await act(async () => result.current.runBatch())
    expect(result.current.batchSnapshot.revision).toBe(3)
    expect(result.current.batchSnapshot.phase).toBe('preparing')
    const rendering = {
      ...snapshot('rendering', 4),
      activeItemId: reference,
      currentItemProgress: {
        itemId: reference,
        plannedFrames: 3600,
        currentFrames: 100,
        renderedFrames: 50,
        encodedFrames: 0,
        elapsedSeconds: 100,
        estimatedSecondsRemaining: null,
        renderingFps: null,
      },
    }
    rendering.items[0].phase = 'rendering'
    act(() => observe({ kind: 'snapshot', data: rendering }))
    // Even with nonzero frames and elapsed time, native warmup stays unavailable.
    expect(result.current.batchProgress.renderingFps).toBeNull()
    expect(result.current.currentItemProgress.renderingFps).toBeNull()
    const queue = result.current.batchQueue
    const visibleQueue = result.current.visibleBatchQueue
    const items = useStore.getState().batchSnapshot.items
    const outputs = useStore.getState().batchSnapshot.outputs
    const tick = {
      batchId: 'batch-1',
      revision: 5,
      snapshotRevision: 4,
      processedFrames: 200,
      renderedFrames: 100,
      encodedFrames: 0,
      currentItemProgress: { ...rendering.currentItemProgress, currentFrames: 200, renderedFrames: 100, renderingFps: 120 },
      elapsedSeconds: 101,
      estimatedSecondsRemaining: 50,
    }
    act(() => observe({ kind: 'progress', data: tick }))
    expect(result.current.batchProgress.renderingFps).toBe(120)
    expect(result.current.currentItemProgress.renderingFps).toBe(120)
    expect(result.current.batchQueue).toBe(queue)
    expect(result.current.visibleBatchQueue).toBe(visibleQueue)
    expect(useStore.getState().batchSnapshot.items).toBe(items)
    expect(useStore.getState().batchSnapshot.outputs).toBe(outputs)
    act(() => {
      observe({ kind: 'progress', data: { ...tick, revision: 2 } })
      observe({ kind: 'progress', data: { ...tick, revision: 6, snapshotRevision: 5 } })
    })
    expect(useStore.getState().batchSnapshot.revision).toBe(5)
    act(() => observe({ kind: 'snapshot', data: snapshot('cancelled', 7) }))
  })

  test('keeps an automatically non-overlapping clip blocked even when calibration would move it into the activity', async () => {
    useStore.setState({ videoSyncOffsetSeconds: -4009 })
    const { result } = renderHook(() => useBatchRenderWorkflow({ phase: 'confirm', settings }))
    await waitFor(() => expect(result.current.batchReady).toBe(true))
    expect(result.current.batchQueue.find((row) => row.path === next).status).toBe('pending')
    expect(result.current.batchQueue.find((row) => row.path === blocked).status).toBe('blocked')
    await act(async () => result.current.runBatch())
    expect(acceptedRequest.jobs.some((job) => job.id === blocked)).toBe(false)
  })

  test('defers an invalid reference timestamp to submission without blocking candidate inspection', async () => {
    useStore.setState({ importedVideoCreationTime: null })
    const { result } = renderHook(() => useBatchRenderWorkflow({ phase: 'confirm', settings }))
    await waitFor(() => expect(result.current.batchReady).toBe(true))
    expect(result.current.batchQueue.find((row) => row.path === next).status).toBe('pending')
    await expect(result.current.runBatch()).rejects.toThrow('Could not calibrate reference video')
    expect(backend.submitBatchRender).not.toHaveBeenCalled()
  })

  test('keeps candidate eligibility independent of an unavailable out-of-folder reference', async () => {
    const referencePath = 'C:/reference/missing.mp4'
    useStore.setState({ importedVideoPath: referencePath })
    vi.mocked(backend.inspectVideoSource).mockImplementation(async (id, path) => {
      if (path === referencePath) throw new Error('Reference video is unavailable')
      return source(id, path)
    })
    const { result } = renderHook(() => useBatchRenderWorkflow({ phase: 'confirm', settings }))
    await waitFor(() => expect(result.current.batchReviewError).toBe('Reference video is unavailable'))
    expect(result.current.batchQueue.find((row) => row.path === next).status).toBe('pending')
    expect(result.current.batchReady).toBe(false)
  })

  test('publishes each inspected row immediately while later sources are still probing', async () => {
    let finish
    vi.mocked(backend.inspectVideoSource).mockImplementation(async (id, path) => {
      if (path === next)
        await new Promise((resolve) => {
          finish = resolve
        })
      return source(id, path)
    })
    const { result } = renderHook(() => useBatchRenderWorkflow({ phase: 'confirm', settings }))
    await waitFor(() => expect(result.current.batchQueue.find((row) => row.path === reference)?.status).toBe('pending'))
    expect(result.current.batchQueue.find((row) => row.path === next).status).toBe('checking')
    expect(result.current.batchReady).toBe(false)
    expect(backend.planBatchOutputs).not.toHaveBeenCalled()
    await act(async () => finish())
    await waitFor(() => expect(result.current.batchReady).toBe(true))
  })
  test('submits once with shared calibration and queue choices; keeps the editor and history through mixed results and unmount', async () => {
    const before = editorState()
    const history = useStore.temporal.getState().pastStates
    const { result, unmount } = renderHook(() => ({
      batch: useBatchRenderWorkflow({ phase: 'confirm', settings }),
      render: useRenderVideoDerivedState({ settings: { ...settings, renderTarget: 'current' } }),
      project: useProjectDocumentState(),
    }))
    await waitFor(() => expect(result.current.batch.batchReady).toBe(true))
    expect(result.current.batch.batchQueue.find((row) => row.path === blocked).status).toBe('blocked')
    act(() => {
      result.current.batch.setBatchItemSkipOverlay(next, true)
      result.current.batch.removeBatchQueueItem(removed)
    })
    await waitFor(() => expect(result.current.batch.batchReady).toBe(true))
    await act(async () => result.current.batch.runBatch())
    expect(backend.submitBatchRender).toHaveBeenCalledOnce()
    expect(backend.subscribeBatchRenderProgress.mock.invocationCallOrder[0]).toBeLessThan(backend.submitBatchRender.mock.invocationCallOrder[0])
    expect(acceptedRequest).toMatchObject({
      inspectionId: 'inspection-1',
      outputDirectory: 'C:/renders',
      encoding: { fps: 24, updateRate: 2 },
      activity: {
        mode: 'externalActivity',
        timezoneMode: 'utc',
        reference: { committedOffsetSeconds: -9, automaticOffsetSeconds: -12 },
        automaticOffsets: { [`inspection-1:${reference}`]: -12, [`inspection-1:${next}`]: 40 },
      },
      jobs: [
        { id: reference, sourceId: `inspection-1:${reference}`, skipOverlay: false },
        { id: next, sourceId: `inspection-1:${next}`, skipOverlay: true },
      ],
    })
    expect(Object.isFrozen(acceptedRequest)).toBe(true)
    // This same wire template is accepted by the Rust batch service test.
    expect(JSON.parse(JSON.stringify(acceptedRequest.template))).toEqual(batchTemplate)
    expect(result.current.batch.batchSnapshot.phase).toBe('preparing') // Older acceptance/read must not roll back the event.
    expect(result.current.batch.batchProgress.total).toBe(7200)
    expect(result.current.render.renderStartDisabled).toBe(true)
    expect(result.current.project.conflictingOperation).toBe(true)
    expect(editorState()).toEqual(before)
    expect(useStore.temporal.getState().pastStates).toBe(history)
    expect(useStore.getState().renderSettings).toMatchObject({ fps: 24, widgetUpdateRate: 2, codec: 'libx264', qualityValue: 35 })
    unmount()
    latest = snapshot('completedWithErrors', 4)
    latest.items[0].outcome = { status: 'succeeded', outputPath: 'C:/renders/reference_video.mp4' }
    latest.resultCounts = { succeeded: 1, failed: 1, cancelled: 0, unstarted: 0 }
    latest.outputs = [{ itemId: reference, outputPath: 'C:/renders/reference_video.mp4' }]
    act(() => observe({ kind: 'snapshot', data: latest }))
    expect(useStore.getState().batchSnapshot.phase).toBe('completedWithErrors')
    expect(unlisten).toHaveBeenCalled()
    const resumed = renderHook(() => useBatchRenderWorkflow({ phase: 'confirm', settings }))
    await waitFor(() => expect(resumed.result.current.batchFinished).toBe(true))
    expect(resumed.result.current.batchQueue.map((row) => row.status)).toEqual(['succeeded', 'failed'])
    expect(editorState()).toEqual(before)
    expect(useStore.temporal.getState().pastStates).toBe(history)
  })

  test('close/reopen retains in-flight inspections; folder replacement disposes sessions and context changes invalidate Start', async () => {
    let finish
    vi.mocked(backend.inspectVideoSource).mockImplementationOnce(
      (id, path) =>
        new Promise((resolve) => {
          finish = () => resolve(source(id, path))
        }),
    )
    const { result, rerender } = renderHook(({ phase }) => useBatchRenderWorkflow({ phase, settings }), { initialProps: { phase: 'confirm' } })
    await waitFor(() => expect(finish).toBeDefined())
    expect(result.current.batchReady).toBe(false)
    rerender({ phase: 'closed' })
    expect(backend.disposeVideoInspection).not.toHaveBeenCalled()
    await act(async () => finish())
    rerender({ phase: 'confirm' })
    await waitFor(() => expect(result.current.batchReady).toBe(true))
    expect(result.current.batchQueue[0].source.sourceId).toContain('inspection-1:')
    expect(backend.listDirectoryVideoFiles).toHaveBeenCalledOnce()
    let finishPlan
    vi.mocked(backend.planBatchOutputs).mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          finishPlan = resolve
        }),
    )
    act(() => useStore.setState({ videoSyncOffsetSeconds: -8, videoSyncTimezoneMode: 'local' }))
    expect(result.current.batchReady).toBe(false)
    await act(async () => result.current.runBatch())
    expect(backend.submitBatchRender).not.toHaveBeenCalled()
    await act(async () => finishPlan({ status: 'planned', plans: [] }))
    await waitFor(() => expect(result.current.batchReady).toBe(true))
    vi.mocked(openDirectoryPath).mockResolvedValue('C:/other')
    await act(async () => result.current.pickVideoFolder())
    await waitFor(() => expect(result.current.batchReady).toBe(true))
    expect(backend.disposeVideoInspection).toHaveBeenCalledWith('inspection-1')
    expect(backend.listDirectoryVideoFiles).toHaveBeenLastCalledWith('C:/other')
  })

  test('identifies stale submission sources and requires explicit fresh inspection, including an out-of-folder reference', async () => {
    useStore.setState({ importedVideoPath: 'C:/reference/reference.mp4' })
    const { result } = renderHook(() => useBatchRenderWorkflow({ phase: 'confirm', settings }))
    await waitFor(() => expect(result.current.batchReady).toBe(true))
    expect(backend.inspectVideoSource).toHaveBeenCalledWith('inspection-1', 'C:/reference/reference.mp4')
    vi.mocked(backend.submitBatchRender).mockRejectedValueOnce({
      code: 'reinspectionRequired',
      message: 'Sources changed',
      issues: [{ sourceId: `inspection-1:${next}`, path: next, reason: 'sourceChanged' }],
    })
    await act(async () => result.current.runBatch())
    expect(result.current.batchReady).toBe(false)
    expect(result.current.batchReviewError).toBe('reinspectionRequired')
    expect(result.current.batchQueue.find((row) => row.path === next).status).toBe('blocked')
    await act(async () => result.current.runBatch())
    expect(backend.submitBatchRender).toHaveBeenCalledOnce()
    act(() => result.current.setBatchItemSkipOverlay(reference, true))
    expect(result.current.batchReady).toBe(false)
    act(() => result.current.refreshInspection())
    await waitFor(() => expect(result.current.batchReady).toBe(true))
    expect(backend.disposeVideoInspection).toHaveBeenCalledWith('inspection-1')
    expect(result.current.batchQueue[0].source.sourceId).toContain('inspection-2:')
  })

  test.each(['failed', 'cancelled'])('recognizes %s after native cleanup without a successful row and preserves the editor', async (outcome) => {
    const before = editorState()
    const { result } = renderHook(() => useBatchRenderWorkflow({ phase: 'confirm', settings }))
    await waitFor(() => expect(result.current.batchReady).toBe(true))
    await act(async () => result.current.runBatch())
    if (outcome === 'cancelled') {
      await act(async () => result.current.cancelBatch())
      expect(backend.cancelBatchRender).toHaveBeenCalledWith('batch-1')
      expect(result.current.batchRunning).toBe(true)
      expect(result.current.batchFinished).toBe(false)
    }
    latest = snapshot(outcome, 5)
    if (outcome === 'cancelled')
      latest.items.forEach((item, i) => {
        item.outcome = { status: i === 0 ? 'cancelled' : 'unstarted' }
      })
    act(() => observe({ kind: 'snapshot', data: latest }))
    expect(result.current.batchFinished).toBe(true)
    expect(result.current.batchRunning).toBe(false)
    expect(editorState()).toEqual(before)
  })
})
