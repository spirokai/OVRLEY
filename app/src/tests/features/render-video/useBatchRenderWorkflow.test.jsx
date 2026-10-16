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
import { createTestVideo, videoStoreState } from '@/tests/helpers/videoFixtures'
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
  listAvailableFonts: vi.fn().mockResolvedValue({ recommendedFonts: [], systemFonts: [] }),
}))
vi.mock('@/lib/file-dialog', () => ({ openDirectoryPath: vi.fn() }))

const folder = 'C:/videos'
const reference = `${folder}/reference.mp4`
const next = `${folder}/ride.part 2.mp4`
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

// Activity 12:00–13:00. "outside" starts an hour after the activity ends.
function setVideos({ creationTime = '2026-10-05T11:59:48Z', offsets = {} } = {}) {
  const video = (id, path, options) => createTestVideo({ id, path, durationSeconds: 120, timeSource: 'ffprobe', ...options })
  const videos = [
    video('reference', reference, { creationTime, importOrder: 0 }),
    video('next', next, { creationTime: '2026-10-05T12:00:40Z', importOrder: 1 }),
    video('outside', blocked, { creationTime: '2026-10-05T14:00:00Z', importOrder: 2 }),
  ]
  useStore.setState(videoStoreState(videos, { reference: -9, next: 40, outside: 7200, ...offsets }))
}

