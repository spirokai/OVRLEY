/**
 * Keyboard shortcut handler for the overlay editor.
 */

import { useEffect } from 'react'
import { deleteWidgetsInConfig, duplicateWidgetsInConfig } from '@/lib/widget/widget-config'
import { isEditableElement } from '../utils/overlayEditorHelpers'

/**
 * Registers keyboard listeners for delete/copy/paste editor actions.
 *
 * @param {object} options
 * @param {*} options.config - Current overlay template config.
 * @param {Function} options.onConfigChange - Callback to update config.
 * @param {Array} options.selectedWidgetIds - Currently selected widget IDs.
 * @param {Array} options.selectedWidgets - Currently selected widgets.
 * @param {Function} options.setWidgetSelection - Store-backed selection intent action.
 * @param {React.MutableRefObject} options.clipboardRef - Editor-local clipboard ref.
 */
export function useEditorKeyboard({ config, onConfigChange, selectedWidgetIds, selectedWidgets, setWidgetSelection, clipboardRef }) {
  useEffect(() => {
    if (!config) {
      return undefined
    }

    const handleKeyDown = (event) => {
      if (event.defaultPrevented || event.altKey || isEditableElement(event.target)) {
        return
      }

      const modifierKey = event.metaKey || event.ctrlKey
      const normalizedKey = String(event.key || '').toLowerCase()

      if (!modifierKey && event.key === 'Delete') {
        if (!selectedWidgetIds.length) {
          return
        }

        event.preventDefault()
        onConfigChange(deleteWidgetsInConfig(config, selectedWidgetIds))
        setWidgetSelection([])
        return
      }

      if (!modifierKey) {
        return
      }

      if (normalizedKey === 'c') {
        if (!selectedWidgets.length) {
          return
        }

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
      }

      if (normalizedKey !== 'v') {
        return
      }

      const clipboardWidgets = clipboardRef.current?.widgets
      if (!Array.isArray(clipboardWidgets) || !clipboardWidgets.length) {
        return
      }

      event.preventDefault()
      const { config: nextConfig, insertedWidgetIds } = duplicateWidgetsInConfig(config, clipboardWidgets)
      onConfigChange(nextConfig)
      setWidgetSelection(insertedWidgetIds, insertedWidgetIds.at(-1) ?? null)
    }

    window.addEventListener('keydown', handleKeyDown)
    return () => window.removeEventListener('keydown', handleKeyDown)
  }, [clipboardRef, config, onConfigChange, selectedWidgetIds, selectedWidgets, setWidgetSelection])
}
