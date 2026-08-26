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

vi.mock('@/features/scene-settings/hooks/useAvailableFonts', () => ({
  default: () => ({ recommendedFonts: [], systemFonts: [] }),
}))

describe('useSceneSettingsState', () => {
  beforeEach(() => {
    useStore.setState(useStore.getInitialState(), true)
  })

  test('keeps temporary custom resolution and fps modes across unrelated scene updates', () => {
    act(() => {
      useStore.setState({ aspectRatio: '16:9', updateRate: 1 })
    })
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
    act(() => {
      useStore.setState({ aspectRatio: '16:9', updateRate: 1 })
    })
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
    act(() => {
      useStore.setState({ aspectRatio: '16:9', updateRate: 1, globalDefaults: { font_size: 42 } })
    })
    const onConfigChange = vi.fn()
    const config = { ...DEFAULT_CONFIG, scene: { ...DEFAULT_CONFIG.scene, font_size: 42 } }

    const { result } = renderHook(() => useSceneSettingsState({ config, onConfigChange }))

    expect(result.current.globalSettings.sceneStyleValue('font_size', 30)).toBe(42)
  })

  test('returns fallback when scene value is undefined', () => {
    act(() => {
      useStore.setState({ aspectRatio: '16:9', updateRate: 1 })
    })
    const onConfigChange = vi.fn()
    const config = { ...DEFAULT_CONFIG, scene: { ...DEFAULT_CONFIG.scene } }

    const { result } = renderHook(() => useSceneSettingsState({ config, onConfigChange }))

    expect(result.current.globalSettings.sceneStyleValue('undefined_key', 'fallback')).toBe('fallback')
  })
})

describe('handleAspectRatioChange', () => {
  test('selects a resolution preset when aspect ratio has presets', () => {
    act(() => {
      useStore.setState({ aspectRatio: '16:9', updateRate: 1 })
    })
    const config = { ...DEFAULT_CONFIG, scene: { ...DEFAULT_CONFIG.scene } }

    const { result } = renderHook(() => useSceneSettingsState({ config, onConfigChange: vi.fn() }))

    act(() => {
      result.current.handlers.handleAspectRatioChange('4:3')
    })

    const updatedConfig = useStore.getState().config
    expect(updatedConfig.scene.width).toBeDefined()
    expect(updatedConfig.scene.height).toBeDefined()
    expect(useStore.getState().aspectRatio).toBe('4:3')
  })

  test('does not change resolution when aspect ratio is custom', () => {
    act(() => {
      useStore.setState({ aspectRatio: '16:9', updateRate: 1 })
    })
    const config = { ...DEFAULT_CONFIG, scene: { ...DEFAULT_CONFIG.scene } }

    const { result } = renderHook(() => useSceneSettingsState({ config, onConfigChange: vi.fn() }))

    const initialWidth = useStore.getState().config.scene.width
    const initialHeight = useStore.getState().config.scene.height

    act(() => {
      result.current.handlers.handleAspectRatioChange('custom')
    })

    expect(useStore.getState().aspectRatio).toBe('custom')
    expect(useStore.getState().config.scene.width).toBe(initialWidth)
    expect(useStore.getState().config.scene.height).toBe(initialHeight)
  })
})

describe('handleFpsModeChange', () => {
  test('applies FPS preset value when selecting a standard FPS mode', () => {
    act(() => {
      useStore.setState({ aspectRatio: '16:9', updateRate: 1 })
    })
    const config = { ...DEFAULT_CONFIG, scene: { ...DEFAULT_CONFIG.scene } }

    const { result } = renderHook(() => useSceneSettingsState({ config, onConfigChange: vi.fn() }))

    act(() => {
      result.current.handlers.handleFpsModeChange('60')
    })

    expect(useStore.getState().config.scene.fps).toBe(60)
  })

  test('enters custom FPS mode without changing scene fps', () => {
    act(() => {
      useStore.setState({ aspectRatio: '16:9', updateRate: 1 })
    })
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
