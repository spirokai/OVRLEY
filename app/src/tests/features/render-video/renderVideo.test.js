/**
 * Regression tests for render-video preparation.
 *
 * These specs document that the render entry point materializes the same
 * render-effective scene values from committed template state after the
 * template-state seam owns render preparation.
 */

import { beforeEach, describe, expect, test, vi } from 'vitest'
import * as backend from '@/api/backend'
import useStore from '@/store/useStore'
import { DEFAULT_CONFIG } from '@/store/store-utils'
import renderVideo from '@/features/render-video/utils/render-video'

vi.mock('@/api/backend', () => ({
  renderVideo: vi.fn(),
}))

describe('renderVideo', () => {
  beforeEach(() => {
    useStore.setState(useStore.getInitialState(), true)
    useStore.setState({
      parsedActivity: {
        sample_elapsed_seconds: [0, 10, 20],
      },
    })
    vi.spyOn(console, 'error').mockImplementation(() => {})
    vi.mocked(backend.renderVideo).mockResolvedValue({
      started: true,
      render_id: 'render-1',
      outputPath: 'C:\\renders\\overlay.mov',
    })
    vi.spyOn(console, 'log').mockImplementation(() => {})
  })

  test('prepares a render-effective payload from committed template state', async () => {
    useStore.setState({
      config: {
        ...DEFAULT_CONFIG,
        scene: {
          ...DEFAULT_CONFIG.scene,
          fps: 60,
          updateRate: 3,
        },
        values: [{ id: 'value-1', value: 'speed', x: 10, y: 20 }],
      },
      globalDefaults: {
        ...useStore.getState().globalDefaults,
        color_values: '#abcdef',
      },
      updateRate: 6,
      exportCodec: 'prores_ks',
      exportRange: {
        type: 'custom',
        from: 5.25,
        to: 15.75,
      },
    })

    await renderVideo({ ...useStore.getState(), outputPath: 'C:\\renders\\overlay.mov', outputKind: 'transparent' })

    expect(backend.renderVideo).toHaveBeenCalledTimes(1)
    expect(backend.renderVideo).toHaveBeenCalledWith(
      expect.objectContaining({
        scene: expect.objectContaining({
          start: 5.25,
          end: 15.75,
          fps: 60,
          update_rate: 6,
          custom_export_range_active: true,
          ffmpeg: expect.objectContaining({
            codec: 'prores_ks',
            prores_profile: '4444',
            pix_fmt: 'yuva444p10le',
          }),
        }),
        values: [
          expect.objectContaining({
            id: 'value-1',
            color: '#abcdef',
          }),
        ],
      }),
      expect.objectContaining({
        sample_elapsed_seconds: [0, 10, 20],
      }),
      expect.objectContaining({ outputPath: 'C:\\renders\\overlay.mov', outputKind: 'transparent', overwrite: false }),
    )
    expect(vi.mocked(backend.renderVideo).mock.calls.at(-1)?.[0]?.scene).not.toHaveProperty('updateRate')
  })

  test('uses editor timeline bounds when hydrated template config omitted scene timing', async () => {
    useStore.setState({
      config: {
        ...DEFAULT_CONFIG,
        scene: {
          width: DEFAULT_CONFIG.scene.width,
          height: DEFAULT_CONFIG.scene.height,
          fps: 30,
        },
      },
      startSecond: 8,
      endSecond: 42,
      exportRange: {
        type: 'full',
        from: 0,
        to: 0,
      },
    })

    await renderVideo({ ...useStore.getState(), outputPath: 'C:\\renders\\overlay.mov', outputKind: 'transparent' })

    expect(backend.renderVideo).toHaveBeenCalledWith(
      expect.objectContaining({
        scene: expect.objectContaining({
          start: 8,
          end: 42,
        }),
      }),
      expect.any(Object),
      expect.objectContaining({ outputPath: 'C:\\renders\\overlay.mov', outputKind: 'transparent', overwrite: false }),
    )
  })

  test('clamps a composite custom range to the imported video when submitting the job', async () => {
    const state = useStore.getState()
    const compositeOverrides = {
      ...state,
      exportMode: 'composite',
      exportCodec: 'libx264',
      exportRange: {
        type: 'custom',
        from: 5.25,
        to: 39.75,
      },
      importedVideoPath: 'C:\\clip.mp4',
      importedVideoDuration: 30,
      importedVideoFps: 30,
      importedVideoFpsNum: 30,
      importedVideoFpsDen: 1,
      importedVideoResolution: { width: 1920, height: 1080 },
      videoSyncOffsetSeconds: 10,
      outputPath: 'C:\\renders\\video.mp4',
      outputKind: 'composite',
    }

    await renderVideo(compositeOverrides)

    expect(backend.renderVideo).toHaveBeenCalledWith(
      expect.objectContaining({
        scene: expect.objectContaining({
          start: 10,
          end: 39.75,
          composite_sync_offset: 10,
          composite_video_trim_start: 0,
          composite_render_duration: 29.75,
        }),
      }),
      expect.any(Object),
      expect.objectContaining({ outputPath: 'C:\\renders\\video.mp4', outputKind: 'composite', overwrite: false }),
    )

    const submittedJobs = vi.mocked(backend.renderVideo).mock.calls.length
    await expect(
      renderVideo({
        ...compositeOverrides,
        exportRange: { type: 'custom', from: 0, to: 5 },
        outputPath: 'C:\\renders\\video.mp4',
        outputKind: 'composite',
      }),
    ).rejects.toThrow('Custom export range must overlap the imported video range')
    expect(backend.renderVideo).toHaveBeenCalledTimes(submittedJobs)
  })
})
