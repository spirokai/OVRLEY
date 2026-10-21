import { act, renderHook, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, test, vi } from 'vitest'
import useVideoImport from '@/features/video-preview/hooks/useVideoImport'
import useStore from '@/store/useStore'

const boundaries = vi.hoisted(() => ({
  extractVideoTelemetry: vi.fn(),
  openMultiplePaths: vi.fn(),
  preparePreviewVideo: vi.fn(),
  registerPreviewVideo: vi.fn(),
  removePreviewVideo: vi.fn(),
}))

vi.mock('@/api/backend', async (importOriginal) => ({
  ...(await importOriginal()),
  extractVideoTelemetry: boundaries.extractVideoTelemetry,
  preparePreviewVideo: boundaries.preparePreviewVideo,
  registerPreviewVideo: boundaries.registerPreviewVideo,
  removePreviewVideo: boundaries.removePreviewVideo,
}))

vi.mock('@/lib/file-dialog', async (importOriginal) => ({
  ...(await importOriginal()),
  openMultiplePaths: boundaries.openMultiplePaths,
}))

const formats = {
  hd30: { resolution: { width: 1920, height: 1080 }, fpsNum: 30, fpsDen: 1 },
  uhd60: { resolution: { width: 3840, height: 2160 }, fpsNum: 60, fpsDen: 1 },
}

// Activity 08:00:00Z–08:02:00Z, Sofia (UTC+3).
const activityFile = {
  coverage: { speed: { source: 'direct' } },
  extended_attributes: [],
  metadata: { duration_seconds: 120, sample_count: 120, end_time: '2026-07-18T08:02:00Z', timezone: 'Europe/Sofia' },
  sync_time: '2026-07-18T08:00:00Z',
  file_format: 'fit',
  valid_attributes: ['speed'],
}

// Probe results keyed by path; a missing entry fails the probe.
let probes = {}

function probe(path, { format = formats.hd30, creationTime = null, timeSource = creationTime ? 'gps' : null, ...overrides } = {}) {
  probes[path] = { path, duration: 30, hasAudio: true, creationTime, timeSource, ...format, ...overrides }
}

function renderImport() {
  const onSetBackgroundMode = vi.fn()
  const hook = renderHook(() => useVideoImport({ onSetBackgroundMode }))
  return { ...hook, onSetBackgroundMode }
}

async function selectFiles(result, paths) {
  boundaries.openMultiplePaths.mockResolvedValueOnce(paths)
  await act(() => result.current.handleImportVideo())
}

function dialog(result) {
  return result.current.importDialog
}

function row(result, filename) {
  return dialog(result).rows.find((candidate) => candidate.filename === filename)
}

async function confirm(result) {
  await act(() => dialog(result).onConfirm())
}

function importedPaths() {
  return useStore.getState().importedVideos.map((video) => video.path)
}

beforeEach(() => {
  vi.clearAllMocks()
  useStore.setState(useStore.getInitialState(), true)
  useStore.temporal.getState().clear()
  probes = {}
  boundaries.preparePreviewVideo.mockImplementation(async (path) => {
    if (!probes[path]) throw new Error(`probe failed: ${path}`)
    return { metadata: probes[path], warnings: [] }
  })
  boundaries.registerPreviewVideo.mockImplementation(async (path) => ({ importId: `import:${path}`, previewUrl: `http://preview/${path}` }))
  boundaries.removePreviewVideo.mockResolvedValue(null)
  boundaries.extractVideoTelemetry.mockResolvedValue({ parsed_activity: null })
})

