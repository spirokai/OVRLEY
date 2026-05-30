import { useState, useCallback } from 'react'
import { getFpsModeValue, sanitizeIntegerFps, PRESET_FPS_VALUES } from '@/lib/update-rate'

export function useFpsMode({ fps, onFpsChange }) {
  const [customFpsAnchor, setCustomFpsAnchor] = useState(null)
  const fpsMode = customFpsAnchor !== null && Number(fps) === customFpsAnchor ? 'custom' : getFpsModeValue(fps)

  const handleFpsModeChange = useCallback(
    (value) => {
      if (value === 'custom') {
        setCustomFpsAnchor(Number(fps))
        return
      }
      setCustomFpsAnchor(null)
      const nextFps = sanitizeIntegerFps(value)
      onFpsChange(nextFps)
    },
    [fps, onFpsChange],
  )

  const handleCustomFpsChange = useCallback(
    (rawValue) => {
      const nextFps = sanitizeIntegerFps(rawValue)
      setCustomFpsAnchor(PRESET_FPS_VALUES.includes(nextFps) ? null : nextFps)
      onFpsChange(nextFps)
    },
    [onFpsChange],
  )

  return { fpsMode, handleFpsModeChange, handleCustomFpsChange, customFpsAnchor, setCustomFpsAnchor }
}
