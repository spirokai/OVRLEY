import { getCurrentParsedActivity } from './activityCache'
import useStore from '../store/useStore'
import * as backend from './backend'
import { applyGlobalDefaults } from '../lib/config-utils'

// Helper to convert HH:MM:SS to seconds
function timeToSeconds(timeStr) {
  if (!timeStr) return 0
  const parts = timeStr.split(':').map(Number)
  if (parts.length === 3) {
    return parts[0] * 3600 + parts[1] * 60 + parts[2]
  }
  if (parts.length === 2) {
    return parts[0] * 60 + parts[1]
  }
  return parts[0] || 0
}

export default async function renderVideo() {
  try {
    const {
      gpxFilename,
      config: baseConfig,
      globalDefaults,
      updateRate,
      exportRange,
      setRenderingVideo,
      setVideoFilename,
    } = useStore.getState()

    const parsedActivity = getCurrentParsedActivity()

    // Validate we have required data
    if (!baseConfig || !baseConfig.scene) {
      throw new Error('No valid config available')
    }

    // Apply global defaults and overrides
    const config = applyGlobalDefaults(baseConfig, globalDefaults)

    // Apply performance and range overrides
    if (config.scene) {
      config.scene.update_rate = updateRate

      // Apply export range override if custom
      if (exportRange.type === 'custom') {
        const start = timeToSeconds(exportRange.fromTime)
        const end = timeToSeconds(exportRange.toTime)
        if (end > start) {
          config.scene.start = start
          config.scene.end = end
        }
      }
    }

    if (!gpxFilename) {
      throw new Error('No GPX file selected')
    }

    if (parsedActivity && gpxFilename !== 'demo.gpxinit') {
      throw new Error(
        'Render for frontend-parsed activities is not connected to the legacy backend renderer yet.',
      )
    }

    if (config.scene.start === undefined || config.scene.end === undefined) {
      throw new Error('Timeline start and end must be set')
    }

    if (config.scene.start >= config.scene.end) {
      throw new Error('Start time must be before end time')
    }

    setRenderingVideo(true)

    console.log('📤 Sending video render request:', {
      gpx: gpxFilename,
      start: config?.scene?.start,
      end: config?.scene?.end,
      duration: (config?.scene?.end || 0) - (config?.scene?.start || 0),
      updateRate: config?.scene?.update_rate,
    })

    const data = await backend.renderVideo(config, gpxFilename)

    if (data.error) {
      // Check for cancellation
      if (data.cancelled || data.error.toLowerCase().includes('cancelled')) {
        console.log('Render cancelled by user')
        return { success: false, cancelled: true }
      }
      throw new Error(data.error)
    }

    const videoFilename = data.filename

    if (videoFilename) {
      setVideoFilename(videoFilename)

      // Tell backend to open the video in default player
      try {
        await backend.openVideo(videoFilename)
      } catch (e) {
        console.error('Error calling open-video:', e)
      }

      return { success: true, filename: videoFilename }
    }

    throw new Error('No video filename returned')
  } catch (error) {
    console.error('Error in renderVideo:', error)
    throw error
  } finally {
    const { setRenderingVideo } = useStore.getState()
    setRenderingVideo(false)
  }
}
