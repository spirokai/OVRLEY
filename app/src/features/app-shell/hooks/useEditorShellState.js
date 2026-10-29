/**
 * Editor shell state for the overlay editor chrome.
 *
 * This hook owns session-scoped UI preferences that affect how the editor is
 * presented, not how a template is serialized. The values intentionally live
 * only in React state:
 *
 * - They must start from explicit in-memory defaults on every launch.
 * - They must not hydrate from browser storage during startup.
 * - They must not write browser storage as the user toggles them.
 *
 * That keeps module evaluation pure, avoids hidden durability rules, and makes
 * the hook safe to render in non-browser test environments.
 */

import { clamp } from '@/lib/utils'
import { useState } from 'react'

// Enable debug controls during local development.
const DEBUG_MODE_ENABLED = true

/**
 * Container hook for editor shell chrome state.
 *
 * Responsibilities:
 *
 * - Own editor-only presentation state such as zoom and background mode.
 * - Expose direct setters for toolbar controls.
 * - Remain safe when `window` is unavailable during tests or server-style renders.
 *
 * The returned state is deliberately local to the current session. Template
 * loading and store hydration do not participate in these values.
 *
 * @returns {{
 *   activeKeyboardWorkspace: string,
 *   closeKeyboardShortcuts: Function,
 *   decreaseZoom: Function,
 *   debugModeEnabled: boolean,
 *   editorBackgroundMode: string,
 *   editorGridVisible: boolean,
 *   editorSnapToGrid: boolean,
 *   editorZoomLevel: number,
 *   increaseZoom: Function,
 *   keyboardShortcutsOpen: boolean,
 *   openKeyboardShortcuts: Function,
 *   resetZoom: Function,
 *   setActiveKeyboardWorkspace: Function,
 *   setEditorBackgroundMode: Function,
 *   setEditorGridVisible: Function,
 *   setEditorSnapToGrid: Function,
 *   setEditorZoomLevel: Function,
 * }}
 */
export default function useEditorShellState() {
  // Session-only editor presentation state. These defaults are the canonical
  // launch values and are not restored from previous app runs.
  const [editorZoomLevel, setEditorZoomLevel] = useState(1)
  const [editorBackgroundMode, setEditorBackgroundMode] = useState('checker')
  const [editorGridVisible, setEditorGridVisible] = useState(false)
  const [editorSnapToGrid, setEditorSnapToGrid] = useState(false)
  const [keyboardShortcutsOpen, setKeyboardShortcutsOpen] = useState(false)
  const [activeKeyboardWorkspace, setActiveKeyboardWorkspace] = useState('editor')
  const debugModeEnabled = import.meta.env.DEV && DEBUG_MODE_ENABLED

  // Zoom adjustments use fixed increments and clamp to editor-supported bounds
  // so wheel/toolbar interactions cannot push the viewport into unusable ranges.
  const decreaseZoom = () => {
    setEditorZoomLevel((current) => clamp(Number((current - 0.05).toFixed(2)), 0.35, 4))
  }

  const increaseZoom = () => {
    setEditorZoomLevel((current) => clamp(Number((current + 0.05).toFixed(2)), 0.35, 4))
  }

  const resetZoom = () => {
    setEditorZoomLevel(1)
  }

  const openKeyboardShortcuts = () => {
    setKeyboardShortcutsOpen(true)
  }

  const closeKeyboardShortcuts = () => {
    setKeyboardShortcutsOpen(false)
  }

  return {
    activeKeyboardWorkspace,
    closeKeyboardShortcuts,
    decreaseZoom,
    debugModeEnabled,
    editorBackgroundMode,
    editorGridVisible,
    editorSnapToGrid,
    editorZoomLevel,
    increaseZoom,
    keyboardShortcutsOpen,
    openKeyboardShortcuts,
    resetZoom,
    setActiveKeyboardWorkspace,
    setEditorBackgroundMode,
    setEditorGridVisible,
    setEditorSnapToGrid,
    setEditorZoomLevel,
  }
}