describe('reviewing an import', () => {
  test('lists the probed files without touching the collection or the preview server until Import', async () => {
    probe('/clips/b.mp4', { format: formats.uhd60, creationTime: '2026-07-18T08:05:00Z' })
    probe('/clips/a.mp4', { format: formats.uhd60, creationTime: '2026-07-18T08:00:00Z' })
    const { result, onSetBackgroundMode } = renderImport()

    await selectFiles(result, ['/clips/b.mp4', '/clips/a.mp4'])

    expect(dialog(result)).toMatchObject({ open: true, phase: 'reviewing', importCount: 2, issues: [] })
    expect(dialog(result).rows.map(({ filename, isNew, verdict, offsetSeconds }) => [filename, isNew, verdict, offsetSeconds])).toEqual([
      ['a.mp4', true, 'import', 0],
      ['b.mp4', true, 'import', 300],
    ])
    expect(importedPaths()).toEqual([])
    expect(boundaries.registerPreviewVideo).not.toHaveBeenCalled()

    await confirm(result)

    const state = useStore.getState()
    expect(dialog(result).open).toBe(false)
    expect(importedPaths()).toEqual(['/clips/b.mp4', '/clips/a.mp4'])
    expect(state.importedVideos.map((video) => video.preview.importId)).toEqual(['import:/clips/b.mp4', 'import:/clips/a.mp4'])
    expect(state.config.scene).toMatchObject({ width: 3840, height: 2160 })
    expect(state.renderSettings.fps).toBe(60)
    expect(onSetBackgroundMode).toHaveBeenCalledWith('video')
    await waitFor(() => expect(useStore.getState().importedVideos.every((video) => video.telemetry.status === 'absent')).toBe(true))
    expect(boundaries.extractVideoTelemetry.mock.calls.map(([path]) => path)).toEqual(['/clips/a.mp4', '/clips/b.mp4'])
    expect(useStore.temporal.getState().pastStates).toEqual([])
  })

  test('lists failed and duplicate files while the other files still import', async () => {
    probe('/clips/existing.mp4')
    probe('/clips/good.mp4')
    probe('/clips/no-fps.mp4', { fpsNum: undefined })
    const { result } = renderImport()
    await selectFiles(result, ['/clips/existing.mp4'])
    await confirm(result)

    await selectFiles(result, [
      '/clips/good.mp4',
      '/clips/existing.mp4',
      '/clips/good.mp4',
      '/clips/notes.txt',
      '/clips/missing.mp4',
      '/clips/no-fps.mp4',
    ])

    expect(dialog(result).failures.map(({ name, message }) => [name, message])).toEqual([
      ['existing.mp4', 'Already imported'],
      ['good.mp4', 'Already imported'],
      ['notes.txt', 'Not a supported video file (MP4, MOV, MKV).'],
      ['missing.mp4', 'probe failed: /clips/missing.mp4'],
      ['no-fps.mp4', 'Imported video metadata contains an invalid frame rate numerator'],
    ])
    expect(row(result, 'existing.mp4')).toMatchObject({ isNew: false, verdict: 'kept' })
    expect(dialog(result).importCount).toBe(1)

    await confirm(result)
    expect(importedPaths()).toEqual(['/clips/existing.mp4', '/clips/good.mp4'])
  })

  test('Cancel commits and registers nothing', async () => {
    probe('/clips/a.mp4')
    const { result } = renderImport()
    await selectFiles(result, ['/clips/a.mp4'])

    act(() => dialog(result).onCancel())

    expect(dialog(result).open).toBe(false)
    expect(importedPaths()).toEqual([])
    expect(boundaries.registerPreviewVideo).not.toHaveBeenCalled()
  })

  test('cancelling while the files are read discards the review', async () => {
    let finishProbe
    boundaries.preparePreviewVideo.mockImplementationOnce(
      (path) =>
        new Promise((resolve) => {
          finishProbe = () => resolve({ metadata: { path, duration: 30, hasAudio: true, ...formats.hd30 }, warnings: [] })
        }),
    )
    const { result } = renderImport()
    boundaries.openMultiplePaths.mockResolvedValueOnce(['/clips/a.mp4'])
    let selection
    act(() => {
      selection = result.current.handleImportVideo()
    })
    await waitFor(() => expect(dialog(result)).toMatchObject({ open: true, phase: 'probing', fileCount: 1 }))

    act(() => dialog(result).onCancel())
    await act(async () => {
      finishProbe()
      await selection
    })

    expect(dialog(result).open).toBe(false)
    expect(importedPaths()).toEqual([])
  })

  test('a failed registration keeps the dialog open with the reason and removes the registrations already made', async () => {
    probe('/clips/a.mp4', { creationTime: '2026-07-18T08:00:00Z' })
    probe('/clips/b.mp4', { creationTime: '2026-07-18T08:05:00Z' })
    boundaries.registerPreviewVideo
      .mockResolvedValueOnce({ importId: 'first-import', previewUrl: 'http://preview/a' })
      .mockRejectedValueOnce(new Error('preview server unavailable'))
    const { result } = renderImport()
    await selectFiles(result, ['/clips/a.mp4', '/clips/b.mp4'])

    await confirm(result)

    expect(dialog(result)).toMatchObject({ open: true, phase: 'reviewing', error: 'preview server unavailable' })
    expect(boundaries.removePreviewVideo).toHaveBeenCalledExactlyOnceWith('first-import')
    expect(importedPaths()).toEqual([])
  })
})

