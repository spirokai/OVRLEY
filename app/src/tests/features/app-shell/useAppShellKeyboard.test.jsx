import { act, renderHook } from '@testing-library/react'
import { describe, expect, test, vi } from 'vitest'
import useAppShellKeyboard from '@/features/app-shell/hooks/useAppShellKeyboard'

function renderShellKeyboard(configure = () => {}) {
  const actions = {
    computeVideoSync: vi.fn(),
    handleActivityFileOpen: vi.fn(),
    handleCreateNewTemplate: vi.fn(),
    handleImportTemplate: vi.fn(),
    handleImportVideo: vi.fn(),
    handleNewProject: vi.fn(),
    handleOpenOutputDirectory: vi.fn(),
    handleOpenProject: vi.fn(),
    handleSaveProject: vi.fn(),
    handleSaveProjectAs: vi.fn(),
    handleSaveTemplate: vi.fn(),
    openRenderDialog: vi.fn(),
    openTemplateSelector: vi.fn(),
    selectLeftDrawerTool: vi.fn(),
  }
  const options = {
    activityImport: {
      handleActivityFileOpen: actions.handleActivityFileOpen,
    },
    appShell: {
      activitySummary: { durationSeconds: 20 },
      computeVideoSync: actions.computeVideoSync,
      config: { scene: {} },
      importedVideoPath: 'ride.mp4',
    },
    backendState: {
      backendStatus: 'connected',
    },
    handleOpenOutputDirectory: actions.handleOpenOutputDirectory,
    layout: {
      leftDrawerPinned: false,
      leftDrawerVisible: false,
      selectLeftDrawerTool: actions.selectLeftDrawerTool,
    },
    projectLifecycle: {
      handleNewProject: actions.handleNewProject,
      handleOpenProject: actions.handleOpenProject,
      handleSaveProject: actions.handleSaveProject,
      handleSaveProjectAs: actions.handleSaveProjectAs,
    },
    renderWorkflow: {
      openRenderDialog: actions.openRenderDialog,
      renderDisabled: false,
    },
    templateManagement: {
      handleCreateNewTemplate: actions.handleCreateNewTemplate,
      handleImportTemplate: actions.handleImportTemplate,
      handleSaveTemplate: actions.handleSaveTemplate,
      openTemplateSelector: actions.openTemplateSelector,
      showTemplateStatus: true,
      templateSelectorOpen: false,
    },
    videoControls: {
      handleImportVideo: actions.handleImportVideo,
      importedMediaFilename: null,
    },
  }

  configure(options)

  const hook = renderHook(() => useAppShellKeyboard(options))

  return { ...hook, actions }
}

describe('useAppShellKeyboard', () => {
  test('routes project and video shortcuts to their shell commands', () => {
    const { actions } = renderShellKeyboard()

    act(() => {
      window.dispatchEvent(new KeyboardEvent('keydown', { ctrlKey: true, key: 'n' }))
      window.dispatchEvent(new KeyboardEvent('keydown', { ctrlKey: true, key: 'i' }))
    })

    expect(actions.handleNewProject).toHaveBeenCalledOnce()
    expect(actions.handleImportVideo).toHaveBeenCalledOnce()
  })

  test('routes project open, save, and save-as shortcuts', () => {
    const { actions } = renderShellKeyboard()

    act(() => {
      window.dispatchEvent(new KeyboardEvent('keydown', { ctrlKey: true, key: 'o' }))
      window.dispatchEvent(new KeyboardEvent('keydown', { ctrlKey: true, key: 's' }))
      window.dispatchEvent(new KeyboardEvent('keydown', { ctrlKey: true, shiftKey: true, key: 's' }))
    })

    expect(actions.handleOpenProject).toHaveBeenCalledOnce()
    expect(actions.handleSaveProject).toHaveBeenCalledOnce()
    expect(actions.handleSaveProjectAs).toHaveBeenCalledOnce()
  })

  test('keeps template commands on their reassigned shortcuts', () => {
    const { actions } = renderShellKeyboard()

    act(() => {
      window.dispatchEvent(new KeyboardEvent('keydown', { ctrlKey: true, shiftKey: true, key: 't' }))
      window.dispatchEvent(new KeyboardEvent('keydown', { ctrlKey: true, shiftKey: true, key: 'l' }))
      window.dispatchEvent(new KeyboardEvent('keydown', { ctrlKey: true, shiftKey: true, key: 'p' }))
    })

    expect(actions.handleCreateNewTemplate).toHaveBeenCalledOnce()
    expect(actions.handleImportTemplate).toHaveBeenCalledOnce()
    expect(actions.handleSaveTemplate).toHaveBeenCalledOnce()
  })

  test('opens the template selector with Ctrl+T', () => {
    const { actions } = renderShellKeyboard()

    act(() => {
      window.dispatchEvent(new KeyboardEvent('keydown', { ctrlKey: true, key: 't' }))
    })

    expect(actions.openTemplateSelector).toHaveBeenCalledOnce()
  })

  test('allows modified shell commands in inputs', () => {
    const { actions } = renderShellKeyboard()
    const input = document.createElement('input')
    document.body.appendChild(input)

    act(() => {
      input.dispatchEvent(new KeyboardEvent('keydown', { bubbles: true, ctrlKey: true, key: 'n' }))
    })

    expect(actions.handleNewProject).toHaveBeenCalledOnce()
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
    const { actions } = renderShellKeyboard((options) => {
      options.backendState.backendStatus = 'offline'
      options.renderWorkflow.renderDisabled = true
      options.templateManagement.showTemplateStatus = false
    })

    act(() => {
      window.dispatchEvent(new KeyboardEvent('keydown', { ctrlKey: true, shiftKey: true, key: 'p' }))
      window.dispatchEvent(new KeyboardEvent('keydown', { ctrlKey: true, key: 'e' }))
      window.dispatchEvent(new KeyboardEvent('keydown', { ctrlKey: true, shiftKey: true, key: 'e' }))
    })

    expect(actions.handleSaveTemplate).not.toHaveBeenCalled()
    expect(actions.openRenderDialog).not.toHaveBeenCalled()
    expect(actions.handleOpenOutputDirectory).not.toHaveBeenCalled()
  })
})
