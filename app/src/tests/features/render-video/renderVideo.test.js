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
    vi.mocked(backend.renderVideo).mockResolvedValue({
      started: true,
      render_id: 'render-1',
    })
  })

  test('prepares a render-effective payload from committed template state', async () => {
    useStore.setState({
      config: {
        ...DEFAULT_CONFIG,
        scene: {
          ...DEFAULT_CONFIG.scene,
          fps: 60,
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
        from: 0,
        to: 0,
        fromTime: '00:00:05',
        toTime: '00:00:15',
      },
    })

    await renderVideo(useStore.getState())

    expect(backend.renderVideo).toHaveBeenCalledTimes(1)
    expect(backend.renderVideo).toHaveBeenCalledWith(
      expect.objectContaining({
        scene: expect.objectContaining({
          start: 5,
          end: 15,
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
    )
  })
})
