import { act, renderHook, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, test, vi } from 'vite-plus/test'
import useVideoImport from '@/features/video-preview/hooks/useVideoImport'
import { createMissingVideo, createVideoEdits, createVideoMetadataSnapshot } from '@/lib/video-program/imported-video'
import useStore from '@/store/useStore'
import { createTestVideo, cutEdits } from '@/tests/helpers/videoFixtures'

const boundaries = vi.hoisted(() => ({
  extractVideoTelemetry: vi.fn(),
  getVideoThumbnail: vi.fn(),
  openMultiplePaths: vi.fn(),
  openSinglePath: vi.fn(),
  preparePreviewVideo: vi.fn(),
  registerPreviewVideo: vi.fn(),
  removePreviewVideo: vi.fn(),
}))

vi.mock('@/api/backend', async (importOriginal) => ({
  ...(await importOriginal()),
  extractVideoTelemetry: boundaries.extractVideoTelemetry,
  getVideoThumbnail: boundaries.getVideoThumbnail,
  preparePreviewVideo: boundaries.preparePreviewVideo,
  registerPreviewVideo: boundaries.registerPreviewVideo,
  removePreviewVideo: boundaries.removePreviewVideo,
}))

vi.mock('@/lib/file-dialog', async (importOriginal) => ({
  ...(await importOriginal()),
  openMultiplePaths: boundaries.openMultiplePaths,
  openSinglePath: boundaries.openSinglePath,
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
  boundaries.getVideoThumbnail.mockImplementation(async (path) => `thumbnail:${path}`)
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

  test('imports a single video into a project without videos without the dialog', async () => {
    probe('/clips/only.mp4')
    const { result } = renderImport()

    await selectFiles(result, ['/clips/only.mp4'])

    expect(dialog(result).open).toBe(false)
    expect(importedPaths()).toEqual(['/clips/only.mp4'])
  })

  test('lists failed and duplicate files while the other files still import', async () => {
    probe('/clips/existing.mp4', { creationTime: '2026-07-18T08:00:00Z' })
    probe('/clips/good.mp4', { creationTime: '2026-07-18T08:05:00Z' })
    probe('/clips/no-fps.mp4', { fpsNum: undefined })
    const { result } = renderImport()
    await selectFiles(result, ['/clips/existing.mp4'])

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
})

describe('editing in the dialog', () => {
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

  test('locating a missing video relinks it to the chosen file, keeping its edits and dropping cuts past the end of a shorter file', async () => {
    const saved = createVideoMetadataSnapshot(createTestVideo({ id: 'clip', path: '/old/ride.mp4', durationSeconds: 60 }))
    useStore.setState({
      importedVideos: [createMissingVideo(saved, { id: 'clip', path: '/old/ride.mp4', importOrder: 0 })],
      videoEdits: {
        clip: {
          ...createVideoEdits(),
          offsetSeconds: 12,
          ...cutEdits([
            { id: 'early', frame: 300 },
            { id: 'late', frame: 1200, excluded: true },
          ]),
        },
      },
    })
    // 30 seconds at 30 fps: 900 frames, so the cut at frame 1200 no longer fits.
    probe('/new/ride.mp4')
    boundaries.openSinglePath.mockResolvedValueOnce('/new/ride.mp4')
    const { result } = renderImport()

    await act(() => result.current.locateVideo('clip'))

    expect(useStore.getState().importedVideos).toEqual([
      expect.objectContaining({
        id: 'clip',
        path: '/new/ride.mp4',
        missing: false,
        thumbnailUrl: 'thumbnail:/new/ride.mp4',
        preview: { importId: 'import:/new/ride.mp4', url: 'http://preview//new/ride.mp4' },
      }),
    ])
    const edits = useStore.getState().videoEdits.clip
    expect(edits).toMatchObject({ offsetSeconds: 12, cuts: [{ id: 'early', frame: 300 }] })
    expect(Object.keys(edits.segments)).toEqual(['start', 'early'])
  })
})
