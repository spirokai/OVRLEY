/** @param {object} options Scene inputs and the preview activity window; omitted previewRasterScale keeps native raster resolution. @returns {object} Rust preview scene. */
export function buildWasmScene({ globalDefaults, sceneSize, window, fallbackDurationSeconds, previewRasterScale = 1 }) {
  return {
    ...sceneSize,
    border_color: globalDefaults.border_color,
    border_thickness: globalDefaults.border_thickness,
    shadow_color: globalDefaults.shadow_color,
    shadow_strength: globalDefaults.shadow_strength,
    shadow_distance: globalDefaults.shadow_distance,
    fixed_width_digits: globalDefaults.fixed_width_digits,
    scale: globalDefaults.scale,
    opacity: globalDefaults.opacity,
    export_start_seconds: window.exportStartSecond,
    start: window.startSecond,
    end: window.endSecond,
    custom_export_range_active: window.isScoped,
    preview_duration_seconds: fallbackDurationSeconds,
    preview_raster_scale: previewRasterScale,
  }
}
