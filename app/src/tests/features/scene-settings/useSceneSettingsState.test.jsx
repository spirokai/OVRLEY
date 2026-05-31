/**
 * Behavior tests for the scene settings container hook.
 *
 * These specs focus on the public state and handlers exposed to the sidebar.
 * The important contract is that temporary custom-mode UI stays visible until
 * the relevant committed scene values actually change.
 */

import { act, renderHook } from '@testing-library/react'
import { beforeEach, describe, expect, test, vi } from 'vitest'
import useSceneSettingsState from '@/features/scene-settings/hooks/useSceneSettingsState'
import useStore from '@/store/useStore'
import { DEFAULT_CONFIG } from '@/store/store-utils'

vi.mock('@/api/backend', () => ({
  listAvailableFonts: vi.fn().mockResolvedValue({
    recommendedFonts: [],
    systemFonts: [],
  }),
}))

describe('useSceneSettingsState', () => {
  beforeEach(() => {
    useStore.setState(useStore.getInitialState(), true)
  })

  test('keeps temporary custom resolution and fps modes across unrelated scene updates', () => {
    useStore.setState({ aspectRatio: '16:9', updateRate: 1 })
    const onConfigChange = vi.fn()
    const initialConfig = { ...DEFAULT_CONFIG, scene: { ...DEFAULT_CONFIG.scene } }

    const { result, rerender } = renderHook(({ config }) => useSceneSettingsState({ config, onConfigChange }), {
      initialProps: { config: initialConfig },
    })

    act(() => {
      result.current.handlers.handleResolutionChange('custom')
      result.current.handlers.handleFpsModeChange('custom')
    })

    expect(result.current.overlaySettings.resId).toBe('custom')
    expect(result.current.overlaySettings.fpsMode).toBe('custom')

    rerender({ config: { ...initialConfig, scene: { ...initialConfig.scene, start: 12 } } })

    expect(result.current.overlaySettings.resId).toBe('custom')
    expect(result.current.overlaySettings.fpsMode).toBe('custom')
  })

  test('returns to committed preset modes when resolution or FPS actually changes', () => {
    useStore.setState({ aspectRatio: '16:9', updateRate: 1 })
    const onConfigChange = vi.fn()
    const initialConfig = { ...DEFAULT_CONFIG, scene: { ...DEFAULT_CONFIG.scene } }

    const { result, rerender } = renderHook(({ config }) => useSceneSettingsState({ config, onConfigChange }), {
      initialProps: { config: initialConfig },
    })

    act(() => {
      result.current.handlers.handleResolutionChange('custom')
      result.current.handlers.handleFpsModeChange('custom')
    })

    rerender({ config: { ...initialConfig, scene: { ...initialConfig.scene, width: 1280, height: 720, fps: 60 } } })

    expect(result.current.overlaySettings.resId).toBe('720p')
    expect(result.current.overlaySettings.fpsMode).toBe('60')
  })
})

describe('sceneStyleValue', () => {
  test('returns scene value when defined', () => {
    useStore.setState({ aspectRatio: '16:9', updateRate: 1 })
    const onConfigChange = vi.fn()
    const config = { ...DEFAULT_CONFIG, scene: { ...DEFAULT_CONFIG.scene, font_size: 42 } }

    const { result } = renderHook(() => useSceneSettingsState({ config, onConfigChange }))

    expect(result.current.globalSettings.sceneStyleValue('font_size', 30)).toBe(42)
  })

  test('returns fallback when scene value is undefined', () => {
    useStore.setState({ aspectRatio: '16:9', updateRate: 1 })
    const onConfigChange = vi.fn()
    const config = { ...DEFAULT_CONFIG, scene: { ...DEFAULT_CONFIG.scene } }

    const { result } = renderHook(() => useSceneSettingsState({ config, onConfigChange }))

    expect(result.current.globalSettings.sceneStyleValue('undefined_key', 'fallback')).toBe('fallback')
  })
})

