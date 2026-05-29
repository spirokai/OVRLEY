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
  listAvailableFonts: vi.fn().mockResolvedValue([]),
}))

describe('useSceneSettingsState', () => {
  beforeEach(() => {
    useStore.setState(useStore.getInitialState(), true)
  })

  test('keeps temporary custom resolution and fps modes across unrelated scene updates', () => {
    useStore.setState({
      aspectRatio: '16:9',
      updateRate: 1,
    })

    const onConfigChange = vi.fn()
    const initialConfig = {
      ...DEFAULT_CONFIG,
      scene: {
        ...DEFAULT_CONFIG.scene,
      },
    }

    const { result, rerender } = renderHook(({ config }) => useSceneSettingsState({ config, onConfigChange }), {
      initialProps: { config: initialConfig },
    })

    act(() => {
      result.current.handleResolutionChange('custom')
      result.current.handleFpsModeChange('custom')
    })

    expect(result.current.resId).toBe('custom')
    expect(result.current.fpsMode).toBe('custom')

    rerender({
      config: {
        ...initialConfig,
        scene: {
          ...initialConfig.scene,
          start: 12,
        },
      },
    })

    expect(result.current.resId).toBe('custom')
    expect(result.current.fpsMode).toBe('custom')
  })

  test('returns to committed preset modes when resolution or FPS actually changes', () => {
    useStore.setState({
      aspectRatio: '16:9',
      updateRate: 1,
    })

    const onConfigChange = vi.fn()
    const initialConfig = {
      ...DEFAULT_CONFIG,
      scene: {
        ...DEFAULT_CONFIG.scene,
      },
    }

    const { result, rerender } = renderHook(({ config }) => useSceneSettingsState({ config, onConfigChange }), {
      initialProps: { config: initialConfig },
    })

    act(() => {
      result.current.handleResolutionChange('custom')
      result.current.handleFpsModeChange('custom')
    })

    rerender({
      config: {
        ...initialConfig,
        scene: {
          ...initialConfig.scene,
          width: 1280,
          height: 720,
          fps: 60,
        },
      },
    })

    expect(result.current.resId).toBe('720p')
    expect(result.current.fpsMode).toBe('60')
  })
})
