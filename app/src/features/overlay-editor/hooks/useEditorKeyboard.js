/**
 * Keyboard shortcut handler for the overlay editor (Delete key).
 */

import { useEffect } from 'react'
import { deleteWidgetsInConfig } from '@/lib/widget-config'
import { isEditableElement } from '../utils/overlayEditorHelpers'

/**
 * Registers a Delete key listener that removes selected widgets.
 *
 * @param {object} options
 * @param {*} options.config - Current overlay template config.
 * @param {Function} options.onConfigChange - Callback to update config.
 * @param {Array} options.selectedWidgetIds - Currently selected widget IDs.
 * @param {Function} options.setWidgetSelection - Store-backed selection intent action.
 */
export function useEditorKeyboard({ config, onConfigChange, selectedWidgetIds, setWidgetSelection }) {
  useEffect(() => {
    if (!selectedWidgetIds.length || !config) {
      return undefined
    }

    const handleKeyDown = (event) => {
      if (event.key !== 'Delete') {
        return
      }

      if (event.defaultPrevented || event.metaKey || event.ctrlKey || event.altKey || isEditableElement(event.target)) {
        return
      }

      event.preventDefault()
      onConfigChange(deleteWidgetsInConfig(config, selectedWidgetIds))
      setWidgetSelection([])
    }

    window.addEventListener('keydown', handleKeyDown)
    return () => window.removeEventListener('keydown', handleKeyDown)
  }, [config, onConfigChange, selectedWidgetIds, setWidgetSelection])
}
