import { useEffect, useState } from 'react'
import { listMapStyles } from '@/api/backend'
import useStore from '@/store/useStore'

/**
 * The map style picker. Names stay as their files give them, in every language. A style that isn't
 * installed draws the first installed one, as the backend resolves it, so that is the one shown.
 *
 * @param {string} mapStyle The widget's style id.
 * @returns {{options: {value: string, label: string}[], value: string}} Installed styles and the drawn one.
 */
export function useMapStyleSelect(mapStyle) {
  const [styles, setStyles] = useState([])
  useEffect(() => {
    let cancelled = false
    listMapStyles()
      .then((loaded) => {
        if (!cancelled) setStyles(loaded)
      })
      .catch((error) => {
        if (!cancelled) useStore.getState().setErrorMessage(error.message)
      })
    return () => {
      cancelled = true
    }
  }, [])
  return {
    options: styles.map((style) => ({ value: style.id, label: style.label })),
    value: (styles.find((style) => style.id === mapStyle) ?? styles[0])?.id ?? mapStyle,
  }
}
