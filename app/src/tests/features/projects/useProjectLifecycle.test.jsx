vi.mock('@/lib/font-resources', () => ({ prepareDocumentFonts: vi.fn(async () => {}) }))
import { act, renderHook, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, test, vi } from 'vitest'
import { createDurableTemplateState } from '@/lib/template/template-state'
import { createDurableEditorState } from '@/lib/widget/editor-state'
import { loadProject } from '@/features/projects/projectOperations'
import useProjectLifecycle from '@/features/projects/hooks/useProjectLifecycle'
import { prepareDocumentFonts } from '@/lib/font-resources'
import { DEFAULT_RENDER_SETTINGS } from '@/store/slices/createRenderSettingsSlice'
import { DEFAULT_TRANSITION_SETTINGS } from '@/lib/video-program/transition-catalog'
import useStore from '@/store/useStore'
import { createTestVideo, videoStoreState } from '@/tests/helpers/videoFixtures'

const boundaries = vi.hoisted(() => ({
  clearPreviewVideos: vi.fn(),
  getDefaultProjectDirectory: vi.fn(),
  openSinglePath: vi.fn(),
  readProjectFile: vi.fn(),
  registerPreviewVideo: vi.fn(),
  removePreviewVideo: vi.fn(),
  saveSinglePath: vi.fn(),
  selectedPathIsFile: vi.fn(),
  writeProjectFile: vi.fn(),
}))

vi.mock('@/api/backend', async (importOriginal) => ({
  ...(await importOriginal()),
  clearPreviewVideos: boundaries.clearPreviewVideos,
  getDefaultProjectDirectory: boundaries.getDefaultProjectDirectory,
  readProjectFile: boundaries.readProjectFile,
  registerPreviewVideo: boundaries.registerPreviewVideo,
  removePreviewVideo: boundaries.removePreviewVideo,
  selectedPathIsFile: boundaries.selectedPathIsFile,
  writeProjectFile: boundaries.writeProjectFile,
}))

vi.mock('@/lib/file-dialog', async (importOriginal) => ({
  ...(await importOriginal()),
  openSinglePath: boundaries.openSinglePath,
  saveSinglePath: boundaries.saveSinglePath,
}))

function preparedVideo(path, durationSeconds = 40) {
  return {
    path,
    metadata: { path, duration: durationSeconds, fpsNum: 30, fpsDen: 1, resolution: { width: 1920, height: 1080 }, hasAudio: true },
    previewWarnings: [],
    telemetry: { status: 'absent', activity: null },
  }
}

function projectVideo(id, path, offsetSeconds = 0) {
  return { id, path: { kind: 'project-relative', value: path }, offsetSeconds, deletedParts: [], transitionIn: null, placementAnchor: null }
}