describe('handleAspectRatioChange', () => {
  test('selects a resolution preset when aspect ratio has presets', () => {
    useStore.setState({ aspectRatio: '16:9', updateRate: 1 })
    const onConfigChange = vi.fn()
    const config = { ...DEFAULT_CONFIG, scene: { ...DEFAULT_CONFIG.scene } }

    const { result } = renderHook(() => useSceneSettingsState({ config, onConfigChange }))

    act(() => {
      result.current.handlers.handleAspectRatioChange('4:3')
    })

    expect(onConfigChange).toHaveBeenCalled()
    const updatedConfig = onConfigChange.mock.calls[0][0]
    expect(updatedConfig.scene.width).toBeDefined()
    expect(updatedConfig.scene.height).toBeDefined()
  })

  test('does not change resolution when aspect ratio is custom', () => {
    useStore.setState({ aspectRatio: '16:9', updateRate: 1 })
    const onConfigChange = vi.fn()
    const config = { ...DEFAULT_CONFIG, scene: { ...DEFAULT_CONFIG.scene } }

    const { result } = renderHook(() => useSceneSettingsState({ config, onConfigChange }))

    act(() => {
      result.current.handlers.handleAspectRatioChange('custom')
    })

    expect(onConfigChange).not.toHaveBeenCalled()
  })
})

describe('handleFpsModeChange', () => {
  test('applies FPS preset value when selecting a standard FPS mode', () => {
    useStore.setState({ aspectRatio: '16:9', updateRate: 1 })
    const onConfigChange = vi.fn()
    const config = { ...DEFAULT_CONFIG, scene: { ...DEFAULT_CONFIG.scene } }

    const { result } = renderHook(() => useSceneSettingsState({ config, onConfigChange }))

    act(() => {
      result.current.handlers.handleFpsModeChange('60')
    })

    expect(onConfigChange).toHaveBeenCalled()
    const updated = onConfigChange.mock.calls[0][0]
    expect(updated.scene.fps).toBe(60)
  })

  test('enters custom FPS mode without changing scene fps', () => {
    useStore.setState({ aspectRatio: '16:9', updateRate: 1 })
    const onConfigChange = vi.fn()
    const config = { ...DEFAULT_CONFIG, scene: { ...DEFAULT_CONFIG.scene, fps: 30 } }

    const { result } = renderHook(() => useSceneSettingsState({ config, onConfigChange }))

    act(() => {
      result.current.handlers.handleFpsModeChange('custom')
    })

    expect(result.current.overlaySettings.fpsMode).toBe('custom')
    expect(config.scene.fps).toBe(30)
  })
})

describe('handleOffsetBlur', () => {
  test('parses and rounds a simple time string', () => {
    useStore.setState({ aspectRatio: '16:9', updateRate: 1 })
    const onConfigChange = vi.fn()
    const config = { ...DEFAULT_CONFIG, scene: { ...DEFAULT_CONFIG.scene } }

    const { result } = renderHook(() => useSceneSettingsState({ config, onConfigChange }))

    act(() => {
      result.current.handlers.handleOffsetBlur('1:30')
    })

    expect(useStore.getState().videoSyncOffsetSeconds).toBe(90)
    expect(result.current.videoSyncSettings.offsetInput).toBe('90')
  })

  test('parses colon-delimited time (H:MM:SS)', () => {
    useStore.setState({ aspectRatio: '16:9', updateRate: 1 })
    const onConfigChange = vi.fn()
    const config = { ...DEFAULT_CONFIG, scene: { ...DEFAULT_CONFIG.scene } }

    const { result } = renderHook(() => useSceneSettingsState({ config, onConfigChange }))

    act(() => {
      result.current.handlers.handleOffsetBlur('1:00:30')
    })

    expect(useStore.getState().videoSyncOffsetSeconds).toBe(3630)
  })

  test('parses decimal seconds and rounds to 1 decimal', () => {
    useStore.setState({ aspectRatio: '16:9', updateRate: 1 })
    const onConfigChange = vi.fn()
    const config = { ...DEFAULT_CONFIG, scene: { ...DEFAULT_CONFIG.scene } }

    const { result } = renderHook(() => useSceneSettingsState({ config, onConfigChange }))

    act(() => {
      result.current.handlers.handleOffsetBlur('5.55')
    })

    expect(useStore.getState().videoSyncOffsetSeconds).toBe(5.6)
  })
})
