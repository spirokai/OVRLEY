import { resolveExportRangeWindow } from '@/lib/export-range'

/** @param {object} options Scene inputs; omitted previewRasterScale keeps native raster resolution. @returns {object} Rust preview scene. */
export function buildWasmScene({ globalDefaults, sceneSize, exportRange, activity, fallbackDurationSeconds, previewRasterScale = 1 }) {
  const window = resolveExportRangeWindow(activity, exportRange)
  return {
    ...sceneSize,
    border_color: globalDefaults.border_color,
    border_thickness: globalDefaults.border_thickness,
    shadow_color: globalDefaults.shadow_color,
    shadow_strength: globalDefaults.shadow_strength,
    shadow_distance: globalDefaults.shadow_distance,
    scale: globalDefaults.scale,
    opacity: globalDefaults.opacity,
    export_start_seconds: exportRange.from,
    start: window.start,
    end: window.end,
    custom_export_range_active: window.active,
    preview_duration_seconds: fallbackDurationSeconds,
    preview_raster_scale: previewRasterScale,
  }
}
