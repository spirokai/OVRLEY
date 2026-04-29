import { useEffect, useState } from 'react'

function clamp(value, min, max) {
  return Math.min(max, Math.max(min, value))
}

function getUiScale(width) {
  return clamp(Number((width / 1440).toFixed(3)), 0.9, 1.08)
}

export default function useEditorShellState() {
  const [editorZoomLevel, setEditorZoomLevel] = useState(1)
  const [editorBackgroundMode, setEditorBackgroundMode] = useState(
    () => localStorage.getItem('overlayBackgroundMode') || 'checker',
  )
  const [uiScale, setUiScale] = useState(() =>
    typeof window === 'undefined' ? 1 : getUiScale(window.innerWidth),
  )

  useEffect(() => {
    localStorage.setItem('overlayBackgroundMode', editorBackgroundMode)
  }, [editorBackgroundMode])

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
    setEditorZoomLevel((current) =>
      clamp(Number((current - 0.1).toFixed(2)), 0.35, 4),
    )
  }

  const increaseZoom = () => {
    setEditorZoomLevel((current) =>
      clamp(Number((current + 0.1).toFixed(2)), 0.35, 4),
    )
  }

  const resetZoom = () => {
    setEditorZoomLevel(1)
  }

  return {
    decreaseZoom,
    editorBackgroundMode,
    editorZoomLevel,
    increaseZoom,
    resetZoom,
    setEditorBackgroundMode,
    setEditorZoomLevel,
    uiScale,
  }
}
