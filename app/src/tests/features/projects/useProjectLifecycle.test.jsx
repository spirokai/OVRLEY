import { act, renderHook, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, test, vi } from 'vitest'
import { createDurableTemplateState } from '@/lib/template/template-state'
import { createDurableEditorState } from '@/lib/widget/editor-state'
import { DEFAULT_RENDER_SETTINGS } from '@/store/slices/createRenderSettingsSlice'
import useStore from '@/store/useStore'

const boundaries = vi.hoisted(() => ({
  getDefaultProjectDirectory: vi.fn(),
  openSinglePath: vi.fn(),
  readProjectFile: vi.fn(),
  saveSinglePath: vi.fn(),
  selectedPathIsFile: vi.fn(),
  writeProjectFile: vi.fn(),
}))

vi.mock('@/api/backend', async (importOriginal) => ({
  ...(await importOriginal()),
  getDefaultProjectDirectory: boundaries.getDefaultProjectDirectory,
  readProjectFile: boundaries.readProjectFile,
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

    boundaries.openSinglePath.mockResolvedValue(projectPath)
    boundaries.getDefaultProjectDirectory.mockResolvedValue('C:\\Users\\test\\Documents\\OVRLEY\\projects')
    boundaries.readProjectFile.mockResolvedValue({
      project,
      resolvedSources: { activityPath, videoPath },
    })
    boundaries.selectedPathIsFile.mockResolvedValue(true)
    let finishActivityLoad
    let finishVideoLoad
    const loadActivityPath = vi.fn(
      (path) =>
        new Promise((resolve) => {
          finishActivityLoad = () => {
            useStore.setState({
              activitySource: { kind: 'file', path },
              parsedActivity: { owner: 'activity-loader' },
              parsedActivitySource: 'activity-file',
              activitySummary: { durationSeconds: 100 },
            })
            resolve()
          }
        }),
    )
    const loadVideoPath = vi.fn(
      (path) =>
        new Promise((resolve) => {
          finishVideoLoad = () => {
            useStore.setState({
              importedVideoPath: path,
              importedVideoDuration: 40,
              importedVideoResolution: { width: 1920, height: 1080 },
              importedVideoImportId: 'owner-import-id',
            })
            resolve()
          }
        }),
    )
    const clearHistory = vi.spyOn(useStore.temporal.getState(), 'clear')
    const { default: useProjectLifecycle } = await import('@/features/projects/hooks/useProjectLifecycle')
    const { result } = renderHook(() =>
      useProjectLifecycle({
        loadActivityPath,
        clearImportedVideo: vi.fn(),
        loadVideoPath,
      }),
    )

    let openPromise
    act(() => {
      openPromise = result.current.handleOpenProject()
    })
    await waitFor(() => {
      expect(loadActivityPath).toHaveBeenCalledOnce()
      expect(loadVideoPath).toHaveBeenCalledOnce()
    })
    finishActivityLoad()
    finishVideoLoad()
    await act(async () => openPromise)

    expect(boundaries.openSinglePath).toHaveBeenCalledWith(expect.any(Array), {
      defaultPath: 'C:\\Users\\test\\Documents\\OVRLEY\\projects',
      lastDirectoryKey: 'last-project-dir',
    })
    expect(loadActivityPath).toHaveBeenCalledOnce()
    expect(loadActivityPath).toHaveBeenCalledWith(activityPath)
    expect(loadVideoPath).toHaveBeenCalledOnce()
    expect(loadVideoPath).toHaveBeenCalledWith(videoPath)

    const state = useStore.getState()
    expect(state.parsedActivity).toEqual({ owner: 'activity-loader' })
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
        loadActivityPath: vi.fn(),
        clearImportedVideo: vi.fn(),
        loadVideoPath: vi.fn(),
      }),
    )

    await act(() => result.current.handleSaveProjectAs())

    expect(boundaries.saveSinglePath).toHaveBeenCalledWith('C:\\Users\\test\\Documents\\OVRLEY\\projects\\project.oly', 'oly', 'OVRLEY Project', {
      lastDirectoryKey: 'last-project-dir',
    })
  })

  test('New Project clears session state and restores only the loaded template widget baseline', async () => {
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
        loadActivityPath: vi.fn(),
        clearImportedVideo,
        loadVideoPath: vi.fn(),
      }),
    )

    await act(() => result.current.handleNewProject())

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
    const loadActivityPath = vi.fn()
    const { default: useProjectLifecycle } = await import('@/features/projects/hooks/useProjectLifecycle')
    const { result } = renderHook(() =>
      useProjectLifecycle({
        loadActivityPath,
        clearImportedVideo: vi.fn(),
        loadVideoPath: vi.fn(),
      }),
    )

    let openPromise
    act(() => {
      openPromise = result.current.handleOpenProject()
    })
    await waitFor(() => expect(result.current.missingSourceDialog.open).toBe(true))
    act(() => result.current.missingSourceDialog.onLoadAnyway())
    await act(async () => openPromise)

    expect(loadActivityPath).not.toHaveBeenCalled()
    expect(useStore.getState().activitySource).toBeNull()
    expect(result.current.loadedProjectPath).toBe(projectPath)
    expect(result.current.status).toBe('Modified')
  })
})
