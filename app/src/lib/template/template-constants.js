/**
 * Template constants — all key lists and default values for template
 * serialization, normalization, and durable state.
 *
 * Key lists are derived from the canonical manifests. Adding a field to a
 * manifest automatically includes it during normalization. The only manual
 * additions are `id` (identity), `value` (widget type), and `display_variants`
 * (empty-by-default container).
 *
 * @module template-constants
 */

import standardTemplateManifest from '../../../../assets/standard-template.json'
import {
  TEXT_LABEL_DEFAULTS,
  TEXT_DEFAULTS,
  COURSE_PLOT_DEFAULTS,
  ELEVATION_PLOT_DEFAULTS,
  DISPLAY_TYPE_DEFINITIONS,
  BACKDROP_CIRCLE_DEFAULTS,
  BACKDROP_RECTANGLE_DEFAULTS,
} from '../widget/standard-widgets'

// ---------------------------------------------------------------------------
// Template metadata / defaults
// ---------------------------------------------------------------------------

/** File format identifier for OVRLEY template files. */
export const TEMPLATE_FILE_FORMAT = standardTemplateManifest.fileFormat

/** Current template file format version. */
export const TEMPLATE_FILE_VERSION = standardTemplateManifest.fileVersion

/** Default export range when no custom range has been configured. */
export const DEFAULT_EXPORT_RANGE = Object.freeze({ ...standardTemplateManifest.exportRange })

/** Default values for global scene/template settings. */
export const DEFAULT_GLOBAL_DEFAULTS = Object.freeze({ ...standardTemplateManifest.globals })

// ---------------------------------------------------------------------------
// Normalization key lists (derived from manifests)
// ---------------------------------------------------------------------------

/** Scene keys that are render-time-only artifacts, stripped during normalization. */
export const SCENE_RENDER_TIME_ONLY_KEYS = [...standardTemplateManifest.renderTimeOnlyKeys]

/** Durable keys persisted on the scene config. */
export const SCENE_DURABLE_KEYS = ['width', 'height', 'fps', 'updateRate']

/** Keys preserved when normalizing a label widget. */
export const LABEL_KEYS = [...Object.keys(TEXT_LABEL_DEFAULTS), 'id']

/** Shared keys preserved when normalizing a backdrop widget. */
export const BACKDROP_SHARED_KEYS = [
  'id',
  'x',
  'y',
  'opacity',
  'display_type',
  'fill_color',
  'fill_opacity',
  'border_thickness',
  'border_color',
  'border_opacity',
]

/** Keys preserved when normalizing a metric value widget. */
export const VALUE_SHARED_KEYS = [...Object.keys(TEXT_DEFAULTS), 'id', 'value', 'display_variants']

/** Flat fields owned by the lap timer display contract. */
export const LAP_TIMER_KEYS = [...Object.keys(DISPLAY_TYPE_DEFINITIONS.lap_timer.defaults), ...DISPLAY_TYPE_DEFINITIONS.lap_timer.configFields]

/** Keys preserved when normalizing a course plot widget. */
export const COURSE_PLOT_KEYS = [...Object.keys(COURSE_PLOT_DEFAULTS), 'id']

/** Keys preserved when normalizing an elevation plot widget. */
export const ELEVATION_PLOT_KEYS = [...Object.keys(ELEVATION_PLOT_DEFAULTS), 'id']

/** Keys that are structural/identity and never belong in per-display variants. */
const DISPLAY_VARIANT_EXCLUDED_KEYS = new Set(['id', 'value', 'display_variants', 'x', 'y', 'display_type'])

/** Allowed keys for display variant configs during normalization. */
const DISPLAY_VARIANT_FRAME_KEYS = ['width', 'height', 'rotation']
const DISPLAY_VARIANT_FRAME_KEYS_BY_TYPE = {
  lean_angle: ['rotation'],
}

export const DISPLAY_VARIANT_KEYS = Object.freeze(
  Object.fromEntries(
    Object.entries(DISPLAY_TYPE_DEFINITIONS)
      .filter(([, definition]) => definition.layoutMode === 'boxed')
      .map(([displayType, definition]) => [
        displayType,
        [
          ...(DISPLAY_VARIANT_FRAME_KEYS_BY_TYPE[displayType] || DISPLAY_VARIANT_FRAME_KEYS),
          ...Object.keys(definition.defaults || {}).filter((key) => !DISPLAY_VARIANT_EXCLUDED_KEYS.has(key)),
          ...(definition.conditionalKeys || []),
        ],
      ]),
  ),
)

export const BACKDROP_VARIANT_KEYS = Object.freeze({
  circle: Object.keys(BACKDROP_CIRCLE_DEFAULTS).filter((key) => !BACKDROP_SHARED_KEYS.includes(key)),
  rectangle: Object.keys(BACKDROP_RECTANGLE_DEFAULTS).filter((key) => !BACKDROP_SHARED_KEYS.includes(key)),
})
