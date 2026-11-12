/**
 * Behavior tests for the render video dialog container hook.
 *
 * The dialog exposes transient UI state to the presentational component. These
 * specs document that temporary custom-mode UI survives unrelated settings
 * updates while still tracking committed FPS changes when they actually change.
 */

import { act, renderHook, waitFor } from '@testing-library/react'
import { useState } from 'react'
import { beforeEach, describe, expect, test, vi } from 'vite-plus/test'
import useRenderVideoDialogState from '@/features/render-video/hooks/useRenderVideoDialogState'
import { DEFAULT_EXPORT_RANGE } from '@/features/template-manager'
import useStore from '@/store/useStore'
import { DEFAULT_CONFIG, DEFAULT_RENDER_PROGRESS } from '@/store/store-utils'
import { createTestVideo, videoStoreState } from '@/tests/helpers/videoFixtures'

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
        libx265: true,
      },
      renderProgress: { ...DEFAULT_RENDER_PROGRESS },
    })
  })

  test('keeps temporary custom FPS mode across unrelated settings updates', () => {
    const onSettingsChange = vi.fn()
    const onClose = vi.fn()
    const onConfirm = vi.fn()
    const initialSettings = {
      outputMode: 'combined',
      exportMode: 'transparent',
      fps: 30,
      widgetUpdateRate: 1,
      codec: 'prores_ks',
      qualityType: 'quality',
      qualityValue: 18,
      exportAcceleration: 'cpu',
      range: { ...DEFAULT_EXPORT_RANGE },
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

    expect(result.current.exportMode).toBe('transparent')

    act(() => {
      result.current.handleFpsModeChange('custom')
    })

    expect(result.current.fpsMode).toBe('custom')

    rerender({
      settings: {
        ...initialSettings,
        range: {
          ...initialSettings.range,
          type: 'custom',
        },
      },
    })

    expect(result.current.fpsMode).toBe('custom')
  })

  test('switches imported-video dialogs between composite and transparent defaults while preserving later range edits', async () => {
    useStore.setState(videoStoreState([createTestVideo({ id: 'clip', path: 'C:\\video.mp4', durationSeconds: 12 })], { clip: 5 }))

    const { result } = renderHook(() => {
      const [settings, setSettings] = useState({
        outputMode: 'combined',
        exportMode: 'composite',
        fps: 30,
        widgetUpdateRate: 1,
        codec: 'prores_ks',
        qualityType: 'quality',
        qualityValue: 18,
        exportAcceleration: 'cpu',
        range: { ...DEFAULT_EXPORT_RANGE },
      })

      return useRenderVideoDialogState({
        phase: 'confirm',
        settings,
        onSettingsChange: (updates) => setSettings((current) => ({ ...current, ...updates })),
        onClose: vi.fn(),
        onConfirm: vi.fn(),
      })
    })

    await waitFor(() => {
      expect(result.current.settings.codec).toBe('libx264')
    })
    expect(result.current.exportMode).toBe('composite')
    expect(result.current.settings.qualityType).toBe('quality')
    expect(result.current.settings.qualityValue).toBe(21)
    act(() => result.current.handleOutputFormatChange('hevc'))
    expect(result.current.settings.qualityValue).toBe(26)
    act(() => result.current.handleOutputFormatChange('h264'))
    expect(result.current.settings.qualityValue).toBe(21)

    act(() => {
      result.current.handleExportModeChange('transparent')
    })

    await waitFor(() => {
      expect(result.current.settings.codec).toBe('prores_ks')
    })
    expect(result.current.exportMode).toBe('transparent')
    // The video parts decide what renders, so the range is left as it was.
    expect(result.current.settings.range).toEqual({ ...DEFAULT_EXPORT_RANGE })
    expect(result.current.showExportRangeSettings).toBe(false)
  })

  test('renders the whole activity in one transparent render only when asked, and turns it off for composite export', async () => {
    useStore.setState({
      ...videoStoreState([createTestVideo({ id: 'clip', path: 'C:\\video.mp4', durationSeconds: 12 })], { clip: 5 }),
      parsedActivity: { metadata: {} },
    })
    const onConfirm = vi.fn()
    const { result } = renderHook(() => {
      const [settings, setSettings] = useState({
        outputMode: 'combined',
        exportMode: 'transparent',
        fps: 30,
        widgetUpdateRate: 1,
        codec: 'prores_ks',
        qualityType: 'quality',
        qualityValue: 18,
        exportAcceleration: 'cpu',
        range: { ...DEFAULT_EXPORT_RANGE },
        renderFullActivity: false,
        outputPath: 'C:\\renders\\overlay.mov',
      })
      return useRenderVideoDialogState({
        phase: 'confirm',
        settings,
        onSettingsChange: (updates) => setSettings((current) => ({ ...current, ...updates })),
        onClose: vi.fn(),
        onConfirm,
      })
    })

    expect(result.current.showRenderFullActivity).toBe(true)
    expect(result.current.renderFullActivity).toBe(false)
    act(() => result.current.handleRenderFullActivityChange(true))
    expect(result.current.renderFullActivity).toBe(true)
    await act(async () => result.current.onConfirm())
    expect(onConfirm).toHaveBeenCalledWith(null)

    act(() => result.current.handleExportModeChange('composite'))
    expect(result.current.renderFullActivity).toBe(false)
    expect(result.current.showRenderFullActivity).toBe(false)
  })
})