describe('which videos are imported', () => {
  test('with an activity, a video starting more than an hour before it is excluded until its sync brings it closer', async () => {
    useStore.getState().activateActivityFile(activityFile)
    probe('/clips/ride.mp4', { creationTime: '2026-07-18T08:00:30Z' })
    probe('/clips/early.mp4', { creationTime: '2026-07-18T06:45:00Z' })
    const { result } = renderImport()

    await selectFiles(result, ['/clips/ride.mp4', '/clips/early.mp4'])

    expect(dialog(result).windowKind).toBe('activity')
    expect(row(result, 'early.mp4')).toMatchObject({ verdict: 'excluded', offsetSeconds: -4500 })
    expect(dialog(result).importCount).toBe(1)

    act(() => row(result, 'early.mp4').onOffsetCommit(-3000))
    expect(row(result, 'early.mp4')).toMatchObject({ verdict: 'import', offsetSeconds: -3000, isManual: true })

    await confirm(result)
    expect(importedPaths()).toEqual(['/clips/ride.mp4', '/clips/early.mp4'])
    expect(useStore.getState().videoEdits[useStore.getState().importedVideos[1].id]).toMatchObject({ offsetSeconds: -3000, offsetSource: 'manual' })
  })

  test('a fresh import without an activity takes nothing while its videos start more than 12 hours apart', async () => {
    probe('/clips/morning.mp4', { creationTime: '2026-07-18T08:00:00Z' })
    probe('/clips/next-day.mp4', { creationTime: '2026-07-19T09:00:00Z' })
    const { result } = renderImport()

    await selectFiles(result, ['/clips/morning.mp4', '/clips/next-day.mp4'])

    expect(dialog(result).windowKind).toBe('span')
    expect(dialog(result).rows.map((candidate) => candidate.verdict)).toEqual(['excluded', 'excluded'])
    expect(dialog(result).importCount).toBe(0)

    act(() => row(result, 'next-day.mp4').onOffsetCommit(600))
    expect(dialog(result).rows.map((candidate) => candidate.verdict)).toEqual(['import', 'import'])
  })

  test('without an activity, a new video must start within an hour of the videos already imported', async () => {
    probe('/clips/first.mp4', { creationTime: '2026-07-18T08:00:00Z' })
    const { result } = renderImport()
    await selectFiles(result, ['/clips/first.mp4'])
    await confirm(result)

    probe('/clips/close.mp4', { creationTime: '2026-07-18T08:50:00Z' })
    probe('/clips/far.mp4', { creationTime: '2026-07-18T10:00:00Z' })
    await selectFiles(result, ['/clips/close.mp4', '/clips/far.mp4'])

    expect(dialog(result).windowKind).toBe('videos')
    expect(dialog(result).rows.map(({ filename, isNew, verdict }) => [filename, isNew, verdict])).toEqual([
      ['first.mp4', false, 'kept'],
      ['close.mp4', true, 'import'],
      ['far.mp4', true, 'excluded'],
    ])
  })

  test('an existing video outside the activity window stays unless it is removed from the project', async () => {
    probe('/clips/ride.mp4', { creationTime: '2026-07-18T08:00:30Z' })
    probe('/clips/late.mp4', { creationTime: '2026-07-18T08:01:00Z' })
    const { result } = renderImport()
    await selectFiles(result, ['/clips/ride.mp4'])
    await confirm(result)
    useStore.getState().activateActivityFile(activityFile)
    const ride = useStore.getState().importedVideos[0]
    useStore.getState().setVideoOffset(ride.id, 9000)

    await selectFiles(result, ['/clips/late.mp4'])
    expect(row(result, 'ride.mp4')).toMatchObject({ verdict: 'outside', onRemove: expect.any(Function) })

    act(() => row(result, 'ride.mp4').onRemove())
    expect(row(result, 'ride.mp4')).toBeUndefined()
    await confirm(result)

    expect(importedPaths()).toEqual(['/clips/late.mp4'])
    expect(boundaries.removePreviewVideo).toHaveBeenCalledExactlyOnceWith('import:/clips/ride.mp4')
  })
})

