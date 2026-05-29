/**
 * Editor shell state — local UI state for the overlay editor chrome.
 * Owns zoom level, background mode, grid visibility, snap-to-grid, and UI scale.
 */

import { clamp } from '@/lib/utils'
import { useEffect, useState } from 'react'
import { DEBUG_MODE_ENABLED } from '@/App'

/**
 * Derives the UI scale factor from the viewport width.
 * Scaled from a 1440px reference, clamped to 0.9–1.08.
 *
 * @param {number} width - Viewport width in pixels.
 * @returns {number} UI scale factor.
 */
function getUiScale(width) {
  return clamp(Number((width / 1440).toFixed(3)), 0.9, 1.08)
}

/**
 * Container hook for editor shell chrome state.
 * Manages zoom, background/grid/snap toggles, and responsive UI scale.
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
  // Local UI state — editor viewport settings with localStorage hydration
  const [editorZoomLevel, setEditorZoomLevel] = useState(1)
  const [editorBackgroundMode, setEditorBackgroundMode] = useState(() => localStorage.getItem('overlayBackgroundMode') || 'checker')
  const [editorGridVisible, setEditorGridVisible] = useState(() => localStorage.getItem('overlayGridVisible') === 'true')
  const [editorSnapToGrid, setEditorSnapToGrid] = useState(() => localStorage.getItem('overlaySnapToGrid') === 'true')
  const [uiScale, setUiScale] = useState(() => (typeof window === 'undefined' ? 1 : getUiScale(window.innerWidth)))
  const debugModeEnabled = import.meta.env.DEV && DEBUG_MODE_ENABLED

  // Persistence effects — sync each toggle to localStorage on change
  useEffect(() => {
    localStorage.setItem('overlayBackgroundMode', editorBackgroundMode)
  }, [editorBackgroundMode])

  useEffect(() => {
    localStorage.setItem('overlayGridVisible', String(editorGridVisible))
  }, [editorGridVisible])

  useEffect(() => {
    localStorage.setItem('overlaySnapToGrid', String(editorSnapToGrid))
  }, [editorSnapToGrid])

  // UI scale — recalculate scale on window resize
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

  // Zoom handlers — step-based zoom in/out/reset with clamped bounds
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
