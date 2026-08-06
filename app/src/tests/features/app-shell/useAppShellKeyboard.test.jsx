import { act, renderHook } from '@testing-library/react'
import { describe, expect, test, vi } from 'vitest'
import useAppShellKeyboard from '@/features/app-shell/hooks/useAppShellKeyboard'

function renderShellKeyboard(overrides = {}) {
  const actions = {
    computeVideoSync: vi.fn(),
    handleActivityFileOpen: vi.fn(),
    handleCreateNewTemplate: vi.fn(),
    handleImportTemplate: vi.fn(),
    handleImportVideo: vi.fn(),
    handleOpenDownloads: vi.fn(),
    handleSaveTemplate: vi.fn(),
    openRenderDialog: vi.fn(),
    openTemplateSelector: vi.fn(),
    toggleWidgetDrawer: vi.fn(),
  }

  const hook = renderHook(() =>
    useAppShellKeyboard({
      activitySummary: { durationSeconds: 20 },
      backendStatus: 'connected',
      config: { scene: {} },
      importedMediaFilename: null,
      importedVideoPath: 'ride.mp4',
      renderDisabled: false,
      showTemplateStatus: true,
      templateSelectorOpen: false,
      widgetDrawerOpen: false,
      ...actions,
      ...overrides,
    }),
  )

  return { ...hook, actions }
}

describe('useAppShellKeyboard', () => {
  test('routes Ctrl+N and Ctrl+I to their shell commands', () => {
    const { actions } = renderShellKeyboard()

    act(() => {
      window.dispatchEvent(new KeyboardEvent('keydown', { ctrlKey: true, key: 'n' }))
      window.dispatchEvent(new KeyboardEvent('keydown', { ctrlKey: true, key: 'i' }))
    })

    expect(actions.handleCreateNewTemplate).toHaveBeenCalledOnce()
    expect(actions.handleImportVideo).toHaveBeenCalledOnce()
  })

  test('opens the template selector with Ctrl+T', () => {
    const { actions } = renderShellKeyboard()

    act(() => {
      window.dispatchEvent(new KeyboardEvent('keydown', { ctrlKey: true, key: 't' }))
    })

    expect(actions.openTemplateSelector).toHaveBeenCalledOnce()
  })

  test('suppresses shell commands in inputs', () => {
    const { actions } = renderShellKeyboard()
    const input = document.createElement('input')
    document.body.appendChild(input)

    act(() => {
      input.dispatchEvent(new KeyboardEvent('keydown', { bubbles: true, ctrlKey: true, key: 'n' }))
    })

    expect(actions.handleCreateNewTemplate).not.toHaveBeenCalled()
    input.remove()
  })

  test('uses the same auto-sync command as the settings button', () => {
    const { actions } = renderShellKeyboard()

    act(() => {
      window.dispatchEvent(new KeyboardEvent('keydown', { ctrlKey: true, key: 'a', shiftKey: true }))
    })

    expect(actions.computeVideoSync).toHaveBeenCalledWith({ durationSeconds: 20 })
  })

  test('keeps disabled actions as no-ops', () => {
    const { actions } = renderShellKeyboard({ backendStatus: 'offline', renderDisabled: true, showTemplateStatus: false })

    act(() => {
      window.dispatchEvent(new KeyboardEvent('keydown', { ctrlKey: true, key: 's' }))
      window.dispatchEvent(new KeyboardEvent('keydown', { ctrlKey: true, key: 'e' }))
      window.dispatchEvent(new KeyboardEvent('keydown', { ctrlKey: true, shiftKey: true, key: 'e' }))
    })

    expect(actions.handleSaveTemplate).not.toHaveBeenCalled()
    expect(actions.openRenderDialog).not.toHaveBeenCalled()
    expect(actions.handleOpenDownloads).not.toHaveBeenCalled()
  })
})
