import { act, renderHook, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, test, vi } from 'vitest'
import { createDurableTemplateState } from '@/lib/template/template-state'
import { createDurableEditorState } from '@/lib/widget/editor-state'
import { DEFAULT_RENDER_SETTINGS } from '@/store/slices/createRenderSettingsSlice'
import useStore from '@/store/useStore'

const boundaries = vi.hoisted(() => ({
  clearPreviewVideo: vi.fn(),
  getDefaultProjectDirectory: vi.fn(),
  openSinglePath: vi.fn(),
  readProjectFile: vi.fn(),
  registerPreviewVideo: vi.fn(),
  saveSinglePath: vi.fn(),
  selectedPathIsFile: vi.fn(),
  writeProjectFile: vi.fn(),
}))

vi.mock('@/api/backend', async (importOriginal) => ({
  ...(await importOriginal()),
  clearPreviewVideo: boundaries.clearPreviewVideo,
  getDefaultProjectDirectory: boundaries.getDefaultProjectDirectory,
  readProjectFile: boundaries.readProjectFile,
  registerPreviewVideo: boundaries.registerPreviewVideo,
  selectedPathIsFile: boundaries.selectedPathIsFile,
  writeProjectFile: boundaries.writeProjectFile,
}))

vi.mock('@/lib/file-dialog', async (importOriginal) => ({
  ...(await importOriginal()),
  openSinglePath: boundaries.openSinglePath,
  saveSinglePath: boundaries.saveSinglePath,
}))

