import { act, renderHook } from '@testing-library/react'
import { beforeEach, describe, expect, test } from 'vitest'
import { useTransitionOverlay } from '@/features/overlay-editor/hooks/useTransitionOverlay'
import { buildTimelineProgram } from '@/lib/video-program/timeline'
import { listTransitionProxyJobs } from '@/lib/video-program/transition-proxies'
import useStore from '@/store/useStore'
import { createTestVideo, videoStoreState } from '@/tests/helpers/videoFixtures'

const telemetry = (seconds) => ({
  metadata: { duration_seconds: seconds },
  sample_elapsed_seconds: [0, seconds],
  valid_attributes: [],
  extended_attributes: [],
})

function markProxyReady() {
  const { importedVideos, videoEdits, transitionDefaults } = useStore.getState()
  const { clips, program } = buildTimelineProgram({ videos: importedVideos, videoEdits, dragPreview: null, transitionDefaults })
  const [job] = listTransitionProxyJobs(program, clips)
  useStore.getState().setTransitionProxy(job.key, { status: 'ready', importId: 'proxy', url: 'http://127.0.0.1:4321/video/proxy' })
}

describe('useTransitionOverlay', () => {
  beforeEach(() => {
    useStore.setState(useStore.getInitialState(), true)
    useStore.setState({
      ...videoStoreState(
        [
          createTestVideo({ id: 'a', durationSeconds: 30, telemetry: { status: 'ready', activity: telemetry(30) } }),
          createTestVideo({
            id: 'b',
            path: '/videos/b.mp4',
            durationSeconds: 30,
            importOrder: 1,
            telemetry: { status: 'ready', activity: telemetry(30) },
          }),
        ],
        { a: 0, b: 40 },
      ),
      transitionDefaults: { type: 'fade', durationSeconds: 2, splitGapThresholdSeconds: 2, dialogShown: true },
      selectedSecond: 29,
    })
  })

  test('shows the incoming clip at its own telemetry time once the proxy is ready', () => {
    const { result } = renderHook(() => useTransitionOverlay())
    // Without a ready proxy the preview cuts.
    expect(result.current).toBeNull()
    act(() => markProxyReady())
    expect(result.current).toMatchObject({
      progress: 0.5,
      activity: useStore.getState().importedVideos[1].telemetry.activity,
      previewSecond: 1,
      hasActivityCoverage: true,
      exportRange: { type: 'custom', from: 0, to: 30 },
    })
    act(() => useStore.setState({ selectedSecond: 20 }))
    expect(result.current).toBeNull()
  })
})
