/**
 * @file Standard Widget Defaults Bridge
 *
 * Bridge between the canonical widget defaults catalog at
 * `assets/standard-widgets.json` and the frontend widget system.
 * Exposes plot defaults (course/elevation), gradient defaults, and
 * course dimensions fallback — types that are not standard metric
 * definitions and have their own rich configuration.
 *
 * @module standard-widgets
 */

import standardWidgetsManifest from '../../../assets/standard-widgets.json'

/**
 * Default position and geometry fields for a plot (course/elevation) widget.
 * @type {{x: number, y: number, rotation: number, completed_line_width: number, remaining_line_width: number}}
 */
export const PLOT_BASE_DEFAULTS = Object.freeze({ ...standardWidgetsManifest.plot.base })

/**
 * Fallback dimensions when course points data is unavailable.
 * @type {{width: number, height: number}}
 */
export const COURSE_DIMENSIONS_FALLBACK = Object.freeze({ ...standardWidgetsManifest.plot.courseDimensionsFallback })

/**
 * Default line/marker fields specific to course plot widgets.
 * @type {object}
 */
export const COURSE_PLOT_DEFAULTS = Object.freeze({ ...standardWidgetsManifest.plot.course })

/**
 * Default line/area/marker fields specific to elevation plot widgets.
 * @type {object}
 */
export const ELEVATION_PLOT_DEFAULTS = Object.freeze({ ...standardWidgetsManifest.plot.elevation })

/**
 * Default fields for gradient metric value widgets.
 * @type {object}
 */
export const GRADIENT_DEFAULTS = Object.freeze({ ...standardWidgetsManifest.gradient })
