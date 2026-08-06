import { useEffect, useEffectEvent } from 'react'
import { isInteractiveElement } from '@/lib/utils'
import { matchKeyboardShortcut } from '@/lib/keyboard-shortcuts'

function hasOpenKeyboardOverlay() {
  if (typeof document === 'undefined') return false

  return Boolean(
    document.querySelector(
      '[data-slot="dialog-content"], [data-slot="select-content"], [data-slot="popover-content"], [data-testid="widget-drawer-backdrop"]',
    ),
  )
}

/**
 * Registers shell-level template, media, export, and workspace commands.
 *
 * @param {object} options - Shell actions and availability state.
 * @returns {void}
 */
export default function useAppShellKeyboard({
  activitySummary,
  backendStatus,
  computeVideoSync,
  config,
  handleActivityFileOpen,
  handleCreateNewTemplate,
  handleImportTemplate,
  handleImportVideo,
  handleOpenDownloads,
  handleSaveTemplate,
  importedMediaFilename,
  importedVideoPath,
  openRenderDialog,
  openTemplateSelector,
  renderDisabled,
  showTemplateStatus,
  templateSelectorOpen,
  toggleWidgetDrawer,
  widgetDrawerOpen,
}) {
  const onKeyDown = useEffectEvent((event) => {
    if (event.repeat || event.defaultPrevented || isInteractiveElement(event.target)) return

    const match = matchKeyboardShortcut(event, 'app')
    if (!match) return
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
        if (importedMediaFilename) return
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
