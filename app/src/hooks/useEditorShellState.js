/**
 * Implements the use Editor Shell State hook and related behavior for the app.
 */

import { useEffect, useState } from 'react'

/**
 * Constrains a value to the provided minimum and maximum bounds.
 *
 * @param {*} value - Input value processed by the helper.
 * @param {*} min - Lower bound used by the calculation.
 * @param {*} max - Upper bound used by the calculation.
 * @returns {number} Result produced by the helper.
 */
function clamp(value, min, max) {
  return Math.min(max, Math.max(min, value))
}

/**
 * Returns ui scale.
 *
 * @param {*} width - Numeric width value.
 * @returns {*} Requested value or structure.
 */
function getUiScale(width) {
  return clamp(Number((width / 1440).toFixed(3)), 0.9, 1.08)
}

/**
 * Provides editor shell state state and actions.
 * @returns {object} Result produced by the helper.
 */
export default function useEditorShellState() {
  const [editorZoomLevel, setEditorZoomLevel] = useState(1)
  const [editorBackgroundMode, setEditorBackgroundMode] = useState(() => localStorage.getItem('overlayBackgroundMode') || 'checker')
  const [editorGridVisible, setEditorGridVisible] = useState(() => localStorage.getItem('overlayGridVisible') === 'true')
  const [editorSnapToGrid, setEditorSnapToGrid] = useState(() => localStorage.getItem('overlaySnapToGrid') === 'true')
  const [uiScale, setUiScale] = useState(() => (typeof window === 'undefined' ? 1 : getUiScale(window.innerWidth)))

  useEffect(() => {
    localStorage.setItem('overlayBackgroundMode', editorBackgroundMode)
  }, [editorBackgroundMode])

  useEffect(() => {
    localStorage.setItem('overlayGridVisible', String(editorGridVisible))
  }, [editorGridVisible])

  useEffect(() => {
    localStorage.setItem('overlaySnapToGrid', String(editorSnapToGrid))
  }, [editorSnapToGrid])

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
