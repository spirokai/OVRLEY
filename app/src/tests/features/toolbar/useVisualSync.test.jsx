import { act, renderHook } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { useVisualSync } from '@/features/toolbar/hooks/useVisualSync'
import * as backend from '@/api/backend'

const mock = vi.hoisted(() => ({ state: null, listener: null, unsubscribe: vi.fn(), request: null }))
vi.mock('react-i18next', () => ({ useTranslation: () => ({ t: (key) => key }) }))
vi.mock('@/hooks/useAppStoreSelectors', () => ({ useVisualSyncStore: () => mock.state }))
vi.mock('@/store/useStore', () => ({ default: { getState: () => mock.state } }))
vi.mock('@/api/backend', () => ({
  subscribeVisualSync: vi.fn(async (listener) => {
    mock.listener = listener
    return mock.unsubscribe
  }),
  startVisualSync: vi.fn(),
  getVisualSyncStatus: vi.fn(),
  cancelVisualSync: vi.fn(async () => {}),
}))

function snapshot(sequence = 0, terminal = null) {
  return { job_id: 'job-1', inputs: mock.request.inputs, sequence, stage: 'analyzing', analyzed_seconds: null, terminal }
}

function matched() {
  const candidate = {
    offset_seconds: 7200,
    accepted: true,
    correlation: 0.75,
    nomination_correlation: 0.74,
    nomination_margin: 0.3,
    observed_seconds: 437.8,
    observed_fraction: 0.47,
    retained_video_observation_fraction: 1,
    sections: [],
    rejection_reasons: [],
  }
  const result = {
    accepted: true,
    candidates: [candidate, { ...candidate, offset_seconds: 100, accepted: false, rejection_reasons: ['weak_turning_agreement'] }],
  }
  return {
    kind: 'result',
    result,
  }
}

beforeEach(() => {
  vi.clearAllMocks()
  mock.state = {
    parsedActivity: {},
    activitySource: { path: 'activity.fit' },
    parsedActivitySource: 'activity-file',
    importedVideoPath: 'clip.mp4',
    importedVideoImportId: 'import-1',
    setVideoSyncOffset: vi.fn(),
  }
  backend.startVisualSync.mockImplementation(async (request) => {
    mock.request = request
    return snapshot()
  })
  backend.getVisualSyncStatus.mockImplementation(async () => snapshot())
})

describe('visual synchronization lifecycle', () => {
  it('handles backend cancellation without a match result', async () => {
    backend.getVisualSyncStatus.mockImplementation(async () => snapshot(2, { kind: 'cancelled' }))
    const { result } = renderHook(() => useVisualSync())
    await act(() => result.current.start())
    expect(result.current.status).toBe('cancelled')
    expect(result.current.busy).toBe(false)
    expect(result.current.candidates).toEqual([])
    expect(mock.unsubscribe).toHaveBeenCalled()
    expect(mock.state.setVideoSyncOffset).not.toHaveBeenCalled()
  })
  it('subscribes before starting and recovers a completion delivered before the start response', async () => {
    backend.startVisualSync.mockImplementation(async (request) => {
      expect(backend.subscribeVisualSync).toHaveBeenCalledOnce()
      mock.request = request
      mock.listener(snapshot(2, matched()))
      return snapshot()
    })
    backend.getVisualSyncStatus.mockImplementation(async () => snapshot(2, matched()))
    const { result } = renderHook(() => useVisualSync())
    await act(() => result.current.start())
    expect(result.current.status).toBe('matched')
    expect(mock.state.setVideoSyncOffset).not.toHaveBeenCalled()
    expect(mock.request).not.toHaveProperty('matching_settings')
    expect(result.current.candidates).toHaveLength(2)
    act(() => result.current.apply(100))
    expect(mock.state.setVideoSyncOffset).not.toHaveBeenCalled()
    act(() => result.current.apply(7200))
    expect(mock.state.setVideoSyncOffset).toHaveBeenCalledWith(7200)
    expect(mock.unsubscribe).toHaveBeenCalled()
  })

  it('rejects application immediately after activity replacement, before the next render', async () => {
    backend.getVisualSyncStatus.mockImplementation(async () => snapshot(2, matched()))
    const { result, rerender } = renderHook(() => useVisualSync())
    await act(() => result.current.start())
    mock.state = { ...mock.state, parsedActivity: {} }
    act(() => result.current.apply(7200))
    expect(mock.state.setVideoSyncOffset).not.toHaveBeenCalled()
    rerender()
    expect(result.current.status).toBe('idle')
  })

  it('cancels on video replacement and ignores old events', async () => {
    const { result, rerender } = renderHook(() => useVisualSync())
    await act(() => result.current.start())
    const oldSnapshot = snapshot(3, matched())
    mock.state = { ...mock.state, importedVideoImportId: 'import-2' }
    rerender()
    expect(backend.cancelVisualSync).toHaveBeenCalledWith('job-1')
    act(() => mock.listener(oldSnapshot))
    expect(result.current.status).toBe('idle')
    expect(mock.state.setVideoSyncOffset).not.toHaveBeenCalled()
  })

  it('cancels a job whose start response arrives after cancellation', async () => {
    let resolveStart
    backend.startVisualSync.mockImplementation((request) => {
      mock.request = request
      return new Promise((resolve) => {
        resolveStart = resolve
      })
    })
    const { result } = renderHook(() => useVisualSync())
    let starting
    await act(async () => {
      starting = result.current.start()
      await Promise.resolve()
    })
    await act(() => result.current.cancel())
    await act(async () => {
      resolveStart(snapshot())
      await starting
    })
    expect(backend.cancelVisualSync).toHaveBeenCalledWith('job-1')
    expect(result.current.status).toBe('cancelled')
  })

  it('ignores older snapshots and leaves no-match diagnostic offsets unapplied', async () => {
    const { result, unmount } = renderHook(() => useVisualSync())
    await act(() => result.current.start())
    act(() => mock.listener({ ...snapshot(3), stage: 'matching' }))
    act(() => mock.listener(snapshot(1)))
    expect(result.current.status).toBe('matching')
    act(() =>
      mock.listener(
        snapshot(4, {
          kind: 'result',
          result: {
            accepted: false,
            candidates: matched().result.candidates,
          },
        }),
      ),
    )
    act(() => result.current.apply(7200))
    expect(mock.state.setVideoSyncOffset).not.toHaveBeenCalled()
    expect(result.current.busy).toBe(false)
    unmount()
  })
})
