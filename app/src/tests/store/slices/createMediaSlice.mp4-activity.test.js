import { beforeEach, describe, expect, test, vi } from 'vitest'
import useStore from '@/store/useStore'

const mp4Telemetry = {
  metadata: { duration_seconds: 120, sample_count: 240 },
  file_format: 'mp4-telemetry',
  valid_attributes: ['speed'],
}

const activityFile = {
  metadata: { duration_seconds: 3600, sample_count: 7200 },
  file_format: 'fit',
  valid_attributes: ['speed', 'heart_rate'],
}

const shortMp4Telemetry = {
  metadata: { duration_seconds: 2.509, sample_count: 45 },
  trim_end_seconds: 2.509,
  file_format: 'mp4-telemetry',
  valid_attributes: ['speed'],
}

function resetStore() {
  useStore.setState(useStore.getInitialState(), true)
}

describe('MP4 activity — store actions', () => {
  beforeEach(resetStore)

  describe('loadVideoTelemetry', () => {
    test('sets parsedActivity when no activity file is active', () => {
      useStore.getState().loadVideoTelemetry(mp4Telemetry)

      const state = useStore.getState()
      expect(state.parsedActivity).toEqual(mp4Telemetry)
      expect(state.parsedActivitySource).toBe('video-telemetry')
      expect(state.hiddenVideoParsedActivity).toBeNull()
    })

    test('stores MP4 telemetry in hiddenVideoParsedActivity when activity file is active', () => {
      useStore.getState().activateActivityFile(activityFile)
      useStore.getState().loadVideoTelemetry(mp4Telemetry)

      const state = useStore.getState()
      expect(state.parsedActivity).toEqual(activityFile)
      expect(state.parsedActivitySource).toBe('activity-file')
      expect(state.hiddenVideoParsedActivity).toEqual(mp4Telemetry)
    })

    test('sets videoSyncOffsetSeconds to 0 and clears videoSyncWarning when telemetry activates', () => {
      useStore.setState({ videoSyncOffsetSeconds: 30, videoSyncWarning: 'some warning' })

      useStore.getState().loadVideoTelemetry(mp4Telemetry)

      const state = useStore.getState()
      expect(state.videoSyncOffsetSeconds).toBe(0)
      expect(state.videoSyncWarning).toBeNull()
    })

    test('sets activityFilename to null when telemetry activates', () => {
      useStore.setState({ activityFilename: 'old.fit' })

      useStore.getState().loadVideoTelemetry(mp4Telemetry)

      expect(useStore.getState().activityFilename).toBeNull()
    })

    test('updates activitySummary from telemetry payload', () => {
      useStore.getState().loadVideoTelemetry(mp4Telemetry)

      const summary = useStore.getState().activitySummary
      expect(summary).not.toBeNull()
      expect(summary.durationSeconds).toBe(120)
    })

    test('syncs the active timeline range to the telemetry duration', () => {
      useStore.getState().loadVideoTelemetry(shortMp4Telemetry)

      const state = useStore.getState()
      expect(state.dummyDurationSeconds).toBe(2)
      expect(state.startSecond).toBe(0)
      expect(state.endSecond).toBe(2)
      expect(state.selectedSecond).toBe(0)
      expect(state.config.scene.start).toBe(0)
      expect(state.config.scene.end).toBe(2)
    })
  })

  describe('activateActivityFile', () => {
    test('sets parsedActivity and parsedActivitySource', () => {
      useStore.getState().activateActivityFile(activityFile)

      const state = useStore.getState()
      expect(state.parsedActivity).toEqual(activityFile)
      expect(state.parsedActivitySource).toBe('activity-file')
    })

    test('moves active video telemetry into hiddenVideoParsedActivity', () => {
      useStore.getState().loadVideoTelemetry(mp4Telemetry)
      useStore.getState().activateActivityFile(activityFile)

      const state = useStore.getState()
      expect(state.parsedActivity).toEqual(activityFile)
      expect(state.parsedActivitySource).toBe('activity-file')
      expect(state.hiddenVideoParsedActivity).toEqual(mp4Telemetry)
    })

    test('updates activitySummary', () => {
      useStore.getState().activateActivityFile(activityFile)

      const summary = useStore.getState().activitySummary
      expect(summary).not.toBeNull()
      expect(summary.durationSeconds).toBe(3600)
    })
  })

  describe('clearActivityFile', () => {
    test('restores hidden video telemetry into parsedActivity', () => {
      useStore.getState().loadVideoTelemetry(mp4Telemetry)
      useStore.getState().activateActivityFile(activityFile)

      expect(useStore.getState().parsedActivitySource).toBe('activity-file')

      useStore.getState().clearActivityFile()

      const state = useStore.getState()
      expect(state.parsedActivity).toEqual(mp4Telemetry)
      expect(state.parsedActivitySource).toBe('video-telemetry')
      expect(state.hiddenVideoParsedActivity).toBeNull()
    })

    test('syncs the timeline range when hidden video telemetry is restored', () => {
      useStore.getState().loadVideoTelemetry(shortMp4Telemetry)
      useStore.getState().activateActivityFile(activityFile)
      useStore.setState((state) => ({
        config: {
          ...state.config,
          scene: {
            ...state.config.scene,
            start: 0,
            end: 3600,
          },
        },
        dummyDurationSeconds: 3600,
        endSecond: 3600,
        selectedSecond: 120,
        startSecond: 0,
      }))

      useStore.getState().clearActivityFile()

      const state = useStore.getState()
      expect(state.parsedActivity).toEqual(shortMp4Telemetry)
      expect(state.parsedActivitySource).toBe('video-telemetry')
      expect(state.dummyDurationSeconds).toBe(2)
      expect(state.endSecond).toBe(2)
      expect(state.selectedSecond).toBe(0)
      expect(state.config.scene.end).toBe(2)
    })

    test('clears everything when no hidden video telemetry exists', () => {
      useStore.getState().activateActivityFile(activityFile)

      useStore.getState().clearActivityFile()

      const state = useStore.getState()
      expect(state.parsedActivity).toBeNull()
      expect(state.parsedActivitySource).toBeNull()
      expect(state.activitySummary).toBeNull()
    })

    test('does not restore video telemetry when restoreVideoTelemetry is false', () => {
      useStore.getState().loadVideoTelemetry(mp4Telemetry)
      useStore.getState().activateActivityFile(activityFile)

      useStore.getState().clearActivityFile({ restoreVideoTelemetry: false })

      const state = useStore.getState()
      expect(state.parsedActivity).toBeNull()
      expect(state.parsedActivitySource).toBeNull()
      expect(state.hiddenVideoParsedActivity).toEqual(mp4Telemetry)
    })

    test('clears activityFilename by default', () => {
      useStore.setState({ activityFilename: 'ride.fit' })
      useStore.getState().activateActivityFile(activityFile)

      useStore.getState().clearActivityFile()

      expect(useStore.getState().activityFilename).toBeNull()
    })

    test('preserves activityFilename when clearFilename is false', () => {
      useStore.setState({ activityFilename: 'ride.fit' })
      useStore.getState().activateActivityFile(activityFile)

      useStore.getState().clearActivityFile({ clearFilename: false })

      expect(useStore.getState().activityFilename).toBe('ride.fit')
    })

    test('sets videoSyncOffsetSeconds to 0 and clears warning when restoring video telemetry', () => {
      useStore.setState({ videoSyncOffsetSeconds: 15, videoSyncWarning: 'old' })
      useStore.getState().loadVideoTelemetry(mp4Telemetry)
      useStore.getState().activateActivityFile(activityFile)

      useStore.getState().clearActivityFile()

      const state = useStore.getState()
      expect(state.videoSyncOffsetSeconds).toBe(0)
      expect(state.videoSyncWarning).toBeNull()
    })

    test('does nothing when parsedActivitySource is not activity-file', () => {
      useStore.getState().loadVideoTelemetry(mp4Telemetry)

      useStore.getState().clearActivityFile()

      const state = useStore.getState()
      expect(state.parsedActivity).toEqual(mp4Telemetry)
      expect(state.parsedActivitySource).toBe('video-telemetry')
    })
  })

  describe('clearVideoTelemetry', () => {
    test('clears parsedActivity when video telemetry is active', () => {
      useStore.getState().loadVideoTelemetry(mp4Telemetry)

      useStore.getState().clearVideoTelemetry()

      const state = useStore.getState()
      expect(state.parsedActivity).toBeNull()
      expect(state.parsedActivitySource).toBeNull()
      expect(state.activitySummary).toBeNull()
      expect(state.hiddenVideoParsedActivity).toBeNull()
    })

    test('leaves parsedActivity alone when activity file is active', () => {
      useStore.getState().loadVideoTelemetry(mp4Telemetry)
      useStore.getState().activateActivityFile(activityFile)

      useStore.getState().clearVideoTelemetry()

      const state = useStore.getState()
      expect(state.parsedActivity).toEqual(activityFile)
      expect(state.parsedActivitySource).toBe('activity-file')
      expect(state.hiddenVideoParsedActivity).toBeNull()
    })

    test('always clears hiddenVideoParsedActivity', () => {
      useStore.getState().loadVideoTelemetry(mp4Telemetry)
      useStore.getState().activateActivityFile(activityFile)

      expect(useStore.getState().hiddenVideoParsedActivity).toEqual(mp4Telemetry)

      useStore.getState().clearVideoTelemetry()

      expect(useStore.getState().hiddenVideoParsedActivity).toBeNull()
    })
  })

  describe('clearActivitySummary', () => {
    test('delegates to clearActivityFile with restoreVideoTelemetry false', () => {
      useStore.getState().loadVideoTelemetry(mp4Telemetry)
      useStore.getState().activateActivityFile(activityFile)

      useStore.getState().clearActivitySummary()

      const state = useStore.getState()
      expect(state.parsedActivity).toBeNull()
      expect(state.hiddenVideoParsedActivity).toEqual(mp4Telemetry)
    })
  })

  describe('syncVideoMetadataWithActiveActivity', () => {
    test('computes video sync when activity file is active', () => {
      const computeVideoSync = vi.fn()
      useStore.setState({
        computeVideoSync,
        parsedActivitySource: 'activity-file',
        activitySummary: { syncTime: '2024-01-01T00:00:00Z' },
      })

      useStore.getState().syncVideoMetadataWithActiveActivity()

      expect(computeVideoSync).toHaveBeenCalledWith({ syncTime: '2024-01-01T00:00:00Z' })
    })

    test('does not compute sync when video telemetry is active', () => {
      const computeVideoSync = vi.fn()
      useStore.setState({
        computeVideoSync,
        parsedActivitySource: 'video-telemetry',
        videoSyncOffsetSeconds: 5,
      })

      useStore.getState().syncVideoMetadataWithActiveActivity()

      expect(computeVideoSync).not.toHaveBeenCalled()
      expect(useStore.getState().videoSyncOffsetSeconds).toBe(0)
      expect(useStore.getState().videoSyncWarning).toBeNull()
    })

    test('does not compute sync when no activity is active', () => {
      const computeVideoSync = vi.fn()
      useStore.setState({ computeVideoSync, parsedActivitySource: null })

      useStore.getState().syncVideoMetadataWithActiveActivity()

      expect(computeVideoSync).not.toHaveBeenCalled()
    })
  })
})
