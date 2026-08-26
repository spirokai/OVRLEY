/**
 * Keyboard shortcut handler for the overlay editor.
 */

import { useEffect, useEffectEvent } from 'react'
import { deleteWidgetsInConfig, duplicateWidgetsInConfig, updateWidgetsInConfig } from '@/lib/widget/widget-config'
import { hasOpenOverlay, isFormFieldShortcut, matchKeyboardShortcut } from '@/lib/keyboard-shortcuts'

/**
 * Registers keyboard listeners for editor actions.
 *
 * @param {object} options
 * @param {*} options.config - Current overlay template config.
 * @param {Function} options.onConfigChange - Callback to update config.
 * @param {Array} options.selectedWidgetIds - Currently selected widget IDs.
 * @param {Array} options.selectedWidgets - Currently selected widgets.
 * @param {Function} options.setWidgetSelection - Store-backed selection intent action.
 * @param {React.MutableRefObject} options.clipboardRef - Editor-local clipboard ref.
 * @param {object} options.editorShell - Canonical editor shell state and actions.
 */
export function useEditorKeyboard({ config, onConfigChange, selectedWidgetIds, selectedWidgets, setWidgetSelection, clipboardRef, editorShell }) {
  const onKeyDown = useEffectEvent((event) => {
    if (event.defaultPrevented) return

    const match = matchKeyboardShortcut(event, 'editor')
    if (!match || isFormFieldShortcut(event)) return

    switch (match.commandId) {
      case 'editor.clearSelection':
        if (hasOpenOverlay()) return
        event.preventDefault()
        setWidgetSelection([])
        return
      case 'editor.delete':
        if (!selectedWidgetIds.length) return
        event.preventDefault()
        onConfigChange(deleteWidgetsInConfig(config, selectedWidgetIds))
        setWidgetSelection([])
        return
      case 'editor.copy':
        if (!selectedWidgets.length) return
        event.preventDefault()
        clipboardRef.current = {
          widgets: selectedWidgets.map((widget) => ({
            id: widget.id,
            category: widget.category,
            type: widget.type,
            data: widget.data,
          })),
        }
        return
      case 'editor.paste': {
        const clipboardWidgets = clipboardRef.current?.widgets
        if (!Array.isArray(clipboardWidgets) || !clipboardWidgets.length) return
        event.preventDefault()
        const { config: nextConfig, insertedWidgetIds } = duplicateWidgetsInConfig(config, clipboardWidgets)
        onConfigChange(nextConfig)
        setWidgetSelection(insertedWidgetIds, insertedWidgetIds.at(-1) ?? null)
        return
      }
      case 'editor.nudge': {
        if (editorShell.activeKeyboardWorkspace !== 'editor' || !selectedWidgets.length) return
        const step = match.binding.step
        const delta = {
          x: match.binding.key === 'arrowleft' ? -step : match.binding.key === 'arrowright' ? step : 0,
          y: match.binding.key === 'arrowup' ? -step : match.binding.key === 'arrowdown' ? step : 0,
        }
        const updatesById = Object.fromEntries(
          selectedWidgets.map((widget) => [
            widget.id,
            {
              x: widget.data.x + delta.x,
              y: widget.data.y + delta.y,
            },
          ]),
        )
        event.preventDefault()
        onConfigChange(updateWidgetsInConfig(config, updatesById))
        return
      }
      case 'editor.toggleSnap':
        event.preventDefault()
        editorShell.setEditorSnapToGrid(!editorShell.editorSnapToGrid)
        return
      case 'editor.toggleGrid':
        event.preventDefault()
        editorShell.setEditorGridVisible(!editorShell.editorGridVisible)
        return
      case 'editor.zoomIn':
        event.preventDefault()
        editorShell.increaseZoom()
        return
      case 'editor.zoomOut':
        event.preventDefault()
        editorShell.decreaseZoom()
        return
      case 'editor.resetZoom':
        event.preventDefault()
        editorShell.resetZoom()
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
