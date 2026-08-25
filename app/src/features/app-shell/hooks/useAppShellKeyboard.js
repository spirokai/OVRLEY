import { useEffect, useEffectEvent } from 'react'
import { hasOpenOverlay, isFormFieldShortcut, matchKeyboardShortcut } from '@/lib/keyboard-shortcuts'
import { WIDGETS_TOOL } from '@/store/slices/createLayoutSlice'

/**
 * Registers shell-level template, media, export, and workspace commands.
 *
 * @param {object} options - Shell actions and availability state.
 * @returns {void}
 */
export default function useAppShellKeyboard({
  activityImport,
  appShell,
  backendState,
  handleOpenOutputDirectory,
  renderWorkflow,
  templateManagement,
  videoControls,
  layout,
}) {
  const { handleActivityFileOpen } = activityImport
  const { activitySummary, computeVideoSync, config, importedVideoPath } = appShell
  const { leftDrawerPinned, leftDrawerVisible, selectLeftDrawerTool } = layout
  const { backendStatus } = backendState
  const { openRenderDialog, renderDisabled } = renderWorkflow
  const { handleCreateNewTemplate, handleImportTemplate, handleSaveTemplate, openTemplateSelector, showTemplateStatus, templateSelectorOpen } =
    templateManagement
  const { handleImportVideo } = videoControls

  const onKeyDown = useEffectEvent((event) => {
    if (event.repeat || event.defaultPrevented) return

    const match = matchKeyboardShortcut(event, 'app')
    if (!match || isFormFieldShortcut(event)) return
    if (hasOpenOverlay() && !(match.commandId === 'drawer.toggle' && leftDrawerVisible && !leftDrawerPinned)) return

    switch (match.commandId) {
      case 'template.new':
        event.preventDefault()
        handleCreateNewTemplate()
        return
      case 'template.import':
        event.preventDefault()
        handleImportTemplate()
        return
      case 'template.save':
        if (!config || !showTemplateStatus) return
        event.preventDefault()
        handleSaveTemplate()
        return
      case 'activity.import':
        event.preventDefault()
        handleActivityFileOpen()
        return
      case 'video.import':
        event.preventDefault()
        handleImportVideo()
        return
      case 'template.select':
        if (templateSelectorOpen) return
        event.preventDefault()
        openTemplateSelector()
        return
      case 'output.open':
        if (backendStatus !== 'connected') return
        event.preventDefault()
        handleOpenOutputDirectory()
        return
      case 'sync.auto':
        if (!importedVideoPath || !activitySummary) return
        event.preventDefault()
        computeVideoSync(activitySummary)
        return
      case 'render.open':
        if (renderDisabled) return
        event.preventDefault()
        openRenderDialog()
        return
      case 'drawer.toggle':
        event.preventDefault()
        selectLeftDrawerTool(WIDGETS_TOOL)
        return
      default:
        return
    }
  })

  useEffect(() => {
    if (typeof window === 'undefined') return undefined

    const handleKeyDown = (event) => onKeyDown(event)
    window.addEventListener('keydown', handleKeyDown)
    return () => window.removeEventListener('keydown', handleKeyDown)
  }, [])
}
