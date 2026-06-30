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

function resetStore() {
  useStore.setState(useStore.getInitialState(), true)
}

describe('video import — telemetry lifecycle', () => {
  beforeEach(resetStore)

  test('re-importing video clears previous telemetry before extraction', () => {
    useStore.getState().loadVideoTelemetry(mp4Telemetry)
    expect(useStore.getState().parsedActivity).toEqual(mp4Telemetry)

    useStore.getState().clearVideoTelemetry()
    expect(useStore.getState().parsedActivity).toBeNull()
    expect(useStore.getState().parsedActivitySource).toBeNull()
  })

  test('importing video while activity file is active stores telemetry as hidden', () => {
    useStore.getState().activateActivityFile(activityFile)
    useStore.getState().loadVideoTelemetry(mp4Telemetry)

    const state = useStore.getState()
    expect(state.parsedActivity).toEqual(activityFile)
    expect(state.parsedActivitySource).toBe('activity-file')
    expect(state.hiddenVideoParsedActivity).toEqual(mp4Telemetry)
  })

  test('video telemetry activation does not run external activity sync', () => {
    const computeVideoSync = vi.fn()
    useStore.setState({ computeVideoSync })

    useStore.getState().loadVideoTelemetry(mp4Telemetry)

    expect(computeVideoSync).not.toHaveBeenCalled()
    expect(useStore.getState().videoSyncOffsetSeconds).toBe(0)
    expect(useStore.getState().videoSyncWarning).toBeNull()
  })

  test('importing video clears previous telemetry even when same path', () => {
    useStore.setState({ importedVideoPath: '/videos/ride.mp4' })
    useStore.getState().loadVideoTelemetry(mp4Telemetry)
    expect(useStore.getState().parsedActivity).toEqual(mp4Telemetry)

    useStore.getState().clearVideoTelemetry()
    expect(useStore.getState().parsedActivity).toBeNull()
  })
})

describe('activity import — overwrites MP4 telemetry', () => {
  beforeEach(resetStore)

  test('activity import overwrites active video telemetry', () => {
    useStore.getState().loadVideoTelemetry(mp4Telemetry)
    expect(useStore.getState().parsedActivitySource).toBe('video-telemetry')

    useStore.getState().activateActivityFile(activityFile)

    const state = useStore.getState()
    expect(state.parsedActivity).toEqual(activityFile)
    expect(state.parsedActivitySource).toBe('activity-file')
    expect(state.hiddenVideoParsedActivity).toEqual(mp4Telemetry)
  })

  test('clearing activity before parsing does not briefly restore video telemetry', () => {
    useStore.getState().loadVideoTelemetry(mp4Telemetry)
    useStore.getState().activateActivityFile(activityFile)

    useStore.getState().clearActivityFile({ restoreVideoTelemetry: false, clearFilename: false })

    const state = useStore.getState()
    expect(state.parsedActivity).toBeNull()
    expect(state.hiddenVideoParsedActivity).toEqual(mp4Telemetry)
  })
})

describe('activity clear — restores video telemetry', () => {
  beforeEach(resetStore)

  test('clearing external activity restores MP4 telemetry as parsedActivity', () => {
    useStore.getState().loadVideoTelemetry(mp4Telemetry)
    useStore.getState().activateActivityFile(activityFile)

    useStore.getState().clearActivityFile()

    const state = useStore.getState()
    expect(state.parsedActivity).toEqual(mp4Telemetry)
    expect(state.parsedActivitySource).toBe('video-telemetry')
    expect(state.hiddenVideoParsedActivity).toBeNull()
    expect(state.videoSyncOffsetSeconds).toBe(0)
    expect(state.videoSyncWarning).toBeNull()
  })

  test('clearing video removes MP4 telemetry and clears parsedActivity', () => {
    useStore.getState().loadVideoTelemetry(mp4Telemetry)

    useStore.getState().clearVideoTelemetry()

    const state = useStore.getState()
    expect(state.parsedActivity).toBeNull()
    expect(state.parsedActivitySource).toBeNull()
    expect(state.hiddenVideoParsedActivity).toBeNull()
  })

  test('clearing video does not disturb active external activity', () => {
    useStore.getState().loadVideoTelemetry(mp4Telemetry)
    useStore.getState().activateActivityFile(activityFile)

    useStore.getState().clearVideoTelemetry()

    const state = useStore.getState()
    expect(state.parsedActivity).toEqual(activityFile)
    expect(state.parsedActivitySource).toBe('activity-file')
  })
})
