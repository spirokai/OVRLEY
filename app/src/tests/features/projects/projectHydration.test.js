import { expect, test } from 'vitest'
import { applyProjectOwnedState } from '@/features/projects/utils/projectHydration'
import useStore from '@/store/useStore'

test('project restoration changes only project-owned settings', () => {
  useStore.setState(useStore.getInitialState(), true)
  const parsedActivity = { canonical: 'activity' }
  const videoResolution = { width: 1920, height: 1080 }
  useStore.setState({
    activitySummary: { durationSeconds: 100 },
    parsedActivity,
    importedVideoPath: 'C:\\video.mp4',
    importedVideoDuration: 30,
    importedVideoResolution: videoResolution,
    importedVideoImportId: 'canonical-import',
  })

  applyProjectOwnedState(useStore, {
    editor: {
      config: { ...useStore.getState().config, scene: { ...useStore.getState().config.scene, width: 1280 } },
      globalDefaults: { ...useStore.getState().globalDefaults, color_text: '#123456' },
    },
    sync: { videoOffsetSeconds: 20, videoTimezoneMode: 'utc' },
    render: {
      fps: 60,
      widgetUpdateRate: 2,
      exportMode: 'composite',
      codec: 'libx264',
      bitrateMbps: 20,
      range: { type: 'custom', from: 10, to: 80 },
    },
    timeline: { playheadSecond: 50, viewStart: 25, viewEnd: 75 },
  })

  const state = useStore.getState()
  expect(state.parsedActivity).toBe(parsedActivity)
  expect(state.config.scene.width).toBe(1280)
  expect(state.globalDefaults.color_text).toBe('#123456')
  expect(state.loadedTemplateSource).toBeNull()
  expect(state.lastSavedTemplateState).toBeNull()
  expect(state.importedVideoResolution).toBe(videoResolution)
  expect(state.importedVideoImportId).toBe('canonical-import')
  expect(state.videoSyncOffsetSeconds).toBe(20)
  expect(state.videoSyncTimezoneMode).toBe('utc')
  expect(state.renderSettings).toMatchObject({ fps: 60, codec: 'libx264' })
  expect(state.selectedSecond).toBe(50)
  expect(state.timelineViewport).toEqual({ viewStart: 25, viewEnd: 75 })
  expect(state.previewPlaybackState).toBe('paused')
})
