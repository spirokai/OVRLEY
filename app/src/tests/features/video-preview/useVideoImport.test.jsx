import { act, renderHook, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, test, vi } from 'vite-plus/test'
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
  hd60: { resolution: { width: 1920, height: 1080 }, fpsNum: 60, fpsDen: 1 },
  uhd60: { resolution: { width: 3840, height: 2160 }, fpsNum: 60, fpsDen: 1 },
}

// Activity 08:00:00Z–08:02:00Z, Sofia (UTC+3).
const activityFile = {
  coverage: { speed: { source: 'direct' } },
  extended_attributes: [],
  metadata: { duration_seconds: 120, sample_count: 120, end_time: '2026-07-18T08:02:00Z', timezone: 'Europe/Sofia' },
  sync_time: '2026-07-18T08:00:00Z',
  file_format: 'fit',
  lap_start_elapsed_seconds: [],
  lap_durations_seconds: [],
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
    expect(dialog(result).rows.map(({ filename, isNew, isIncluded, offsetSeconds }) => [filename, isNew, isIncluded, offsetSeconds])).toEqual([
      ['a.mp4', true, true, 0],
      ['b.mp4', true, true, 300],
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
    probe('/clips/existing.mp4', { creationTime: '2026-07-18T08:00:00Z' })
    probe('/clips/good.mp4', { creationTime: '2026-07-18T08:05:00Z' })
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
    expect(row(result, 'existing.mp4')).toMatchObject({ isNew: false, isIncluded: true })
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
  test('with an activity, a video starting more than an hour before it is switched off and asks before it is imported', async () => {
    useStore.getState().activateActivityFile(activityFile)
    probe('/clips/ride.mp4', { creationTime: '2026-07-18T08:00:30Z' })
    probe('/clips/early.mp4', { creationTime: '2026-07-18T06:45:00Z' })
    const { result } = renderImport()

    await selectFiles(result, ['/clips/ride.mp4', '/clips/early.mp4'])

    expect(dialog(result).windowKind).toBe('activity')
    expect(row(result, 'early.mp4')).toMatchObject({ isIncluded: false, isFar: true, offsetSeconds: -4500 })
    expect(dialog(result).importCount).toBe(1)

    // A sync edit never switches a video on or off; only the warning follows it.
    act(() => row(result, 'early.mp4').onIncludedChange(true))
    act(() => row(result, 'early.mp4').onOffsetCommit(-3000))
    expect(row(result, 'early.mp4')).toMatchObject({ isIncluded: true, isFar: false, offsetSeconds: -3000, isManual: true })
    act(() => row(result, 'early.mp4').onOffsetCommit(-4000))
    expect(row(result, 'early.mp4')).toMatchObject({ isIncluded: true, isFar: true })

    await confirm(result)
    expect(dialog(result)).toMatchObject({
      phase: 'confirming',
      farConfirmation: { names: ['early.mp4'], from: '-01:06:40', to: '02:00' },
    })
    expect(importedPaths()).toEqual([])
    act(() => dialog(result).onBack())
    expect(dialog(result)).toMatchObject({ phase: 'reviewing', farConfirmation: null })

    await confirm(result)
    await confirm(result)
    expect(importedPaths()).toEqual(['/clips/ride.mp4', '/clips/early.mp4'])
    expect(useStore.getState().videoEdits[useStore.getState().importedVideos[1].id]).toMatchObject({ offsetSeconds: -4000, offsetSource: 'manual' })
  })

  test('a fresh import without an activity switches on the largest group of videos starting within 12 hours', async () => {
    probe('/clips/morning.mp4', { creationTime: '2026-07-18T08:00:00Z' })
    probe('/clips/noon.mp4', { creationTime: '2026-07-18T12:00:00Z' })
    probe('/clips/next-day.mp4', { creationTime: '2026-07-19T09:00:00Z' })
    const { result } = renderImport()

    await selectFiles(result, ['/clips/morning.mp4', '/clips/noon.mp4', '/clips/next-day.mp4'])

    expect(dialog(result).windowKind).toBe('span')
    expect(dialog(result).rows.map(({ filename, isIncluded, isFar }) => [filename, isIncluded, isFar])).toEqual([
      ['morning.mp4', true, false],
      ['noon.mp4', true, false],
      ['next-day.mp4', false, true],
    ])
    expect(dialog(result).importCount).toBe(2)
  })

  test('a first import against an activity reads camera clocks the way that puts most videos near it', async () => {
    useStore.getState().activateActivityFile(activityFile)
    // As UTC: a minute before the activity and inside it. As Sofia time: both three hours before it.
    // Inferred per video, the first (overlapping the activity in neither reading) would be read as local time.
    probe('/clips/before.mp4', { creationTime: '2026-07-18T07:59:00.000000Z', timeSource: 'ffprobe' })
    probe('/clips/inside.mp4', { creationTime: '2026-07-18T08:01:00.000000Z', timeSource: 'ffprobe' })
    const { result } = renderImport()
    await selectFiles(result, ['/clips/before.mp4', '/clips/inside.mp4'])

    expect(dialog(result).sync.videoSyncTimezoneMode).toBe('utc')
    expect(dialog(result).rows.map(({ filename, isIncluded, offsetSeconds }) => [filename, isIncluded, offsetSeconds])).toEqual([
      ['before.mp4', true, -60],
      ['inside.mp4', true, 60],
    ])
    await confirm(result)

    // Adding videos keeps the reading of the collection.
    act(() => useStore.getState().setVideoSyncTimezoneMode('local'))
    probe('/clips/more.mp4', { creationTime: '2026-07-18T08:01:30.000000Z', timeSource: 'ffprobe' })
    await selectFiles(result, ['/clips/more.mp4'])
    expect(dialog(result).sync.videoSyncTimezoneMode).toBe('local')
  })

  test('a new video with another resolution than the videos already imported cannot be switched on', async () => {
    probe('/clips/first.mp4', { creationTime: '2026-07-18T08:00:00Z' })
    const { result } = renderImport()
    await selectFiles(result, ['/clips/first.mp4'])
    await confirm(result)

    probe('/clips/uhd.mp4', { creationTime: '2026-07-18T08:01:00Z', format: formats.uhd60 })
    probe('/clips/hd.mp4', { creationTime: '2026-07-18T08:02:00Z' })
    await selectFiles(result, ['/clips/uhd.mp4', '/clips/hd.mp4'])

    expect(dialog(result).resolutionLabel).toBe('1920×1080')
    expect(row(result, 'uhd.mp4')).toMatchObject({ hasOtherResolution: true, isIncluded: false, onIncludedChange: null })
    expect(row(result, 'hd.mp4')).toMatchObject({ hasOtherResolution: false, isIncluded: true })
    // Editing other videos never switches it on.
    act(() => dialog(result).sync.commitSharedOffset(5))
    expect(row(result, 'uhd.mp4').isIncluded).toBe(false)
    await confirm(result)
    expect(importedPaths()).toEqual(['/clips/first.mp4', '/clips/hd.mp4'])
  })

  test('a fresh import takes the resolution of most new videos', async () => {
    probe('/clips/a.mp4', { creationTime: '2026-07-18T08:00:00Z' })
    probe('/clips/b.mp4', { creationTime: '2026-07-18T08:01:00Z', format: formats.uhd60 })
    probe('/clips/c.mp4', { creationTime: '2026-07-18T08:02:00Z', format: formats.uhd60 })
    const { result } = renderImport()
    await selectFiles(result, ['/clips/a.mp4', '/clips/b.mp4', '/clips/c.mp4'])

    expect(dialog(result).resolutionLabel).toBe('3840×2160')
    expect(dialog(result).rows.map(({ filename, isIncluded, hasOtherResolution }) => [filename, isIncluded, hasOtherResolution])).toEqual([
      ['a.mp4', false, true],
      ['b.mp4', true, false],
      ['c.mp4', true, false],
    ])
    await confirm(result)
    expect(useStore.getState().config.scene).toMatchObject({ width: 3840, height: 2160 })
  })

  test('switches follow the defaults when times are detected again, unless the user set them', async () => {
    useStore.getState().activateActivityFile(activityFile)
    // Camera clocks: as UTC both start inside the activity, as Sofia time three hours before it.
    probe('/clips/a.mp4', { creationTime: '2026-07-18T08:00:30.000000Z', timeSource: 'ffprobe' })
    probe('/clips/b.mp4', { creationTime: '2026-07-18T08:01:00.000000Z', timeSource: 'ffprobe' })
    const { result } = renderImport()
    await selectFiles(result, ['/clips/a.mp4', '/clips/b.mp4'])
    const switches = () => dialog(result).rows.map(({ filename, isIncluded, isFar }) => [filename, isIncluded, isFar])
    expect(switches()).toEqual([
      ['a.mp4', true, false],
      ['b.mp4', true, false],
    ])

    act(() => row(result, 'b.mp4').onIncludedChange(false))
    act(() => dialog(result).sync.setVideoSyncTimezoneMode('local'))
    expect(switches()).toEqual([
      ['a.mp4', false, true],
      ['b.mp4', false, true],
    ])
    act(() => dialog(result).sync.setVideoSyncTimezoneMode('utc'))
    expect(switches()).toEqual([
      ['a.mp4', true, false],
      ['b.mp4', false, false],
    ])
  })

  test('a switched-off video is not imported, cannot be moved, and keeps its sync until it is switched on again', async () => {
    useStore.getState().activateActivityFile(activityFile)
    probe('/clips/ride.mp4', { creationTime: '2026-07-18T08:00:30Z' })
    probe('/clips/extra.mp4', { creationTime: '2026-07-18T08:01:00Z' })
    probe('/clips/more.mp4', { creationTime: '2026-07-18T08:01:30Z' })
    const { result } = renderImport()
    await selectFiles(result, ['/clips/ride.mp4', '/clips/extra.mp4', '/clips/more.mp4'])

    act(() => row(result, 'extra.mp4').onOffsetCommit(10))
    act(() => row(result, 'extra.mp4').onIncludedChange(false))
    expect(row(result, 'extra.mp4')).toMatchObject({ isIncluded: false, overlaps: [] })
    expect(dialog(result).importCount).toBe(2)
    act(() => row(result, 'extra.mp4').onIncludedChange(true))
    expect(row(result, 'extra.mp4')).toMatchObject({ isIncluded: true, offsetSeconds: 10 })
    act(() => row(result, 'extra.mp4').onIncludedChange(false))
    await confirm(result)

    expect(importedPaths()).toEqual(['/clips/ride.mp4', '/clips/more.mp4'])
    expect(boundaries.registerPreviewVideo.mock.calls.map(([path]) => path)).toEqual(['/clips/ride.mp4', '/clips/more.mp4'])
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
    expect(dialog(result).rows.map(({ filename, isNew, isIncluded, isFar }) => [filename, isNew, isIncluded, isFar])).toEqual([
      ['first.mp4', false, true, false],
      ['close.mp4', true, true, false],
      ['far.mp4', true, false, true],
    ])
  })

  test('an existing video outside the activity window stays switched on and is removed from the project when switched off', async () => {
    probe('/clips/ride.mp4', { creationTime: '2026-07-18T08:00:30Z' })
    probe('/clips/late.mp4', { creationTime: '2026-07-18T08:01:00Z' })
    const { result } = renderImport()
    await selectFiles(result, ['/clips/ride.mp4'])
    await confirm(result)
    useStore.getState().activateActivityFile(activityFile)
    const ride = useStore.getState().importedVideos[0]
    useStore.getState().setVideoOffset(ride.id, 9000)

    await selectFiles(result, ['/clips/late.mp4'])
    expect(row(result, 'ride.mp4')).toMatchObject({ isIncluded: true, isFar: true })

    act(() => row(result, 'ride.mp4').onIncludedChange(false))
    expect(row(result, 'ride.mp4')).toMatchObject({ isIncluded: false })
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

  test('videos without a recording time start switched off after the last switched-on video, which they follow', async () => {
    useStore.getState().activateActivityFile(activityFile)
    probe('/clips/ride.mp4', { creationTime: '2026-07-18T08:00:30Z' })
    probe('/clips/late.mp4', { creationTime: '2026-07-18T10:00:00Z' })
    probe('/clips/first.mp4')
    probe('/clips/second.mp4')
    const { result } = renderImport()
    await selectFiles(result, ['/clips/ride.mp4', '/clips/late.mp4', '/clips/first.mp4', '/clips/second.mp4'])

    const placement = () => dialog(result).rows.map(({ filename, isIncluded, offsetSeconds }) => [filename, isIncluded, offsetSeconds])
    expect(placement()).toEqual([
      ['ride.mp4', true, 30],
      ['first.mp4', false, 60],
      ['second.mp4', false, 90],
      ['late.mp4', false, 7200],
    ])
    expect(row(result, 'first.mp4')).toMatchObject({ isPositioned: true, onMoveUp: null, onMoveDown: null })

    // A switched-off video leaves no gap: the second follows the ride while the first waits after it.
    act(() => row(result, 'second.mp4').onIncludedChange(true))
    const expected = [
      ['ride.mp4', true, 30],
      ['second.mp4', true, 60],
      ['first.mp4', false, 90],
    ]
    expect(placement().slice(0, 3)).toEqual(expected)
    // Switched on, a video stays where it waited.
    act(() => row(result, 'first.mp4').onIncludedChange(true))
    expect(placement().slice(0, 3)).toEqual([expected[0], expected[1], ['first.mp4', true, 90]])
    await confirm(result)
    const { importedVideos, videoEdits } = useStore.getState()
    expect(importedVideos.map((video) => [video.path, videoEdits[video.id].offsetSeconds])).toEqual([
      ['/clips/ride.mp4', 30],
      ['/clips/first.mp4', 90],
      ['/clips/second.mp4', 60],
    ])
  })

  test('a video without a recording time switched on where it starts far from the activity stays on', async () => {
    probe('/clips/ride.mp4', { creationTime: '2026-07-18T08:00:30Z' })
    probe('/clips/unknown.mp4')
    const { result } = renderImport()
    await selectFiles(result, ['/clips/ride.mp4'])
    await confirm(result)
    useStore.getState().activateActivityFile(activityFile)
    useStore.getState().setVideoOffset(useStore.getState().importedVideos[0].id, 9000)

    await selectFiles(result, ['/clips/unknown.mp4'])
    expect(row(result, 'unknown.mp4')).toMatchObject({ isWaiting: true, isIncluded: false, isFar: false, offsetSeconds: 9030 })
    act(() => row(result, 'unknown.mp4').onIncludedChange(true))
    expect(row(result, 'unknown.mp4')).toMatchObject({ isWaiting: false, isIncluded: true, isFar: true, offsetSeconds: 9030 })
  })

  test('a video placed by hand follows the video it was placed after when the timezone changes, and comes back', async () => {
    useStore.getState().activateActivityFile(activityFile)
    // As UTC both camera clocks are inside the activity; as Sofia time three hours before it.
    probe('/clips/a.mp4', { creationTime: '2026-07-18T08:00:00.000000Z', timeSource: 'ffprobe' })
    probe('/clips/b.mp4', { creationTime: '2026-07-18T08:01:00.000000Z', timeSource: 'ffprobe' })
    probe('/clips/unknown.mp4')
    const { result } = renderImport()
    await selectFiles(result, ['/clips/a.mp4', '/clips/b.mp4', '/clips/unknown.mp4'])
    act(() => row(result, 'unknown.mp4').onIncludedChange(true))
    act(() => row(result, 'unknown.mp4').onMoveUp())
    const placement = () => dialog(result).rows.map(({ filename, isIncluded, offsetSeconds }) => [filename, isIncluded, offsetSeconds])
    const placedByHand = [
      ['a.mp4', true, 0],
      ['unknown.mp4', true, 30],
      ['b.mp4', true, 60],
    ]
    expect(placement()).toEqual(placedByHand)

    // Once switched on, it follows the defaults like the others: off and far with the video it follows.
    act(() => dialog(result).sync.setVideoSyncTimezoneMode('local'))
    expect(row(result, 'unknown.mp4')).toMatchObject({ isIncluded: false, isFar: true, offsetSeconds: 30 - 3 * 3600 })
    act(() => dialog(result).sync.setVideoSyncTimezoneMode('utc'))
    expect(placement()).toEqual(placedByHand)

    // Switched by hand, it keeps the user's choice.
    act(() => row(result, 'unknown.mp4').onIncludedChange(false))
    act(() => dialog(result).sync.setVideoSyncTimezoneMode('local'))
    act(() => dialog(result).sync.setVideoSyncTimezoneMode('utc'))
    expect(row(result, 'unknown.mp4')).toMatchObject({ isIncluded: false, isWaiting: false, offsetSeconds: 30 })
  })

  test('the arrows move a video without a recording time among the switched-on videos', async () => {
    probe('/clips/known.mp4', { creationTime: '2026-07-18T08:00:00Z' })
    probe('/clips/first.mp4')
    probe('/clips/second.mp4')
    const { result } = renderImport()
    await selectFiles(result, ['/clips/known.mp4', '/clips/first.mp4', '/clips/second.mp4'])
    act(() => row(result, 'first.mp4').onIncludedChange(true))
    act(() => row(result, 'second.mp4').onIncludedChange(true))

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
    probe('/clips/c.mp4', { creationTime: '2026-07-18T08:05:00Z', format: formats.hd60 })
    const { result } = renderImport()
    await selectFiles(result, ['/clips/a.mp4', '/clips/b.mp4', '/clips/c.mp4'])

    expect(row(result, 'a.mp4').overlaps).toEqual([{ name: 'b.mp4', seconds: 10 }])
    expect(row(result, 'c.mp4')).toMatchObject({ isIncluded: true, isIncompatible: true, formatLabel: '1920×1080 · 60 fps · 00:30' })
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