describe('useProjectLifecycle canonical load orchestration', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    useStore.setState(useStore.getInitialState(), true)
    useStore.temporal.getState().clear()
    boundaries.clearPreviewVideo.mockResolvedValue(null)
    boundaries.registerPreviewVideo.mockResolvedValue({ importId: 'owner-import-id', previewUrl: 'http://preview/video' })
  })

  test('delegates source population to each owner before restoring project settings', async () => {
    const projectPath = 'C:\\Events\\Race.oly'
    const activityPath = 'C:\\Events\\ride.fit'
    const videoPath = 'C:\\Events\\ride.mp4'
    const initialState = useStore.getState()
    const project = {
      format: 'ovrley-project',
      version: 1,
      savedAt: '2026-08-27T12:00:00.000Z',
      editor: createDurableEditorState({
        config: { ...initialState.config, scene: { ...initialState.config.scene, width: 1280 } },
        globalDefaults: { ...initialState.globalDefaults, color_text: '#123456' },
      }),
      sources: {
        activity: { path: { kind: 'project-relative', value: 'ride.fit' } },
        video: { path: { kind: 'project-relative', value: 'ride.mp4' } },
      },
      sync: { videoOffsetSeconds: 12, videoTimezoneMode: 'utc' },
      render: {
        fps: 60,
        widgetUpdateRate: 2,
        exportMode: 'composite',
        codec: 'libx264',
        bitrateMbps: 20,
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
      resolvedSources: { activityPath, videoPath },
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
          finishVideoPreparation = () =>
            resolve({
              path,
              telemetry: null,
              importedVideoState: {
                importedVideoPath: path,
                importedVideoDuration: 40,
                importedVideoResolution: { width: 1920, height: 1080 },
              },
            })
        }),
    )
    const clearHistory = vi.spyOn(useStore.temporal.getState(), 'clear')
    const onSetBackgroundMode = vi.fn()
    const { default: useProjectLifecycle } = await import('@/features/projects/hooks/useProjectLifecycle')
    const { result } = renderHook(() =>
      useProjectLifecycle({
        clearImportedVideo: vi.fn(),
        onSetBackgroundMode,
        prepareActivityPath,
        prepareVideoPath,
      }),
    )

    let openPromise
    act(() => {
      openPromise = result.current.handleOpenProject()
    })
    await waitFor(() => {
      expect(prepareActivityPath).toHaveBeenCalledOnce()
      expect(prepareVideoPath).toHaveBeenCalledOnce()
    })
    expect(useStore.getState().activitySource).toBeNull()
    expect(useStore.getState().importedVideoPath).toBeNull()
    finishActivityPreparation()
    finishVideoPreparation()
    await act(async () => openPromise)

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
    expect(state.importedVideoImportId).toBe('owner-import-id')
    expect(state.videoSyncOffsetSeconds).toBe(12)
    expect(state.videoSyncTimezoneMode).toBe('utc')
    expect(state.renderSettings).toEqual(project.render)
    expect(state.selectedSecond).toBe(30)
    expect(state.timelineViewport).toEqual({ viewStart: 20, viewEnd: 60 })
    expect(state.previewPlaybackState).toBe('paused')
    expect(onSetBackgroundMode).toHaveBeenCalledWith('video')
    expect(clearHistory).toHaveBeenCalled()
    expect(result.current.status).toBe('Saved')

    act(() => useStore.getState().setLoadedTemplateSource({ kind: 'bundled', templateId: 'another-template.json' }))
    expect(result.current.status).toBe('Saved')

    act(() => useStore.getState().setConfig({ ...state.config, scene: { ...state.config.scene, width: 1440 } }))
    expect(result.current.status).toBe('Modified')
  })

  test('Save As does not involve template persistence', async () => {
    boundaries.getDefaultProjectDirectory.mockResolvedValue('C:\\Users\\test\\Documents\\OVRLEY\\projects')
    boundaries.saveSinglePath.mockResolvedValue(null)
    const { default: useProjectLifecycle } = await import('@/features/projects/hooks/useProjectLifecycle')
    const { result } = renderHook(() =>
      useProjectLifecycle({
        clearImportedVideo: vi.fn(),
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
      parsedActivitySource: 'activity-file',
      activitySummary: { durationSeconds: 120 },
      importedVideoPath: 'C:\\Media\\ride.mp4',
      importedVideoDuration: 120,
      selectedSecond: 45,
      timelineViewport: { viewStart: 30, viewEnd: 60 },
      videoSyncOffsetSeconds: 12,
      renderSettings: {
        ...DEFAULT_RENDER_SETTINGS,
        fps: 60,
        range: { type: 'custom', from: 10, to: 80 },
      },
    })

    const clearImportedVideo = vi.fn(async () => useStore.getState().clearImportedVideo())
    const { default: useProjectLifecycle } = await import('@/features/projects/hooks/useProjectLifecycle')
    const { result } = renderHook(() =>
      useProjectLifecycle({
        clearImportedVideo,
        prepareActivityPath: vi.fn(),
        prepareVideoPath: vi.fn(),
      }),
    )

    let createPromise
    act(() => {
      createPromise = result.current.handleNewProject()
    })
    await waitFor(() => expect(result.current.newProjectConfirmDialog.open).toBe(true))
    expect(clearImportedVideo).not.toHaveBeenCalled()

    act(() => result.current.newProjectConfirmDialog.onDiscard())
    await act(async () => createPromise)

    const state = useStore.getState()
    expect(clearImportedVideo).toHaveBeenCalledOnce()
    expect(state.config.labels.map((widget) => widget.id)).toEqual(['template-widget'])
    expect(state.loadedTemplateSource).toEqual(templateSource)
    expect(state.lastSavedTemplateState).toEqual(templateState)
    expect(state.activitySource).toBeNull()
    expect(state.parsedActivity).toBeNull()
    expect(state.importedVideoPath).toBeNull()
    expect(state.videoSyncOffsetSeconds).toBe(0)
    expect(state.renderSettings).toEqual(DEFAULT_RENDER_SETTINGS)
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
    boundaries.writeProjectFile.mockResolvedValue(null)

    const clearImportedVideo = vi.fn(async () => useStore.getState().clearImportedVideo())
    const { default: useProjectLifecycle } = await import('@/features/projects/hooks/useProjectLifecycle')
    const { result } = renderHook(() =>
      useProjectLifecycle({
        clearImportedVideo,
        prepareActivityPath: vi.fn(),
        prepareVideoPath: vi.fn(),
      }),
    )

    let createPromise
    act(() => {
      createPromise = result.current.handleNewProject()
    })
    await waitFor(() => expect(result.current.newProjectConfirmDialog.open).toBe(true))

    act(() => result.current.newProjectConfirmDialog.onSave())
    await act(async () => createPromise)

    expect(boundaries.saveSinglePath).toHaveBeenCalledOnce()
    expect(boundaries.writeProjectFile).toHaveBeenCalledOnce()
    expect(useStore.getState().config.labels.map((widget) => widget.id)).toEqual(['template-widget'])
    expect(result.current.loadedProjectPath).toBeNull()
    expect(result.current.status).toBe('Unsaved')
  })

  test('New Project keeps the session when the user cancels the confirmation', async () => {
    const initialState = useStore.getState()
    useStore.setState({ activitySource: { kind: 'file', path: 'C:\\Media\\ride.fit' }, parsedActivity: { samples: [] } })

    const clearImportedVideo = vi.fn()
    const { default: useProjectLifecycle } = await import('@/features/projects/hooks/useProjectLifecycle')
    const { result } = renderHook(() =>
      useProjectLifecycle({
        clearImportedVideo,
        prepareActivityPath: vi.fn(),
        prepareVideoPath: vi.fn(),
      }),
    )

    let createPromise
    act(() => {
      createPromise = result.current.handleNewProject()
    })
    await waitFor(() => expect(result.current.newProjectConfirmDialog.open).toBe(true))

    act(() => result.current.newProjectConfirmDialog.onCancel())
    const outcome = await act(async () => createPromise)

    expect(outcome).toBe(false)
    expect(clearImportedVideo).not.toHaveBeenCalled()
    expect(useStore.getState().activitySource).toEqual({ kind: 'file', path: 'C:\\Media\\ride.fit' })
    expect(result.current.status).toBe('Unsaved')
  })

  test('Load Anyway opens a project with a missing source treated as absent', async () => {
    const projectPath = 'C:\\Events\\Race.oly'
    const missingActivityPath = 'C:\\Events\\missing.fit'
    const initialState = useStore.getState()
    const project = {
      format: 'ovrley-project',
      version: 1,
      savedAt: '2026-08-27T12:00:00.000Z',
      editor: createDurableEditorState({ config: initialState.config, globalDefaults: initialState.globalDefaults }),
      sources: {
        activity: { path: { kind: 'project-relative', value: 'missing.fit' } },
        video: null,
      },
      sync: { videoOffsetSeconds: 0, videoTimezoneMode: null },
      render: { ...DEFAULT_RENDER_SETTINGS, range: { ...DEFAULT_RENDER_SETTINGS.range } },
      timeline: { playheadSecond: 0, viewStart: 0, viewEnd: 73 },
    }
    useStore.setState({
      activitySource: { kind: 'file', path: 'C:\\Previous\\activity.fit' },
      parsedActivity: { samples: [] },
      parsedActivitySource: 'activity-file',
      activitySummary: { durationSeconds: 50 },
    })
    boundaries.openSinglePath.mockResolvedValue(projectPath)
    boundaries.getDefaultProjectDirectory.mockResolvedValue('C:\\Users\\test\\Documents\\OVRLEY\\projects')
    boundaries.readProjectFile.mockResolvedValue({
      project,
      resolvedSources: { activityPath: missingActivityPath, videoPath: null },
    })
    boundaries.selectedPathIsFile.mockResolvedValue(false)
    const prepareActivityPath = vi.fn()
    const onSetBackgroundMode = vi.fn()
    const { default: useProjectLifecycle } = await import('@/features/projects/hooks/useProjectLifecycle')
    const { result } = renderHook(() =>
      useProjectLifecycle({
        clearImportedVideo: vi.fn(),
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
      version: 1,
      savedAt: '2026-08-27T12:00:00.000Z',
      editor: createDurableEditorState({ config: initialState.config, globalDefaults: initialState.globalDefaults }),
      sources: {
        activity: { path: { kind: 'project-relative', value: 'broken.fit' } },
        video: { path: { kind: 'project-relative', value: 'ride.mp4' } },
      },
      sync: { videoOffsetSeconds: 0, videoTimezoneMode: null },
      render: { ...DEFAULT_RENDER_SETTINGS, range: { ...DEFAULT_RENDER_SETTINGS.range } },
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
      resolvedSources: { activityPath: 'C:\\Events\\broken.fit', videoPath: 'C:\\Events\\ride.mp4' },
    })
    boundaries.selectedPathIsFile.mockResolvedValue(true)
    const prepareActivityPath = vi.fn().mockRejectedValue(new Error('invalid activity'))
    const prepareVideoPath = vi.fn().mockResolvedValue({ path: 'C:\\Events\\ride.mp4' })
    const { default: useProjectLifecycle } = await import('@/features/projects/hooks/useProjectLifecycle')
    const { result } = renderHook(() =>
      useProjectLifecycle({
        clearImportedVideo: vi.fn(),
        prepareActivityPath,
        prepareVideoPath,
      }),
    )

    await act(() => result.current.handleOpenProject())

    expect(prepareActivityPath).toHaveBeenCalledOnce()
    expect(prepareVideoPath).toHaveBeenCalledOnce()
    expect(boundaries.registerPreviewVideo).not.toHaveBeenCalled()
    expect(boundaries.clearPreviewVideo).not.toHaveBeenCalled()
    expect(useStore.getState().activitySource.path).toBe('C:\\Current\\ride.fit')
    expect(useStore.getState().importedBackgroundImagePath).toBe('C:\\Current\\background.png')
  })

  test('clears a previous background image when the opened project has no video', async () => {
    const initialState = useStore.getState()
    const projectPath = 'C:\\Events\\Empty.oly'
    const project = {
      format: 'ovrley-project',
      version: 1,
      savedAt: '2026-08-27T12:00:00.000Z',
      editor: createDurableEditorState({ config: initialState.config, globalDefaults: initialState.globalDefaults }),
      sources: { activity: null, video: null },
      sync: { videoOffsetSeconds: 0, videoTimezoneMode: null },
      render: { ...DEFAULT_RENDER_SETTINGS, exportMode: 'transparent', range: { ...DEFAULT_RENDER_SETTINGS.range } },
      timeline: { playheadSecond: 0, viewStart: 0, viewEnd: 73 },
    }
    useStore.setState({ importedBackgroundImagePath: 'C:\\Current\\background.png' })
    boundaries.getDefaultProjectDirectory.mockResolvedValue('C:\\Projects')
    boundaries.openSinglePath.mockResolvedValue(projectPath)
    boundaries.readProjectFile.mockResolvedValue({ project, resolvedSources: { activityPath: null, videoPath: null } })
    const { default: useProjectLifecycle } = await import('@/features/projects/hooks/useProjectLifecycle')
    const { result } = renderHook(() =>
      useProjectLifecycle({
        clearImportedVideo: vi.fn(),
        prepareActivityPath: vi.fn(),
        prepareVideoPath: vi.fn(),
      }),
    )

    await act(() => result.current.handleOpenProject())

    expect(boundaries.clearPreviewVideo).toHaveBeenCalledOnce()
    expect(useStore.getState().importedBackgroundImagePath).toBeNull()
  })

  test('rejects a second project command while the first command owns the lock', async () => {
    let finishDirectoryLookup
    boundaries.getDefaultProjectDirectory.mockReturnValue(
      new Promise((resolve) => {
        finishDirectoryLookup = resolve
      }),
    )
    const { default: useProjectLifecycle } = await import('@/features/projects/hooks/useProjectLifecycle')
    const { result } = renderHook(() =>
      useProjectLifecycle({
        clearImportedVideo: vi.fn(),
        prepareActivityPath: vi.fn(),
        prepareVideoPath: vi.fn(),
      }),
    )

    let openPromise
    act(() => {
      openPromise = result.current.handleOpenProject()
    })
    expect(result.current.loadingProject).toBe(true)
    const secondResult = await act(() => result.current.handleSaveProjectAs())
    finishDirectoryLookup('C:\\Projects')
    boundaries.openSinglePath.mockResolvedValue(null)
    await act(async () => openPromise)

    expect(secondResult).toBe(false)
    expect(result.current.loadingProject).toBe(false)
    expect(boundaries.getDefaultProjectDirectory).toHaveBeenCalledOnce()
    expect(boundaries.saveSinglePath).not.toHaveBeenCalled()
  })
})
