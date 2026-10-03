import { useEffect, useRef, useState } from 'react'
import useStore from '@/store/useStore'
import { getFontCatalog, getPreparedFont, isFontPrepared, prepareFont } from '@/lib/font-resources'
import { createFontSelection, getFontWeightControl, supportsFontItalic } from '@/lib/fonts'

/** @returns {Function} Commits the latest font intent after preparation; errors go to the editor's error owner. */
export function useFontPreparation() {
  const requests = useRef(new Map())
  useEffect(() => () => requests.current.clear(), [])

  return (key, ids, commit) => {
    const keys = Array.isArray(key) ? key : [key]
    const request = {}
    const documentRevision = useStore.getState().editorDocumentRevision
    for (const key of keys) requests.current.set(key, request)
    const isCurrent = () =>
      useStore.getState().editorDocumentRevision === documentRevision && keys.every((key) => requests.current.get(key) === request)
    const release = () => {
      for (const key of keys) {
        if (requests.current.get(key) === request) requests.current.delete(key)
      }
    }
    if (ids.every(isFontPrepared)) {
      try {
        commit()
      } finally {
        release()
      }
      return
    }
    Promise.all(ids.map(prepareFont))
      .then(() => {
        if (isCurrent()) commit()
      })
      .catch((error) => {
        if (isCurrent()) useStore.getState().setErrorMessage(error.message)
      })
      .finally(release)
  }
}

/** @param {object} widget Effective widget. @param {Function} updateWidgetData Editor action. @returns {object} Label typography controls and actions. */
export function useLabelTypography(widget, updateWidgetData) {
  const font = widget.category === 'labels' ? getPreparedFont(widget.data.font) : null
  const controls = font ? getFontWeightControl(font, widget.data.font_weight, widget.data.italic) : null
  const italicSupported = font !== null && supportsFontItalic(font)
  return {
    ...controls,
    font,
    changeFont: (id) => updateWidgetData(widget.id, createFontSelection(id)),
    changeItalic: () => updateWidgetData(widget.id, { italic: !widget.data.italic }),
    italicSupported,
    italic: widget.data.italic && italicSupported,
  }
}
/**
 * Provides available fonts state from system and bundled fonts.
 */

const EMPTY_AVAILABLE_FONTS = {
  recommendedFonts: [],
  systemFonts: [],
}

/** @returns {object} Available family options from the shared session catalog. */
export function useAvailableFonts() {
  const [availableFonts, setAvailableFonts] = useState(EMPTY_AVAILABLE_FONTS)

  useEffect(() => {
    let cancelled = false

    getFontCatalog()
      .then((fonts) => {
        if (!cancelled) {
          setAvailableFonts(fonts)
        }
      })
      .catch((error) => {
        console.warn('Failed to load available fonts:', error)
      })

    return () => {
      cancelled = true
    }
  }, [])

  return availableFonts
}
