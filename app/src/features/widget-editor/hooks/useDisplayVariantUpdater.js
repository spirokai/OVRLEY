import { useCallback } from 'react'
import { buildFrameGeometryUpdate } from '@/lib/widget/widget-resolver'

const GEOMETRY_KEYS = ['width', 'height', 'rotation']

/**
 * Returns a stable callback that merges updates into a display variant,
 * automatically routing geometry keys through buildFrameGeometryUpdate
 * to keep top-level and variant-level geometry in sync.
 *
 * @param {object} widget - Widget config object.
 * @param {string} variantKey - Key in display_variants (e.g. 'linear').
 * @param {Function} updateWidgetData - Widget data updater.
 * @returns {Function} updateVariant(updates) callback.
 */
export default function useDisplayVariantUpdater(widget, variantKey, updateWidgetData) {
  return useCallback(
    (updates) => {
      const geometryKeys = Object.keys(updates).filter((key) => GEOMETRY_KEYS.includes(key))
      const geometryPatch = geometryKeys.length > 0 ? Object.fromEntries(geometryKeys.map((key) => [key, updates[key]])) : null

      const patch = geometryPatch ? buildFrameGeometryUpdate(widget.data, geometryPatch) : {}

      patch.display_variants = {
        ...patch.display_variants,
        [variantKey]: { ...patch.display_variants?.[variantKey], ...updates },
      }

      updateWidgetData(widget.id, patch)
    },
    [updateWidgetData, widget.data, widget.id, variantKey],
  )
}
