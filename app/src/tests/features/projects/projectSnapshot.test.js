import { describe, expect, test } from 'vitest'
import { createProjectDirtyProjection, createProjectSnapshot } from '@/features/projects/utils/projectSnapshot'
import { createPathLocator } from '@/features/projects/utils/projectPaths'
import useStore from '@/store/useStore'

describe('project snapshot contract', () => {
  test('projects only sources and project-owned settings', () => {
    useStore.setState(useStore.getInitialState(), true)
    useStore.setState((state) => ({
      loadedTemplateSource: { kind: 'file', path: 'C:\\Events\\templates\\race.json' },
      activitySource: { kind: 'file', path: 'C:\\Events\\media\\ride.fit' },
      importedVideoPath: 'D:\\video\\lap.mp4',
      parsedActivity: { samples: [1, 2, 3] },
      activitySummary: { durationSeconds: 100 },
      importedVideoImportId: 'runtime-id',
      importedVideoPreviewUrl: 'http://127.0.0.1/runtime-id',
      importedVideoDuration: 100,
      renderSettings: {
        ...state.renderSettings,
        fps: 60,
        codec: 'h264_nvenc',
      },
      selectedSecond: 12.5,
      timelineViewport: { viewStart: 10, viewEnd: 30 },
    }))

    const project = createProjectSnapshot(useStore.getState(), 'C:\\Events\\Race.oly')

    expect(project).not.toHaveProperty('template')
    expect(project.editor.config).toEqual(expect.objectContaining({ scene: expect.objectContaining({ width: 1920, height: 1080 }) }))
    expect(project.editor.config.scene.fps).toBe(project.render.fps)
    expect(project.editor.config.scene.updateRate).toBe(project.render.widgetUpdateRate)
    expect(project.editor.globalDefaults).toEqual(useStore.getState().globalDefaults)
    expect(project.sources).toEqual({
      activity: { path: { kind: 'project-relative', value: 'media/ride.fit' } },
      video: { path: { kind: 'absolute', value: 'D:\\video\\lap.mp4' } },
    })
    expect(project.render.fps).toBe(60)
    expect(project.timeline).toEqual({ playheadSecond: 12.5, viewStart: 10, viewEnd: 30 })

    const serialized = JSON.stringify(project)
    for (const forbidden of [
      'parsedActivity',
      'activitySummary',
      'importedVideoImportId',
      'importedVideoPreviewUrl',
      'importedVideoDuration',
      'isVideoMuted',
      'selectedWidgetIds',
    ]) {
      expect(serialized).not.toContain(forbidden)
    }

    expect(createProjectDirtyProjection(project)).not.toHaveProperty('savedAt')
    expect(createProjectDirtyProjection(project).editor).toEqual(project.editor)
  })

  test('creates child-relative and external absolute locators', () => {
    expect(createPathLocator('C:\\Events\\media\\ride.fit', 'C:\\Events\\Race.oly')).toEqual({
      kind: 'project-relative',
      value: 'media/ride.fit',
    })
    expect(createPathLocator('D:\\media\\ride.fit', 'C:\\Events\\Race.oly')).toEqual({
      kind: 'absolute',
      value: 'D:\\media\\ride.fit',
    })
  })

  test('editor widget state is identical regardless of the selected template source', () => {
    useStore.setState(useStore.getInitialState(), true)
    const withoutTemplate = createProjectSnapshot(useStore.getState(), 'C:\\Events\\Race.oly')
    useStore.setState({ loadedTemplateSource: { kind: 'bundled', templateId: 'acid-titanium.json' } })
    const withTemplate = createProjectSnapshot(useStore.getState(), 'C:\\Events\\Race.oly')

    expect(withTemplate.editor).toEqual(withoutTemplate.editor)
    expect(withTemplate).not.toHaveProperty('template')
  })

  test('serializes a valid transparent mode when the default composite source is absent', () => {
    useStore.setState(useStore.getInitialState(), true)

    const project = createProjectSnapshot(useStore.getState(), 'C:\\Events\\Race.oly')

    expect(useStore.getState().renderSettings.exportMode).toBe('composite')
    expect(project.sources.video).toBeNull()
    expect(project.render.exportMode).toBe('transparent')
  })
})