describe('editing in the dialog', () => {
  test('a sync typed by hand survives other sync settings and resets to the detected time', async () => {
    useStore.getState().activateActivityFile({
      ...activityFile,
      sync_time: '2026-07-18T07:20:03.000Z',
      metadata: { ...activityFile.metadata, end_time: '2026-07-18T11:20:03.000Z', timezone: 'Europe/Moscow' },
    })
    // Camera clocks: 10:51 fits the activity as local time and as UTC.
    probe('/clips/a.mp4', { creationTime: '2026-07-18T10:51:00.000000Z', timeSource: 'ffprobe' })
    probe('/clips/b.mp4', { creationTime: '2026-07-18T10:55:00.000000Z', timeSource: 'ffprobe' })
    const { result } = renderImport()
    await selectFiles(result, ['/clips/a.mp4', '/clips/b.mp4'])
    expect([row(result, 'a.mp4').offsetSeconds, row(result, 'b.mp4').offsetSeconds]).toEqual([1857, 2097])

    act(() => row(result, 'b.mp4').onOffsetCommit(2500))
    act(() => dialog(result).sync.setVideoSyncTimezoneMode('utc'))
    expect([row(result, 'a.mp4').offsetSeconds, row(result, 'b.mp4').offsetSeconds]).toEqual([12657, 2500])

    act(() => row(result, 'b.mp4').onOffsetReset())
    expect(row(result, 'b.mp4')).toMatchObject({ offsetSeconds: 12897, isManual: false })
  })

  test('videos without a recording time follow the previous video and move with the arrows', async () => {
    probe('/clips/known.mp4', { creationTime: '2026-07-18T08:00:00Z' })
    probe('/clips/first.mp4')
    probe('/clips/second.mp4')
    const { result } = renderImport()
    await selectFiles(result, ['/clips/known.mp4', '/clips/first.mp4', '/clips/second.mp4'])

    expect(dialog(result).rows.map(({ filename, offsetSeconds, isPositioned }) => [filename, offsetSeconds, isPositioned])).toEqual([
      ['known.mp4', 0, false],
      ['first.mp4', 30, true],
      ['second.mp4', 60, true],
    ])
    expect(row(result, 'known.mp4')).toMatchObject({ onMoveUp: null, onMoveDown: null })
    expect(row(result, 'second.mp4').onMoveDown).toBeNull()

    act(() => row(result, 'second.mp4').onMoveUp())
    expect(dialog(result).rows.map((candidate) => candidate.filename)).toEqual(['known.mp4', 'second.mp4', 'first.mp4'])
    act(() => row(result, 'second.mp4').onMoveUp())
    expect(dialog(result).rows.map(({ filename, offsetSeconds }) => [filename, offsetSeconds])).toEqual([
      ['second.mp4', -30],
      ['known.mp4', 0],
      ['first.mp4', 30],
    ])
  })

  test('shows overlaps and what prevents a single-file export, and commits the chosen transition', async () => {
    probe('/clips/a.mp4', { creationTime: '2026-07-18T08:00:00Z' })
    probe('/clips/b.mp4', { creationTime: '2026-07-18T08:00:20Z' })
    probe('/clips/c.mp4', { creationTime: '2026-07-18T08:05:00Z', format: formats.uhd60 })
    const { result } = renderImport()
    await selectFiles(result, ['/clips/a.mp4', '/clips/b.mp4', '/clips/c.mp4'])

    expect(row(result, 'a.mp4').overlaps).toEqual([{ name: 'b.mp4', seconds: 10 }])
    expect(row(result, 'c.mp4')).toMatchObject({ isIncompatible: true, formatLabel: '3840×2160 · 60 fps' })
    expect(dialog(result).issues).toEqual([
      'Videos overlap (a.mp4, b.mp4). Move a video or delete the overlapping part.',
      'Videos differ in resolution or frame rate (c.mp4). Remove them or export separate files.',
    ])
    expect(dialog(result).transition).toMatchObject({ type: 'none', typeLabel: 'No transition' })

    act(() => dialog(result).transition.onTypeChange('fade'))
    act(() => dialog(result).transition.onDurationCommit(1.5))
    await confirm(result)
    expect(useStore.getState().transitionDefaults).toEqual({ type: 'fade', durationSeconds: 1.5, splitGapThresholdSeconds: 2 })
  })
})

