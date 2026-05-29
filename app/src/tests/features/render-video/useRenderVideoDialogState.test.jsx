/**
 * Behavior tests for the render video dialog container hook.
 *
 * The dialog exposes transient UI state to the presentational component. These
 * specs document that temporary custom-mode UI survives unrelated settings
 * updates while still tracking committed FPS changes when they actually change.
 */

import { act, renderHook, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, test, vi } from 'vitest'
import useRenderVideoDialogState from '@/features/render-video/hooks/useRenderVideoDialogState'
import { DEFAULT_EXPORT_RANGE } from '@/features/template-manager'
import useStore from '@/store/useStore'
import { DEFAULT_CONFIG, DEFAULT_RENDER_PROGRESS } from '@/store/store-utils'

describe('useRenderVideoDialogState', () => {
  beforeEach(() => {
    useStore.setState(useStore.getInitialState(), true)
    useStore.setState({
      config: {
        ...DEFAULT_CONFIG,
        scene: {
          ...DEFAULT_CONFIG.scene,
        },
      },
      platformOs: 'windows',
      availableCodecs: {
        proresKs: true,
        libx264: true,
      },
      renderProgress: { ...DEFAULT_RENDER_PROGRESS },
    })
  })

  test('keeps temporary custom FPS mode across unrelated settings updates', () => {
    const onSettingsChange = vi.fn()
    const onClose = vi.fn()
    const onConfirm = vi.fn()
    const initialSettings = {
      fps: 30,
      updateRate: 1,
      exportCodec: 'prores_ks',
      exportAcceleration: 'cpu',
      exportRange: { ...DEFAULT_EXPORT_RANGE },
    }

    const { result, rerender } = renderHook(
      ({ settings }) =>
        useRenderVideoDialogState({
          phase: 'confirm',
          settings,
          onSettingsChange,
          onClose,
          onConfirm,
        }),
      {
        initialProps: {
          settings: initialSettings,
        },
      },
    )

    act(() => {
      result.current.handleFpsModeChange('custom')
    })

    expect(result.current.fpsMode).toBe('custom')

    rerender({
      settings: {
        ...initialSettings,
        exportRange: {
          ...initialSettings.exportRange,
          type: 'custom',
        },
      },
    })

    expect(result.current.fpsMode).toBe('custom')
  })

  test('switches imported-video renders to the first available MP4 codec', async () => {
    useStore.setState({
      importedVideoPath: 'C:\\video.mp4',
      importedVideoFps: 30,
      importedVideoResolution: { width: 1920, height: 1080 },
      availableCodecs: {
        proresKs: true,
        libx264: true,
      },
    })

    const onSettingsChange = vi.fn()
    const settings = {
      fps: 30,
      updateRate: 1,
      exportCodec: 'prores_ks',
      exportAcceleration: 'cpu',
      exportRange: { ...DEFAULT_EXPORT_RANGE },
    }

    renderHook(() =>
      useRenderVideoDialogState({
        phase: 'confirm',
        settings,
        onSettingsChange,
        onClose: vi.fn(),
        onConfirm: vi.fn(),
      }),
    )

    await waitFor(() => {
      expect(onSettingsChange).toHaveBeenCalledWith(
        expect.objectContaining({
          exportCodec: 'libx264',
          exportAcceleration: 'cpu',
        }),
      )
    })

    expect(onSettingsChange.mock.calls.at(-1)?.[0]?.exportBitrate).toBeGreaterThan(0)
  })
})
