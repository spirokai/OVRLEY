import { useEffect, useState } from 'react'
import { listMapStyles } from '@/api/backend'
import useStore from '@/store/useStore'

/**
 * A map style picker over the installed styles. Names stay as their files give them, in every
 * language. A style that isn't installed draws the first installed one, as the backend resolves it,
 * so that is the one shown. Until the styles load, and outside the desktop app, none is.
 *
 * @param {string|null} mapStyle The chosen style id, or null for the first installed style.
 * @returns {{options: {value: string, label: string}[], value: string|null}} Installed styles and the drawn one.
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
    value: (styles.find((style) => style.id === mapStyle) ?? styles[0])?.id ?? null,
  }
}
