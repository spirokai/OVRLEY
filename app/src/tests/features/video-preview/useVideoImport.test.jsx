import { act, renderHook, waitFor } from '@testing-library/react'
import i18next from 'i18next'
import { beforeEach, describe, expect, test, vi } from 'vitest'
import useVideoImport from '@/features/video-preview/hooks/useVideoImport'
import { buildVideoImportReportSections } from '@/features/video-preview/utils/videoImportReport'
import useStore from '@/store/useStore'

const boundaries = vi.hoisted(() => ({
  extractVideoTelemetry: vi.fn(),
  importPreviewVideo: vi.fn(),
  openMultiplePaths: vi.fn(),
  removePreviewVideo: vi.fn(),
}))

vi.mock('@/api/backend', async (importOriginal) => ({
  ...(await importOriginal()),
  extractVideoTelemetry: boundaries.extractVideoTelemetry,
  importPreviewVideo: boundaries.importPreviewVideo,
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

// Probe results keyed by path; a missing entry fails the probe.
let probes = {}

function probe(path, { format = formats.hd30, creationTime = null, ...overrides } = {}) {
  probes[path] = { path, duration: 30, hasAudio: true, creationTime, timeSource: creationTime ? 'ffprobe' : null, ...format, ...overrides }
}

function renderImport() {
  const onSetBackgroundMode = vi.fn()
  const hook = renderHook(() => useVideoImport({ onSetBackgroundMode }))
  return { ...hook, onSetBackgroundMode }
}

async function importPaths(result, paths) {
  boundaries.openMultiplePaths.mockResolvedValueOnce(paths)
  await act(() => result.current.handleImportVideo())
}

function importedPaths() {
  return useStore.getState().importedVideos.map((video) => video.path)
}

describe('useVideoImport multi-file import', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    useStore.setState(useStore.getInitialState(), true)
    useStore.temporal.getState().clear()
    probes = {}
    boundaries.importPreviewVideo.mockImplementation(async (path) => {
      if (!probes[path]) throw new Error(`probe failed: ${path}`)
      return { importId: `import:${path}`, previewUrl: `http://preview/${path}`, metadata: probes[path], warnings: [] }
    })
    boundaries.removePreviewVideo.mockResolvedValue(null)
    boundaries.extractVideoTelemetry.mockResolvedValue({ parsed_activity: null })
  })

  test('imports every selected file in one change, adopts the first format, and extracts telemetry in timeline order', async () => {
    probe('/clips/b.mp4', { format: formats.uhd60, creationTime: '2026-07-18T08:05:00Z' })
    probe('/clips/a.mp4', { format: formats.uhd60, creationTime: '2026-07-18T08:00:00Z' })
    const { result, onSetBackgroundMode } = renderImport()

    await importPaths(result, ['/clips/b.mp4', '/clips/a.mp4'])

    const state = useStore.getState()
    expect(importedPaths()).toEqual(['/clips/b.mp4', '/clips/a.mp4'])
    expect(state.importedVideos.map((video) => video.importOrder)).toEqual([0, 1])
    expect(Object.values(state.videoEdits).map((edits) => edits.offsetSeconds)).toEqual([300, 0])
    expect(state.config.scene).toMatchObject({ width: 3840, height: 2160 })
    expect(state.renderSettings.fps).toBe(60)
    expect(state.importingVideo).toBe(false)
    expect(onSetBackgroundMode).toHaveBeenCalledWith('video')
    expect(result.current.importReportDialog.open).toBe(false)
    await waitFor(() => expect(useStore.getState().importedVideos.every((video) => video.telemetry.status === 'absent')).toBe(true))
    expect(boundaries.extractVideoTelemetry.mock.calls.map(([path]) => path)).toEqual(['/clips/a.mp4', '/clips/b.mp4'])
    expect(useStore.temporal.getState().pastStates).toEqual([])
  })

  test('reports failed and duplicate files while the other files still import', async () => {
    probe('/clips/existing.mp4')
    probe('/clips/good.mp4')
    probe('/clips/no-fps.mp4', { fpsNum: undefined })
    const { result } = renderImport()
    await importPaths(result, ['/clips/existing.mp4'])

    await importPaths(result, [
      '/clips/good.mp4',
      '/clips/existing.mp4',
      '/clips/good.mp4',
      '/clips/notes.txt',
      '/clips/missing.mp4',
      '/clips/no-fps.mp4',
    ])

    expect(importedPaths()).toEqual(['/clips/existing.mp4', '/clips/good.mp4'])
    expect(boundaries.removePreviewVideo).toHaveBeenCalledExactlyOnceWith('import:/clips/no-fps.mp4')
    const { open, report } = result.current.importReportDialog
    expect(open).toBe(true)
    expect(report.duplicates).toEqual(['/clips/existing.mp4', '/clips/good.mp4'])
    expect(report.failures).toEqual([
      { path: '/clips/notes.txt', message: 'Not a supported video file (MP4, MOV, MKV).' },
      { path: '/clips/missing.mp4', message: 'probe failed: /clips/missing.mp4' },
      { path: '/clips/no-fps.mp4', message: 'Imported video metadata contains an invalid frame rate numerator' },
    ])
    expect(report.incompatibleVideos).toEqual([])

    act(() => result.current.importReportDialog.onClose())
    expect(result.current.importReportDialog.open).toBe(false)
  })

  test('warns when an import adds clips that cannot be combined, and only the first import sizes the scene', async () => {
    probe('/clips/a.mp4')
    probe('/clips/b.mp4')
    probe('/clips/c.mp4', { format: formats.uhd60 })
    probe('/clips/d.mp4')
    const { result } = renderImport()
    await importPaths(result, ['/clips/a.mp4'])
    useStore.getState().setConfig({ ...useStore.getState().config, scene: { ...useStore.getState().config.scene, width: 1280, height: 720 } })

    await importPaths(result, ['/clips/b.mp4', '/clips/c.mp4'])

    const { report } = result.current.importReportDialog
    expect(report.reference).toEqual({ resolution: { width: 1920, height: 1080 }, fps: { num: 30, den: 1 } })
    expect(report.incompatibleVideos.map((video) => video.path)).toEqual(['/clips/c.mp4'])
    expect(buildVideoImportReportSections(report, i18next.t)).toEqual([
      {
        id: 'incompatible',
        title: 'Different resolution or frame rate',
        description:
          'These videos differ from the other videos (1920×1080 · 30 fps). They can be exported as separate files only; remove them to export one combined file.',
        items: [{ key: report.incompatibleVideos[0].id, name: 'c.mp4', detail: '3840×2160 · 60 fps' }],
      },
    ])
    expect(useStore.getState().config.scene).toMatchObject({ width: 1280, height: 720 })

    act(() => result.current.importReportDialog.onClose())
    await importPaths(result, ['/clips/d.mp4'])
    expect(result.current.importReportDialog.open).toBe(false)
  })

  test('commits nothing when every file fails', async () => {
    const { result, onSetBackgroundMode } = renderImport()

    await importPaths(result, ['/clips/missing.mp4'])

    expect(importedPaths()).toEqual([])
    expect(onSetBackgroundMode).not.toHaveBeenCalled()
    expect(result.current.importReportDialog.report.failures).toHaveLength(1)
  })

  test('imports several dropped native paths and rejects browser files without a path', async () => {
    probe('/clips/a.mp4')
    probe('/clips/b.mp4')
    const { result } = renderImport()

    await act(() => result.current.handleVideoFilesDrop(['/clips/a.mp4', '/clips/b.mp4']))
    expect(importedPaths()).toEqual(['/clips/a.mp4', '/clips/b.mp4'])

    await act(() => result.current.handleVideoFilesDrop([new File([], 'c.mp4')]))
    expect(useStore.getState().errorMessage).toBe('Video drop failed: Dropped files must be local files.')
    expect(importedPaths()).toEqual(['/clips/a.mp4', '/clips/b.mp4'])
  })
})