describe('useProjectLifecycle canonical load orchestration', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    useStore.setState(useStore.getInitialState(), true)
    useStore.temporal.getState().clear()
    boundaries.clearPreviewVideos.mockResolvedValue(null)
    boundaries.removePreviewVideo.mockResolvedValue(null)
    boundaries.registerPreviewVideo.mockResolvedValue({ importId: 'owner-import-id', previewUrl: 'http://preview/video' })
  })

  test('normalizes legacy widget alignment at the frontend project-load seam', async () => {
    const initialState = useStore.getState()
    const editor = createDurableEditorState({
      config: {
        ...initialState.config,
        values: [{ id: 'legacy-speed', value: 'speed', x: 300, y: 100 }],
      },
      globalDefaults: initialState.globalDefaults,
    })
    delete editor.config.values[0].content_alignment
    const project = {
      format: 'ovrley-project',
      version: 4,
      savedAt: '2026-08-27T12:00:00.000Z',
      editor,
      sources: { activity: null, videos: [] },
      sync: {
        videoTimezoneMode: null,
        manual: { videoId: null, landmarks: [], detectedLocationSecond: null, speedThresholdKmh: 5, turnThresholdDegrees: 90 },
      },
      transitionDefaults: { ...DEFAULT_TRANSITION_SETTINGS },
      render: { ...DEFAULT_RENDER_SETTINGS, batchVideoFolder: null, batchOutputFolder: null, range: { ...DEFAULT_RENDER_SETTINGS.range } },
      timeline: { playheadSecond: 0, viewStart: 0, viewEnd: 73 },
    }
    boundaries.readProjectFile.mockResolvedValue({
      project,
      resolvedSources: { activityPath: null, videoPaths: [] },
      rasterLoadResults: {},
    })

    const loaded = await loadProject({
      path: 'C:\\Events\\Legacy.oly',
      resolveProjectSources: vi.fn().mockResolvedValue({ activityPath: null, videoPaths: [] }),
      prepareActivityPath: vi.fn(),
      prepareVideoPath: vi.fn(),
    })

    expect(loaded.editor.config.values[0].content_alignment).toBe('left')
    expect(useStore.getState().config.values[0].content_alignment).toBe('left')
  })

  test('replaces the previous preview registration only after the opened project video is applied', async () => {
    const initialState = useStore.getState()
    const videoPath = 'C:\\Events\\ride.mp4'
    const project = {
      format: 'ovrley-project',
      version: 4,
      savedAt: '2026-08-27T12:00:00.000Z',
      editor: createDurableEditorState({ config: initialState.config, globalDefaults: initialState.globalDefaults }),
      sources: { activity: null, videos: [projectVideo('ride-video', 'ride.mp4')] },
      sync: {
        videoTimezoneMode: null,
        manual: { videoId: 'ride-video', landmarks: [], detectedLocationSecond: null, speedThresholdKmh: 5, turnThresholdDegrees: 90 },
      },
      transitionDefaults: { ...DEFAULT_TRANSITION_SETTINGS },
      render: { ...DEFAULT_RENDER_SETTINGS, batchVideoFolder: null, batchOutputFolder: null, range: { ...DEFAULT_RENDER_SETTINGS.range } },
      timeline: { playheadSecond: 0, viewStart: 0, viewEnd: 40 },
    }
    useStore.setState(
      videoStoreState([
        createTestVideo({ id: 'previous-1', path: 'C:\\Previous\\a.mp4' }),
        createTestVideo({ id: 'previous-2', path: 'C:\\Previous\\b.mp4', importOrder: 1 }),
      ]),
    )
    boundaries.readProjectFile.mockResolvedValue({ project, resolvedSources: { activityPath: null, videoPaths: [videoPath] }, rasterLoadResults: {} })
    boundaries.removePreviewVideo.mockImplementation(async () => {
      expect(useStore.getState().importedVideos.map((video) => video.preview.importId)).toEqual(['owner-import-id'])
      return null
    })

    await loadProject({
      path: 'C:\\Events\\Ride.oly',
      resolveProjectSources: vi.fn().mockResolvedValue({ activityPath: null, videoPaths: [videoPath] }),
      prepareActivityPath: vi.fn(),
      prepareVideoPath: vi.fn().mockResolvedValue(preparedVideo(videoPath)),
    })

    expect(boundaries.registerPreviewVideo).toHaveBeenCalledWith(videoPath)
    expect(boundaries.removePreviewVideo).toHaveBeenCalledTimes(2)
    expect(boundaries.removePreviewVideo).toHaveBeenNthCalledWith(1, 'import-previous-1')
    expect(boundaries.removePreviewVideo).toHaveBeenNthCalledWith(2, 'import-previous-2')
    expect(boundaries.clearPreviewVideos).not.toHaveBeenCalled()
  })

  test('delegates source population to each owner before restoring project settings', async () => {
    const projectPath = 'C:\\Events\\Race.oly'
    const activityPath = 'C:\\Events\\ride.fit'
    const videoPath = 'C:\\Events\\ride.mp4'
    const initialState = useStore.getState()
    const project = {
      format: 'ovrley-project',
      version: 4,
      savedAt: '2026-08-27T12:00:00.000Z',
      editor: createDurableEditorState({
        config: { ...initialState.config, scene: { ...initialState.config.scene, width: 1280 } },
        globalDefaults: { ...initialState.globalDefaults, color_text: '#123456' },
      }),
      sources: {
        activity: { path: { kind: 'project-relative', value: 'ride.fit' } },
        videos: [projectVideo('ride-video', 'ride.mp4', 12)],
      },
      sync: {
        videoTimezoneMode: 'utc',
        manual: { videoId: 'ride-video', landmarks: [], detectedLocationSecond: null, speedThresholdKmh: 5, turnThresholdDegrees: 90 },
      },
      transitionDefaults: { ...DEFAULT_TRANSITION_SETTINGS },
      render: {
        renderTarget: 'current',
        batchVideoFolder: null,
        batchOutputFolder: null,
        fps: 60,
        widgetUpdateRate: 2,
        exportMode: 'composite',
        codec: 'libx264',
        qualityType: 'bitrate',
        qualityValue: 20,
        range: { type: 'custom', from: 10, to: 80 },
      },
      timeline: { playheadSecond: 30, viewStart: 20, viewEnd: 60 },
    }
    project.editor.config.scene.fps = project.render.fps
    project.editor.config.scene.updateRate = project.render.widgetUpdateRate

    boundaries.openSinglePath.mockResolvedValue(projectPath)
    boundaries.getDefaultProjectDirectory.mockResolvedValue('C:\\Users\\test\\Documents\\OVRLEY\\projects')
    boundaries.readProjectFile.mockResolvedValue({
      project,
      resolvedSources: { activityPath, videoPaths: [videoPath] },
      rasterLoadResults: {},
    })
    boundaries.selectedPathIsFile.mockResolvedValue(true)
    let finishActivityPreparation
    let finishVideoPreparation
    const preparedActivity = {
      source: { kind: 'file', path: activityPath },
      parsedActivity: {
        valid_attributes: [],
        extended_attributes: [],
        coverage: {},
        metadata: { duration_seconds: 100 },
        file_format: 'fit',
        file_name: 'ride.fit',
      },
    }
    const prepareActivityPath = vi.fn(
      (path) =>
        new Promise((resolve) => {
          finishActivityPreparation = () => resolve({ ...preparedActivity, source: { kind: 'file', path } })
        }),
    )
    const prepareVideoPath = vi.fn(
      (path) =>
        new Promise((resolve) => {
          finishVideoPreparation = () => resolve(preparedVideo(path))
        }),
    )
    const clearHistory = vi.spyOn(useStore.temporal.getState(), 'clear')
    const onSetBackgroundMode = vi.fn()
    const { result } = renderHook(() =>
      useProjectLifecycle({
        clearImportedVideos: vi.fn(),
        onSetBackgroundMode,
        prepareActivityPath,
        prepareVideoPath,
      }),
    )

    let openPromise
    let finishFonts
    vi.mocked(prepareDocumentFonts).mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          finishFonts = resolve
        }),
    )
    act(() => {
      openPromise = result.current.handleOpenProject()
    })
    await waitFor(() => {
      expect(prepareActivityPath).toHaveBeenCalledOnce()
      expect(prepareVideoPath).toHaveBeenCalledOnce()
    })
    expect(result.current.loadingProject).toBe(true)
    expect(useStore.getState().activitySource).toBeNull()
    expect(useStore.getState().importedVideos).toEqual([])
    finishActivityPreparation()
    finishVideoPreparation()
    await act(async () => Promise.resolve())
    expect(boundaries.registerPreviewVideo).not.toHaveBeenCalled()
    expect(useStore.getState().importedVideos).toEqual([])
    finishFonts()
    await act(async () => openPromise)
    expect(result.current.loadingProject).toBe(false)

    expect(boundaries.openSinglePath).toHaveBeenCalledWith(expect.any(Array), {
      defaultPath: 'C:\\Users\\test\\Documents\\OVRLEY\\projects',
      lastDirectoryKey: 'last-project-dir',
    })
    expect(prepareActivityPath).toHaveBeenCalledWith(activityPath)
    expect(prepareVideoPath).toHaveBeenCalledWith(videoPath)
    expect(boundaries.registerPreviewVideo).toHaveBeenCalledWith(videoPath)

    const state = useStore.getState()
    expect(state.parsedActivity).toBe(preparedActivity.parsedActivity)
    expect(state.config.scene.width).toBe(1280)
    expect(state.globalDefaults.color_text).toBe('#123456')
    expect(state.loadedTemplateSource).toBeNull()
    expect(state.importedVideos).toHaveLength(1)
    const [video] = state.importedVideos
    expect(video).toMatchObject({ id: 'ride-video', path: videoPath, durationSeconds: 40, telemetry: { status: 'absent', activity: null } })
    expect(video.preview).toEqual({ importId: 'owner-import-id', url: 'http://preview/video', warnings: [] })
    expect(state.videoEdits[video.id].offsetSeconds).toBe(12)
    expect(state.manualVideoSync.videoId).toBe(video.id)
    expect(state.videoSyncTimezoneMode).toBe('utc')
    const { batchVideoFolder, batchOutputFolder, ...renderSettings } = project.render
    expect(state.renderSettings).toEqual(renderSettings)
    expect(state.batchVideoFolder).toBe(batchVideoFolder)
    expect(state.batchOutputFolder).toBe(batchOutputFolder)
    expect(state.selectedSecond).toBe(30)
    expect(state.timelineViewport).toEqual({ viewStart: 20, viewEnd: 60 })
    expect(state.previewPlaybackState).toBe('paused')
    expect(onSetBackgroundMode).toHaveBeenCalledWith('video')
    expect(clearHistory).toHaveBeenCalled()
    expect(result.current.status).toBe('Saved')

    act(() => useStore.getState().setSelectedSecond(31))
    expect(result.current.status).toBe('Modified')

    act(() => useStore.getState().setSelectedSecond(30))
    expect(result.current.status).toBe('Saved')

    act(() => useStore.getState().setTimelineViewport({ viewStart: 21, viewEnd: 60 }))
    expect(result.current.status).toBe('Modified')

    act(() => useStore.getState().setTimelineViewport({ viewStart: 20, viewEnd: 60 }))
    expect(result.current.status).toBe('Saved')

    act(() => useStore.getState().setLoadedTemplateSource({ kind: 'bundled', templateId: 'another-template.json' }))
    expect(result.current.status).toBe('Saved')

    act(() => useStore.getState().setRenderTarget('batch'))
    expect(result.current.status).toBe('Modified')
    act(() => useStore.getState().setRenderTarget('current'))
    expect(result.current.status).toBe('Saved')
    act(() => useStore.getState().setBatchVideoFolder('C:\\batch-videos'))
    expect(result.current.status).toBe('Modified')
    act(() => useStore.getState().setBatchVideoFolder(null))
    expect(result.current.status).toBe('Saved')
    act(() => useStore.getState().setBatchOutputFolder('C:\\batch-renders'))
    expect(result.current.status).toBe('Modified')
    act(() => useStore.getState().setBatchOutputFolder(null))
    expect(result.current.status).toBe('Saved')
    act(() => useStore.getState().setBatchQueueFromPaths(['C:\\batch-videos\\ride.mp4']))
    expect(result.current.status).toBe('Saved')
    expect(result.current.status).toBe('Saved')

    act(() => useStore.getState().setVideoSyncDetectedLocation(20))
    expect(result.current.status).toBe('Modified')

    act(() => useStore.getState().clearVideoSyncDetectedLocation())
    expect(result.current.status).toBe('Saved')

    act(() => useStore.getState().setConfig({ ...state.config, scene: { ...state.config.scene, width: 1440 } }))
    expect(result.current.status).toBe('Modified')
  })

  test('Save As does not involve template persistence', async () => {
    boundaries.getDefaultProjectDirectory.mockResolvedValue('C:\\Users\\test\\Documents\\OVRLEY\\projects')
    boundaries.saveSinglePath.mockResolvedValue(null)
    const { result } = renderHook(() =>
      useProjectLifecycle({
        clearImportedVideos: vi.fn(),
        prepareActivityPath: vi.fn(),
        prepareVideoPath: vi.fn(),
      }),
    )

    await act(() => result.current.handleSaveProjectAs())

    expect(boundaries.saveSinglePath).toHaveBeenCalledWith('C:\\Users\\test\\Documents\\OVRLEY\\projects\\project.oly', 'oly', 'OVRLEY Project', {
      lastDirectoryKey: 'last-project-dir',
    })
  })

  test('New Project asks for confirmation and discards changes when the user discards', async () => {
    const initialState = useStore.getState()
    const templateSource = { kind: 'file', path: 'C:\\Templates\\race.json' }
    const templateState = createDurableTemplateState({
      config: {
        ...initialState.config,
        labels: [{ id: 'template-widget', text: 'Template', x: 10, y: 20 }],
      },
      globalDefaults: initialState.globalDefaults,
    })
    useStore.setState({
      loadedTemplateSource: templateSource,
      lastSavedTemplateState: templateState,
      config: {
        ...templateState.config,
        labels: [...templateState.config.labels, { id: 'project-widget', text: 'Project', x: 30, y: 40 }],
      },
      activitySource: { kind: 'file', path: 'C:\\Media\\ride.fit' },
      parsedActivity: { samples: [] },
      activitySummary: { durationSeconds: 120 },
      ...videoStoreState([createTestVideo({ path: 'C:\\Media\\ride.mp4', durationSeconds: 120 })], { 'video-1': 12 }),
      transitionDefaults: { type: 'fade', durationSeconds: 1, splitGapThresholdSeconds: 5, dialogShown: true },
      selectedSecond: 45,
      timelineViewport: { viewStart: 30, viewEnd: 60 },
      batchVideoFolder: 'C:\\batch-videos',
      batchOutputFolder: 'C:\\batch-renders',
      renderSettings: {
        ...DEFAULT_RENDER_SETTINGS,
        renderTarget: 'batch',
        fps: 60,
        range: { type: 'custom', from: 10, to: 80 },
      },
    })

    useStore.getState().setBatchQueueFromPaths(['C:\\batch-videos\\ride.mp4'])
    const clearImportedVideos = vi.fn(async () => useStore.getState().clearImportedVideos())
    const { result } = renderHook(() =>
      useProjectLifecycle({
        clearImportedVideos,
        prepareActivityPath: vi.fn(),
        prepareVideoPath: vi.fn(),
      }),
    )

    let createPromise
    act(() => {
      createPromise = result.current.handleNewProject()
    })
    await waitFor(() => expect(result.current.unsavedProjectDialog.open).toBe(true))
    expect(clearImportedVideos).not.toHaveBeenCalled()

    act(() => result.current.unsavedProjectDialog.onDiscard())
    await act(async () => createPromise)

    const state = useStore.getState()
    expect(clearImportedVideos).toHaveBeenCalledOnce()
    expect(state.config.labels.map((widget) => widget.id)).toEqual(['template-widget'])
    expect(state.loadedTemplateSource).toEqual(templateSource)
    expect(state.lastSavedTemplateState).toEqual(templateState)
    expect(state.activitySource).toBeNull()
    expect(state.parsedActivity).toBeNull()
    expect(state.importedVideos).toEqual([])
    expect(state.videoEdits).toEqual({})
    expect(state.transitionDefaults).toEqual(DEFAULT_TRANSITION_SETTINGS)
    expect(state.renderSettings).toEqual(DEFAULT_RENDER_SETTINGS)
    expect(state.batchVideoFolder).toBeNull()
    expect(state.batchOutputFolder).toBeNull()
    expect(state.batchQueue).toEqual([])
    expect(state.selectedSecond).toBe(0)
    expect(state.timelineViewport).toEqual({ viewStart: 0, viewEnd: 73 })
    expect(result.current.loadedProjectPath).toBeNull()
    expect(result.current.status).toBe('Unsaved')
  })

  test('New Project saves the current project first when the user chooses Save', async () => {
    const initialState = useStore.getState()
    const templateSource = { kind: 'file', path: 'C:\\Templates\\race.json' }
    const templateState = createDurableTemplateState({
      config: {
        ...initialState.config,
        labels: [{ id: 'template-widget', text: 'Template', x: 10, y: 20 }],
      },
      globalDefaults: initialState.globalDefaults,
    })
    useStore.setState({
      loadedTemplateSource: templateSource,
      lastSavedTemplateState: templateState,
      config: {
        ...templateState.config,
        labels: [...templateState.config.labels, { id: 'project-widget', text: 'Project', x: 30, y: 40 }],
      },
    })
    boundaries.getDefaultProjectDirectory.mockResolvedValue('C:\\Projects')
    boundaries.saveSinglePath.mockResolvedValue('C:\\Projects\\project.oly')
    boundaries.writeProjectFile.mockResolvedValue({ path: 'C:\\Projects\\project.oly', backupPath: null })

    const clearImportedVideos = vi.fn(async () => useStore.getState().clearImportedVideos())
    const { result } = renderHook(() =>
      useProjectLifecycle({
        clearImportedVideos,
        prepareActivityPath: vi.fn(),
        prepareVideoPath: vi.fn(),
      }),
    )

    let createPromise
    act(() => {
      createPromise = result.current.handleNewProject()
    })
    await waitFor(() => expect(result.current.unsavedProjectDialog.open).toBe(true))

    act(() => result.current.unsavedProjectDialog.onSave())
    await act(async () => createPromise)

    expect(boundaries.saveSinglePath).toHaveBeenCalledOnce()
    expect(boundaries.writeProjectFile).toHaveBeenCalledOnce()
    expect(useStore.getState().config.labels.map((widget) => widget.id)).toEqual(['template-widget'])
    expect(result.current.loadedProjectPath).toBeNull()
    expect(result.current.status).toBe('Unsaved')
  })

  test('New Project keeps the session when the user cancels the confirmation', async () => {
    useStore.setState({ activitySource: { kind: 'file', path: 'C:\\Media\\ride.fit' }, parsedActivity: { samples: [] } })

    const clearImportedVideos = vi.fn()
    const { result } = renderHook(() =>
      useProjectLifecycle({
        clearImportedVideos,
        prepareActivityPath: vi.fn(),
        prepareVideoPath: vi.fn(),
      }),
    )

    let createPromise
    act(() => {
      createPromise = result.current.handleNewProject()
    })
    await waitFor(() => expect(result.current.unsavedProjectDialog.open).toBe(true))

    act(() => result.current.unsavedProjectDialog.onCancel())
    const outcome = await act(async () => createPromise)

    expect(outcome).toBe(false)
    expect(clearImportedVideos).not.toHaveBeenCalled()
    expect(useStore.getState().activitySource).toEqual({ kind: 'file', path: 'C:\\Media\\ride.fit' })
    expect(result.current.status).toBe('Unsaved')
  })

  test('Load Anyway opens a project with a missing source treated as absent', async () => {
    const projectPath = 'C:\\Events\\Race.oly'
    const missingActivityPath = 'C:\\Events\\missing.fit'
    const initialState = useStore.getState()
    const project = {
      format: 'ovrley-project',
      version: 4,
      savedAt: '2026-08-27T12:00:00.000Z',
      editor: createDurableEditorState({ config: initialState.config, globalDefaults: initialState.globalDefaults }),
      sources: {
        activity: { path: { kind: 'project-relative', value: 'missing.fit' } },
        videos: [],
      },
      sync: {
        videoTimezoneMode: null,
        manual: { videoId: null, landmarks: [], detectedLocationSecond: null, speedThresholdKmh: 5, turnThresholdDegrees: 90 },
      },
      transitionDefaults: { ...DEFAULT_TRANSITION_SETTINGS },
      render: { ...DEFAULT_RENDER_SETTINGS, batchVideoFolder: null, batchOutputFolder: null, range: { ...DEFAULT_RENDER_SETTINGS.range } },
      timeline: { playheadSecond: 0, viewStart: 0, viewEnd: 73 },
    }
    useStore.setState({
      activitySource: { kind: 'file', path: 'C:\\Previous\\activity.fit' },
      parsedActivity: { samples: [] },
      activitySummary: { durationSeconds: 50 },
    })
    boundaries.openSinglePath.mockResolvedValue(projectPath)
    boundaries.getDefaultProjectDirectory.mockResolvedValue('C:\\Users\\test\\Documents\\OVRLEY\\projects')
    boundaries.readProjectFile.mockResolvedValue({
      project,
      resolvedSources: { activityPath: missingActivityPath, videoPaths: [] },
      rasterLoadResults: {},
    })
    boundaries.selectedPathIsFile.mockResolvedValue(false)
    const prepareActivityPath = vi.fn()
    const onSetBackgroundMode = vi.fn()
    const { result } = renderHook(() =>
      useProjectLifecycle({
        clearImportedVideos: vi.fn(),
        onSetBackgroundMode,
        prepareActivityPath,
        prepareVideoPath: vi.fn(),
      }),
    )

    let openPromise
    act(() => {
      openPromise = result.current.handleOpenProject()
    })
    await waitFor(() => expect(result.current.missingSourceDialog.open).toBe(true))
    act(() => result.current.missingSourceDialog.onLoadAnyway())
    await act(async () => openPromise)

    expect(prepareActivityPath).not.toHaveBeenCalled()
    expect(onSetBackgroundMode).not.toHaveBeenCalled()
    expect(useStore.getState().activitySource).toBeNull()
    expect(result.current.loadedProjectPath).toBe(projectPath)
    expect(result.current.status).toBe('Modified')
  })

  test('keeps the current session untouched when either parallel preparation fails', async () => {
    const initialState = useStore.getState()
    const projectPath = 'C:\\Events\\Broken.oly'
    const project = {
      format: 'ovrley-project',
      version: 4,
      savedAt: '2026-08-27T12:00:00.000Z',
      editor: createDurableEditorState({ config: initialState.config, globalDefaults: initialState.globalDefaults }),
      sources: {
        activity: { path: { kind: 'project-relative', value: 'broken.fit' } },
        videos: [projectVideo('ride-video', 'ride.mp4')],
      },
      sync: {
        videoTimezoneMode: null,
        manual: { videoId: 'ride-video', landmarks: [], detectedLocationSecond: null, speedThresholdKmh: 5, turnThresholdDegrees: 90 },
      },
      transitionDefaults: { ...DEFAULT_TRANSITION_SETTINGS },
      render: { ...DEFAULT_RENDER_SETTINGS, batchVideoFolder: null, batchOutputFolder: null, range: { ...DEFAULT_RENDER_SETTINGS.range } },
      timeline: { playheadSecond: 0, viewStart: 0, viewEnd: 73 },
    }
    useStore.setState({
      activitySource: { kind: 'file', path: 'C:\\Current\\ride.fit' },
      importedBackgroundImagePath: 'C:\\Current\\background.png',
    })
    boundaries.getDefaultProjectDirectory.mockResolvedValue('C:\\Projects')
    boundaries.openSinglePath.mockResolvedValue(projectPath)
    boundaries.readProjectFile.mockResolvedValue({
      project,
      resolvedSources: { activityPath: 'C:\\Events\\broken.fit', videoPaths: ['C:\\Events\\ride.mp4'] },
      rasterLoadResults: {},
    })
    boundaries.selectedPathIsFile.mockResolvedValue(true)
    const prepareActivityPath = vi.fn().mockRejectedValue(new Error('invalid activity'))
    const prepareVideoPath = vi.fn().mockResolvedValue(preparedVideo('C:\\Events\\ride.mp4'))
    const { result } = renderHook(() =>
      useProjectLifecycle({
        clearImportedVideos: vi.fn(),
        prepareActivityPath,
        prepareVideoPath,
      }),
    )

    await act(() => result.current.handleOpenProject())

    expect(prepareActivityPath).toHaveBeenCalledOnce()
    expect(prepareVideoPath).toHaveBeenCalledOnce()
    expect(boundaries.registerPreviewVideo).not.toHaveBeenCalled()
    expect(boundaries.clearPreviewVideos).not.toHaveBeenCalled()
    expect(useStore.getState().activitySource.path).toBe('C:\\Current\\ride.fit')
    expect(useStore.getState().importedBackgroundImagePath).toBe('C:\\Current\\background.png')
    expect(result.current.loadingProject).toBe(false)
  })

  test('clears a previous background image when the opened project has no video', async () => {
    const initialState = useStore.getState()
    const projectPath = 'C:\\Events\\Empty.oly'
    const project = {
      format: 'ovrley-project',
      version: 4,
      savedAt: '2026-08-27T12:00:00.000Z',
      editor: createDurableEditorState({ config: initialState.config, globalDefaults: initialState.globalDefaults }),
      sources: { activity: null, videos: [] },
      sync: {
        videoTimezoneMode: null,
        manual: { videoId: null, landmarks: [], detectedLocationSecond: null, speedThresholdKmh: 5, turnThresholdDegrees: 90 },
      },
      transitionDefaults: { ...DEFAULT_TRANSITION_SETTINGS },
      render: {
        ...DEFAULT_RENDER_SETTINGS,
        batchVideoFolder: null,
        batchOutputFolder: null,
        exportMode: 'transparent',
        range: { ...DEFAULT_RENDER_SETTINGS.range },
      },
      timeline: { playheadSecond: 0, viewStart: 0, viewEnd: 73 },
    }
    useStore.setState({ importedBackgroundImagePath: 'C:\\Current\\background.png' })
    boundaries.getDefaultProjectDirectory.mockResolvedValue('C:\\Projects')
    boundaries.openSinglePath.mockResolvedValue(projectPath)
    boundaries.readProjectFile.mockResolvedValue({ project, resolvedSources: { activityPath: null, videoPaths: [] }, rasterLoadResults: {} })
    const { result } = renderHook(() =>
      useProjectLifecycle({
        clearImportedVideos: vi.fn(),
        prepareActivityPath: vi.fn(),
        prepareVideoPath: vi.fn(),
      }),
    )

    await act(() => result.current.handleOpenProject())

    expect(boundaries.clearPreviewVideos).toHaveBeenCalledOnce()
    expect(useStore.getState().importedBackgroundImagePath).toBeNull()
  })

  test('rejects a second project command while the first command owns the lock', async () => {
    let finishDirectoryLookup
    let finishPicking
    boundaries.getDefaultProjectDirectory.mockReturnValue(
      new Promise((resolve) => {
        finishDirectoryLookup = resolve
      }),
    )
    boundaries.openSinglePath.mockReturnValue(
      new Promise((resolve) => {
        finishPicking = resolve
      }),
    )
    const { result } = renderHook(() =>
      useProjectLifecycle({
        clearImportedVideos: vi.fn(),
        prepareActivityPath: vi.fn(),
        prepareVideoPath: vi.fn(),
      }),
    )

    let openPromise
    act(() => {
      openPromise = result.current.handleOpenProject()
    })
    expect(result.current.loadingProject).toBe(false)
    expect(result.current.busy).toBe(true)
    await act(async () => finishDirectoryLookup('C:\\Projects'))
    expect(boundaries.openSinglePath).toHaveBeenCalledOnce()
    expect(result.current.loadingProject).toBe(false)
    expect(result.current.busy).toBe(true)
    const secondResult = await act(() => result.current.handleSaveProjectAs())
    finishPicking(null)
    await act(async () => openPromise)

    expect(secondResult).toBe(false)
    expect(result.current.loadingProject).toBe(false)
    expect(boundaries.getDefaultProjectDirectory).toHaveBeenCalledOnce()
    expect(boundaries.saveSinglePath).not.toHaveBeenCalled()
  })

  function multiVideoProject() {
    const initialState = useStore.getState()
    return {
      format: 'ovrley-project',
      version: 4,
      savedAt: '2026-08-27T12:00:00.000Z',
      editor: createDurableEditorState({ config: initialState.config, globalDefaults: initialState.globalDefaults }),
      sources: {
        activity: null,
        videos: [
          projectVideo('clip-a', 'a.mp4', -5),
          projectVideo('clip-missing', 'missing.mp4', 50),
          {
            ...projectVideo('clip-c', 'c.mp4', 100),
            deletedParts: [{ id: 'cut-1', fromSecond: 5, toSecond: 10, transition: null }],
            transitionIn: { type: 'fade', durationSeconds: 1 },
            placementAnchor: { videoId: 'clip-missing', side: 'after' },
          },
        ],
      },
      sync: {
        videoTimezoneMode: null,
        manual: { videoId: 'clip-c', landmarks: [], detectedLocationSecond: null, speedThresholdKmh: 5, turnThresholdDegrees: 90 },
      },
      transitionDefaults: { type: 'none', durationSeconds: 2, splitGapThresholdSeconds: 3, dialogShown: true },
      render: { ...DEFAULT_RENDER_SETTINGS, batchVideoFolder: null, batchOutputFolder: null, range: { ...DEFAULT_RENDER_SETTINGS.range } },
      timeline: { playheadSecond: 0, viewStart: -5, viewEnd: 140 },
    }
  }

  const multiVideoPaths = ['C:\\Events\\a.mp4', 'C:\\Events\\missing.mp4', 'C:\\Events\\c.mp4']

  test('opens every project video and skips a missing one after Load Anyway', async () => {
    const project = multiVideoProject()
    boundaries.getDefaultProjectDirectory.mockResolvedValue('C:\\Projects')
    boundaries.openSinglePath.mockResolvedValue('C:\\Events\\Session.oly')
    boundaries.readProjectFile.mockResolvedValue({
      project,
      resolvedSources: { activityPath: null, videoPaths: multiVideoPaths },
      rasterLoadResults: {},
    })
    boundaries.selectedPathIsFile.mockImplementation(async (path) => path !== 'C:\\Events\\missing.mp4')
    boundaries.registerPreviewVideo.mockImplementation(async (path) => ({ importId: `import:${path}`, previewUrl: `http://preview/${path}` }))
    const prepareVideoPath = vi.fn(async (path) => preparedVideo(path))
    const { result } = renderHook(() =>
      useProjectLifecycle({ clearImportedVideos: vi.fn(), prepareActivityPath: vi.fn(), prepareVideoPath, onSetBackgroundMode: vi.fn() }),
    )

    let openPromise
    act(() => {
      openPromise = result.current.handleOpenProject()
    })
    await waitFor(() => expect(result.current.missingSourceDialog.open).toBe(true))
    expect(result.current.missingSourceDialog).toMatchObject({ sourceRole: 'video', fileName: 'missing.mp4' })
    act(() => result.current.missingSourceDialog.onLoadAnyway())
    await act(async () => openPromise)

    expect(prepareVideoPath.mock.calls.map(([path]) => path)).toEqual(['C:\\Events\\a.mp4', 'C:\\Events\\c.mp4'])
    const state = useStore.getState()
    expect(state.importedVideos.map((video) => [video.id, video.importOrder, video.path])).toEqual([
      ['clip-a', 0, 'C:\\Events\\a.mp4'],
      ['clip-c', 2, 'C:\\Events\\c.mp4'],
    ])
    expect(state.importedVideos.map((video) => video.preview.importId)).toEqual(['import:C:\\Events\\a.mp4', 'import:C:\\Events\\c.mp4'])
    expect(state.videoEdits).toEqual({
      'clip-a': { offsetSeconds: -5, deletedParts: [], transitionIn: null, placementAnchor: null },
      'clip-c': {
        offsetSeconds: 100,
        deletedParts: [{ id: 'cut-1', fromSecond: 5, toSecond: 10, transition: null }],
        transitionIn: { type: 'fade', durationSeconds: 1 },
        placementAnchor: null,
      },
    })
    expect(state.manualVideoSync.videoId).toBe('clip-c')
    expect(state.transitionDefaults).toEqual(project.transitionDefaults)
    expect(result.current.status).toBe('Modified')
  })

  test('removes registrations already made when a later project video fails to register', async () => {
    const project = multiVideoProject()
    useStore.setState({ activitySource: { kind: 'file', path: 'C:\\Current\\ride.fit' } })
    boundaries.readProjectFile.mockResolvedValue({
      project,
      resolvedSources: { activityPath: null, videoPaths: multiVideoPaths },
      rasterLoadResults: {},
    })
    boundaries.registerPreviewVideo
      .mockResolvedValueOnce({ importId: 'first-import', previewUrl: 'http://preview/a' })
      .mockResolvedValueOnce({ importId: 'second-import', previewUrl: 'http://preview/missing' })
      .mockRejectedValueOnce(new Error('preview server unavailable'))

    await expect(
      loadProject({
        path: 'C:\\Events\\Session.oly',
        resolveProjectSources: vi.fn().mockResolvedValue({ activityPath: null, videoPaths: multiVideoPaths }),
        prepareActivityPath: vi.fn(),
        prepareVideoPath: vi.fn(async (path) => preparedVideo(path)),
      }),
    ).rejects.toThrow('preview server unavailable')

    expect(boundaries.removePreviewVideo.mock.calls).toEqual([['first-import'], ['second-import']])
    expect(boundaries.clearPreviewVideos).not.toHaveBeenCalled()
    expect(useStore.getState().importedVideos).toEqual([])
    expect(useStore.getState().activitySource.path).toBe('C:\\Current\\ride.fit')
  })

  test('tells the user once when a save replaced an older-format project', async () => {
    boundaries.getDefaultProjectDirectory.mockResolvedValue('C:\\Projects')
    boundaries.saveSinglePath.mockResolvedValue('C:\\Projects\\Legacy.oly')
    boundaries.writeProjectFile.mockResolvedValueOnce({ path: 'C:\\Projects\\Legacy.oly', backupPath: 'C:\\Projects\\Legacy.oly.v3.bak' })
    boundaries.writeProjectFile.mockResolvedValueOnce({ path: 'C:\\Projects\\Legacy.oly', backupPath: null })
    const { result } = renderHook(() =>
      useProjectLifecycle({ clearImportedVideos: vi.fn(), prepareActivityPath: vi.fn(), prepareVideoPath: vi.fn() }),
    )
    expect(result.current.formatUpgradeDialog.open).toBe(false)

    await act(() => result.current.handleSaveProjectAs())

    expect(boundaries.writeProjectFile).toHaveBeenCalledWith('C:\\Projects\\Legacy.oly', expect.stringContaining('"version": 4'), expect.any(Object))
    expect(result.current.formatUpgradeDialog).toMatchObject({ open: true, backupPath: 'C:\\Projects\\Legacy.oly.v3.bak' })
    act(() => result.current.formatUpgradeDialog.onClose())
    expect(result.current.formatUpgradeDialog.open).toBe(false)

    await act(() => result.current.handleSaveProject())
    expect(result.current.formatUpgradeDialog.open).toBe(false)
  })
})