function setOffset(videoId, offsetSeconds) {
  const { videoEdits } = useStore.getState()
  useStore.setState({ videoEdits: { ...videoEdits, [videoId]: { ...videoEdits[videoId], offsetSeconds } } })
}

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
      creationTime: null,
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
    plannedFrames: acceptedRequest.outputs.length * 3600,
    processedFrames: terminal ? acceptedRequest.outputs.length * 3600 : 0,
    renderedFrames: 0,
    encodedFrames: 0,
    currentItemProgress: null,
    elapsedSeconds: 0,
    estimatedSecondsRemaining: null,
    items: acceptedRequest.outputs.map((output) => ({
      id: output.id,
      phase: terminal ? 'finished' : 'queued',
      plannedFrames: 3600,
      currentFrames: 0,
      renderedFrames: 0,
      encodedFrames: 0,
      outcome: terminal ? { status: 'failed', message: 'Telemetry unavailable' } : null,
    })),
    outputs: [],
    resultCounts: { succeeded: 0, failed: terminal ? acceptedRequest.outputs.length : 0, cancelled: 0, unstarted: 0 },
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
      'importedVideos',
      'videoEdits',
      'videoDragPreview',
      'selectedSecond',
      'timelineViewport',
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
  vi.mocked(backend.planBatchOutputs).mockImplementation(async ({ outputs }) => ({
    status: 'planned',
    plans: outputs.map((output) => ({ outputId: output.id, plannedFrames: 3600 })),
  }))
  vi.mocked(backend.subscribeBatchRenderProgress).mockImplementation(async (handler) => {
    observe = handler
    return unlisten
  })
  vi.mocked(backend.submitBatchRender).mockImplementation(async (request) => {
    acceptedRequest = request
    latest = snapshot()
    observe({ kind: 'snapshot', data: { ...latest, revision: 2, snapshotRevision: 2, phase: 'preparing', activeItemId: request.outputs[0].id } })
    return { batchId: latest.batchId, snapshot: latest }
  })
  vi.mocked(backend.getBatchRenderSnapshot).mockImplementation(async () => latest)
  vi.mocked(backend.cancelBatchRender).mockImplementation(async () => ({ ...latest, revision: 3, phase: 'cancelling' }))
  useStore.setState(useStore.getInitialState(), true)
  useStore.setState({
    config: structuredClone(DEFAULT_CONFIG),
    parsedActivity: { sample_elapsed_seconds: [0, 3600] },
    activitySummary: { syncTime: '2026-10-05T12:00:00Z', endTime: '2026-10-05T13:00:00Z', timezone: 'UTC', durationSeconds: 3600 },
    videoDragPreview: { videoId: 'reference', deltaSeconds: 123 },
    videoSyncTimezoneMode: 'utc',
    selectedSecond: 27,
    timelineViewport: { viewStart: 20, viewEnd: 40 },
    availableCodecs: { libx264: true },
    platformOs: 'windows',
  })
  setVideos()
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

  test('renders every imported video in timeline order at its committed offset, also outside the activity', async () => {
    const { result } = renderHook(() => useBatchRenderWorkflow({ phase: 'confirm', settings }))
    await waitFor(() => expect(result.current.batchReady).toBe(true))
    expect(result.current.batchQueue.map((row) => [row.id, row.status, row.error])).toEqual([
      ['reference', 'pending', null],
      ['next', 'pending', null],
      ['outside', 'pending', null],
    ])
    expect(backend.inspectVideoSource.mock.calls.map(([, path]) => path)).toEqual([reference, next, blocked])
    await act(async () => result.current.runBatch())
    expect(acceptedRequest.outputs.map((output) => output.id)).toEqual([reference, next, blocked])
    expect(acceptedRequest.activity.offsets[`inspection-1:${blocked}`]).toBe(7200)
  })

  test('renders only the part of each video inside the export range and blocks videos outside it', async () => {
    const ranged = { ...settings, range: { type: 'custom', from: 20, to: 100 } }
    const { result } = renderHook(() => useBatchRenderWorkflow({ phase: 'confirm', settings: ranged }))
    await waitFor(() => expect(result.current.batchReady).toBe(true))
    expect(result.current.batchQueue.find((row) => row.id === 'outside')).toMatchObject({ status: 'blocked', error: 'Outside the export range' })
    await act(async () => result.current.runBatch())
    expect(acceptedRequest.outputs).toEqual([
      { id: reference, pieces: [{ sourceId: `inspection-1:${reference}`, startFrame: 870, endFrame: 3270, skipOverlay: false }], transitions: [] },
      { id: next, pieces: [{ sourceId: `inspection-1:${next}`, startFrame: 0, endFrame: 1800, skipOverlay: false }], transitions: [] },
    ])
  })

  test('an unreadable camera timestamp only hides the date of its row', async () => {
    setVideos({ creationTime: 'not-a-time' })
    const { result } = renderHook(() => useBatchRenderWorkflow({ phase: 'confirm', settings }))
    await waitFor(() => expect(result.current.batchReady).toBe(true))
    expect(result.current.batchQueue[0]).toMatchObject({ status: 'pending', creationDateLabel: null, durationLabel: '02:00' })
  })

  test('a video that cannot be inspected blocks only its own row', async () => {
    vi.mocked(backend.inspectVideoSource).mockImplementation(async (id, path) => {
      if (path === next) throw new Error('Video is unavailable')
      return source(id, path)
    })
    const { result } = renderHook(() => useBatchRenderWorkflow({ phase: 'confirm', settings }))
    await waitFor(() => expect(result.current.batchReady).toBe(true))
    expect(result.current.batchQueue.find((row) => row.id === 'next')).toMatchObject({ status: 'blocked', error: 'Video is unavailable' })
    expect(result.current.batchQueue.find((row) => row.id === 'reference').status).toBe('pending')
    await act(async () => result.current.runBatch())
    expect(acceptedRequest.outputs.map((output) => output.id)).toEqual([reference, blocked])
  })

  test('submits embedded-telemetry batches without an external activity', async () => {
    useStore.setState({ parsedActivity: null, activitySummary: null })
    const { result } = renderHook(() => useBatchRenderWorkflow({ phase: 'confirm', settings }))
    await waitFor(() => expect(result.current.batchReady).toBe(true))
    expect(backend.planBatchOutputs).toHaveBeenLastCalledWith(
      {
        inspectionId: 'inspection-1',
        outputs: [reference, next, blocked].map((path) => ({
          id: path,
          pieces: [{ sourceId: `inspection-1:${path}`, startFrame: 0, endFrame: 3600, skipOverlay: false }],
          transitions: [],
        })),
        destination: { kind: 'sourceNamed', directory: 'C:/renders' },
      },
      expect.any(Object),
    )
    await act(async () => result.current.runBatch())
    expect(acceptedRequest.activity).toEqual({ mode: 'embeddedActivity' })
    expect(acceptedRequest.outputs.map((output) => output.id)).toEqual([reference, next, blocked])
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
  test('submits once with committed offsets and overlay choices; keeps the editor and history through mixed results and unmount', async () => {
    const before = editorState()
    const history = useStore.temporal.getState().pastStates
    const { result, unmount } = renderHook(() => ({
      batch: useBatchRenderWorkflow({ phase: 'confirm', settings }),
      render: useRenderVideoDerivedState({ settings: { ...settings, renderTarget: 'current' } }),
      project: useProjectDocumentState(),
    }))
    await waitFor(() => expect(result.current.batch.batchReady).toBe(true))
    expect(result.current.batch.batchQueue.find((row) => row.path === blocked).status).toBe('pending')
    act(() => result.current.batch.setBatchItemSkipOverlay('next', true))
    await waitFor(() => expect(result.current.batch.batchReady).toBe(true))
    await act(async () => result.current.batch.runBatch())
    expect(backend.submitBatchRender).toHaveBeenCalledOnce()
    expect(backend.subscribeBatchRenderProgress.mock.invocationCallOrder[0]).toBeLessThan(backend.submitBatchRender.mock.invocationCallOrder[0])
    expect(acceptedRequest).toMatchObject({
      inspectionId: 'inspection-1',
      destination: { kind: 'sourceNamed', directory: 'C:/renders' },
      encoding: { fps: 24, updateRate: 2 },
      activity: {
        mode: 'externalActivity',
        offsets: { [`inspection-1:${reference}`]: -9, [`inspection-1:${next}`]: 40, [`inspection-1:${blocked}`]: 7200 },
      },
      outputs: [
        { id: reference, pieces: [{ sourceId: `inspection-1:${reference}`, startFrame: 0, endFrame: 3600, skipOverlay: false }] },
        { id: next, pieces: [{ sourceId: `inspection-1:${next}`, startFrame: 0, endFrame: 3600, skipOverlay: true }] },
        { id: blocked, pieces: [{ sourceId: `inspection-1:${blocked}`, startFrame: 0, endFrame: 3600, skipOverlay: false }] },
      ],
    })
    expect(Object.isFrozen(acceptedRequest)).toBe(true)
    // This same wire template is accepted by the Rust batch service test.
    expect(JSON.parse(JSON.stringify(acceptedRequest.template))).toEqual(batchTemplate)
    expect(result.current.batch.batchSnapshot.phase).toBe('preparing') // Older acceptance/read must not roll back the event.
    expect(result.current.batch.batchProgress.total).toBe(10800)
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
    expect(resumed.result.current.batchQueue.map((row) => row.status)).toEqual(['succeeded', 'failed', 'failed'])
    expect(editorState()).toEqual(before)
    expect(useStore.temporal.getState().pastStates).toBe(history)
  })

  test('close/reopen retains in-flight inspections; collection changes dispose sessions and edits invalidate Start', async () => {
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
    let finishPlan
    vi.mocked(backend.planBatchOutputs).mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          finishPlan = resolve
        }),
    )
    act(() => {
      setOffset('reference', -8)
      useStore.setState({ videoSyncTimezoneMode: 'local' })
    })
    expect(result.current.batchReady).toBe(false)
    await act(async () => result.current.runBatch())
    expect(backend.submitBatchRender).not.toHaveBeenCalled()
    await act(async () => finishPlan({ status: 'planned', plans: [] }))
    await waitFor(() => expect(result.current.batchReady).toBe(true))
    const added = createTestVideo({ id: 'added', path: `${folder}/added.mp4`, durationSeconds: 60, importOrder: 3 })
    act(() => useStore.getState().addImportedVideos([added]))
    await waitFor(() => expect(result.current.batchReady).toBe(true))
    expect(backend.disposeVideoInspection).toHaveBeenCalledWith('inspection-1')
    expect(result.current.batchQueue.map((row) => row.id)).toContain('added')
    expect(result.current.batchQueue[0].source.sourceId).toContain('inspection-2:')
  })

  test('identifies stale submission sources and requires explicit fresh inspection', async () => {
    const { result } = renderHook(() => useBatchRenderWorkflow({ phase: 'confirm', settings }))
    await waitFor(() => expect(result.current.batchReady).toBe(true))
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
    act(() => result.current.setBatchItemSkipOverlay('reference', true))
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
