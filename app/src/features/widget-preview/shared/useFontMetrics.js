import { useEffect, useState } from 'react'
import { getFontRenderStyle, loadFont } from '@/lib/fonts'
import { hasTauriRuntime } from '@/api/backend'
import { WIDGET_FONT_WEIGHT } from '@/lib/widget/standard-widgets'
import { getPreviewFontFamily } from './textMeasurement'

/**
 * Reloads canvas font metrics when the requested fonts become ready.
 *
 * @param {{ fontId: string, fontSize: number, fontWeight?: number, italic?: boolean }[]} fontRequests - Requested fonts.
 * @returns {number} Readiness version for the requested fonts.
 */
export function useFontMetrics(fontRequests = []) {
  const requestKey = JSON.stringify(fontRequests)
  const [version, setVersion] = useState(0)
  const [error, setError] = useState(null)

  useEffect(() => {
    if (!requestKey || requestKey === '[]' || typeof document === 'undefined' || !document.fonts || typeof document.fonts.load !== 'function') {
      return undefined
    }

    let cancelled = false
    const requests = JSON.parse(requestKey)

    Promise.all([
      ...requests.map(async ({ fontId, fontSize, fontWeight = WIDGET_FONT_WEIGHT, italic = false }) => {
        if (hasTauriRuntime()) await loadFont(fontId)
        const { weight, fontStyle } = getFontRenderStyle(fontId, fontWeight, italic)
        const fontFamily = getPreviewFontFamily(fontId)
        await document.fonts.load(`${fontStyle === 'normal' ? '' : `${fontStyle} `}${weight} ${fontSize}px ${fontFamily}`, '0123456789WBMPRK/H')
      }),
      document.fonts.ready,
    ])
      .then(() => {
        if (!cancelled) setVersion((current) => current + 1)
      })
      .catch((error) => {
        if (!cancelled) setError(error)
      })

    return () => {
      cancelled = true
    }
  }, [requestKey])

  if (error) throw error
  return version
}
