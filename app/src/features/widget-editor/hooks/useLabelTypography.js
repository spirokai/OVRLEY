import { useEffect, useState } from 'react'
import { createFontSelection, getFontWeightControl, loadFont, resolveFontStyle, supportsFontItalic } from '@/lib/fonts'

/**
 * Loads the selected label's capabilities and owns explicit font-switch updates.
 * @param {object} widget Effective label or other typography widget.
 * @param {Function} updateWidgetData Existing committed widget updater.
 * @returns {object} Weight control presentation and font-change action.
 */
export default function useLabelTypography(widget, updateWidgetData) {
  const fontId = widget.data.font
  const isLabel = widget.category === 'labels'
  const [loaded, setLoaded] = useState(null)
  const [error, setError] = useState(null)

  useEffect(() => {
    if (!isLabel) return undefined
    let cancelled = false
    loadFont(fontId)
      .then((font) => {
        if (!cancelled) setLoaded(font)
      })
      .catch((error) => {
        if (!cancelled) setError(error)
      })
    return () => {
      cancelled = true
    }
  }, [fontId, isLabel])

  const font = loaded?.id === fontId ? loaded : null
  const controls = font ? getFontWeightControl(font, widget.data.font_weight, widget.data.italic) : null

  async function changeFont(id) {
    try {
      const selection = createFontSelection(id)
      if (isLabel) {
        const selected = await loadFont(id)
        selection.italic = widget.data.italic && supportsFontItalic(selected)
        selection.font_weight = resolveFontStyle(selected, widget.data.font_weight, selection.italic).weight
      }
      updateWidgetData(widget.id, selection)
    } catch (error) {
      setError(error)
    }
  }

  function changeItalic() {
    updateWidgetData(widget.id, { italic: !widget.data.italic })
  }

  const italicSupported = font !== null && supportsFontItalic(font)
  return { ...controls, font, error, changeFont, changeItalic, italicSupported, italic: widget.data.italic && italicSupported }
}
