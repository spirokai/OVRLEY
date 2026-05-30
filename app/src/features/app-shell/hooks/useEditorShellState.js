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
import { useEffect, useState } from 'react'
import { DEBUG_MODE_ENABLED } from '@/lib/dev-config'

/**
 * Derives the UI scale factor from the viewport width.
 *
 * The editor chrome scales gently with available width so controls stay usable
 * on narrower layouts without overgrowing on large monitors. The factor is
 * derived from a 1440px reference width and then clamped to a conservative
 * range so the shell remains visually stable across desktop sizes.
 *
 * @param {number} width - Viewport width in pixels.
 * @returns {number} UI scale factor.
 */
function getUiScale(width) {
  return clamp(Number((width / 1440).toFixed(3)), 0.9, 1.08)
}

/**
 * Container hook for editor shell chrome state.
 *
 * Responsibilities:
 *
 * - Own editor-only presentation state such as zoom and background mode.
 * - Expose direct setters for toolbar controls.
 * - Keep UI scale in sync with the current window width when running in a browser.
 * - Remain safe when `window` is unavailable during tests or server-style renders.
 *
 * The returned state is deliberately local to the current session. Template
 * loading and store hydration do not participate in these values.
 *
 * @returns {{
 *   decreaseZoom: Function,
 *   debugModeEnabled: boolean,
 *   editorBackgroundMode: string,
 *   editorGridVisible: boolean,
 *   editorSnapToGrid: boolean,
 *   editorZoomLevel: number,
 *   increaseZoom: Function,
 *   resetZoom: Function,
 *   setEditorBackgroundMode: Function,
 *   setEditorGridVisible: Function,
 *   setEditorSnapToGrid: Function,
 *   setEditorZoomLevel: Function,
 *   uiScale: number,
 * }}
 */
export default function useEditorShellState() {
  // Session-only editor presentation state. These defaults are the canonical
  // launch values and are not restored from previous app runs.
  const [editorZoomLevel, setEditorZoomLevel] = useState(1)
  const [editorBackgroundMode, setEditorBackgroundMode] = useState('checker')
  const [editorGridVisible, setEditorGridVisible] = useState(false)
  const [editorSnapToGrid, setEditorSnapToGrid] = useState(false)
  const [uiScale, setUiScale] = useState(() => (typeof window === 'undefined' ? 1 : getUiScale(window.innerWidth)))
  // Debug-only controls are surfaced from the app-level development flag.
  const debugModeEnabled = import.meta.env.DEV && DEBUG_MODE_ENABLED

  // Resize subscription for responsive chrome scaling. In non-browser
  // environments there is nothing to subscribe to, so the hook becomes a
  // predictable no-op and keeps the default scale of `1`.
  useEffect(() => {
    if (typeof window === 'undefined') {
      return undefined
    }

    const syncUiScale = () => {
      setUiScale(getUiScale(window.innerWidth))
    }

    syncUiScale()
    window.addEventListener('resize', syncUiScale)
    return () => {
      window.removeEventListener('resize', syncUiScale)
    }
  }, [])

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

  return {
    decreaseZoom,
    debugModeEnabled,
    editorBackgroundMode,
    editorGridVisible,
    editorSnapToGrid,
    editorZoomLevel,
    increaseZoom,
    resetZoom,
    setEditorBackgroundMode,
    setEditorGridVisible,
    setEditorSnapToGrid,
    setEditorZoomLevel,
    uiScale,
  }
}