describe('the video collection outside the dialog', () => {
  test('removes one clip, releases its preview registration, and clears the background after the last clip', async () => {
    probe('/clips/a.mp4', { creationTime: '2026-07-18T08:00:00Z' })
    probe('/clips/b.mp4', { creationTime: '2026-07-18T08:05:00Z' })
    const { result, onSetBackgroundMode } = renderImport()
    await selectFiles(result, ['/clips/a.mp4', '/clips/b.mp4'])
    await confirm(result)
    const [first, second] = useStore.getState().importedVideos

    act(() => useStore.getState().startPreviewPlayback({ source: 'video', second: 10 }))
    await act(() => result.current.removeVideo(first.id))
    expect(importedPaths()).toEqual(['/clips/b.mp4'])
    expect(useStore.getState().previewPlaybackState).toBe('paused')
    expect(boundaries.removePreviewVideo).toHaveBeenCalledExactlyOnceWith('import:/clips/a.mp4')
    expect(onSetBackgroundMode).not.toHaveBeenCalledWith('checker')

    await act(() => result.current.removeVideo(second.id))
    expect(importedPaths()).toEqual([])
    expect(onSetBackgroundMode).toHaveBeenLastCalledWith('checker')

    await act(() => result.current.removeVideo(second.id))
    expect(useStore.getState().errorMessage).toBe(`Removing the video failed: Unknown imported video: ${second.id}`)
  })

  test('dropped native paths open the review, browser files without a path are rejected', async () => {
    probe('/clips/a.mp4')
    probe('/clips/b.mp4')
    const { result } = renderImport()

    await act(() => result.current.handleVideoFilesDrop(['/clips/a.mp4', '/clips/b.mp4']))
    expect(dialog(result).rows.map((candidate) => candidate.filename)).toEqual(['a.mp4', 'b.mp4'])
    act(() => dialog(result).onCancel())

    await act(() => result.current.handleVideoFilesDrop([new File([], 'c.mp4')]))
    expect(useStore.getState().errorMessage).toBe('Video drop failed: Dropped files must be local files.')
    expect(dialog(result).open).toBe(false)
  })
})
