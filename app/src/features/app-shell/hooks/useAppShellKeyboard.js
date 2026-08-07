import { useEffect, useEffectEvent } from 'react'
import { isFormFieldShortcut, matchKeyboardShortcut } from '@/lib/keyboard-shortcuts'

function hasOpenKeyboardOverlay() {
  if (typeof document === 'undefined') return false

  return Boolean(document.querySelector('[data-slot="dialog-content"], [data-slot="popover-content"], [data-testid="widget-drawer-backdrop"]'))
}

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
  handleOpenDownloads,
  renderWorkflow,
  templateManagement,
  videoControls,
}) {
  const { handleActivityFileOpen } = activityImport
  const { activitySummary, computeVideoSync, config, importedVideoPath, toggleWidgetDrawer, widgetDrawerOpen } = appShell
  const { backendStatus } = backendState
  const { openRenderDialog, renderDisabled } = renderWorkflow
  const { handleCreateNewTemplate, handleImportTemplate, handleSaveTemplate, openTemplateSelector, showTemplateStatus, templateSelectorOpen } =
    templateManagement
  const { handleImportVideo } = videoControls

  const onKeyDown = useEffectEvent((event) => {
    if (event.repeat || event.defaultPrevented) return

    const match = matchKeyboardShortcut(event, 'app')
    if (!match || isFormFieldShortcut(event)) return
    if (hasOpenKeyboardOverlay() && !(match.commandId === 'drawer.toggle' && widgetDrawerOpen)) return

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
        handleOpenDownloads()
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
        toggleWidgetDrawer()
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
