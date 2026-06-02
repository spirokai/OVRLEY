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

import standardTemplateManifest from '../../../assets/standard-template.json'
import { TEXT_LABEL_DEFAULTS, TEXT_DEFAULTS, COURSE_PLOT_DEFAULTS, ELEVATION_PLOT_DEFAULTS, HEADING_TAPE_DEFAULTS } from '@/lib/standard-widgets'

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

/** Keys preserved when normalizing a metric value widget. */
export const VALUE_SHARED_KEYS = [...Object.keys(TEXT_DEFAULTS), 'id', 'value', 'display_variants']

/** Keys preserved when normalizing a course plot widget. */
export const COURSE_PLOT_KEYS = [...Object.keys(COURSE_PLOT_DEFAULTS), 'id']

/** Keys preserved when normalizing an elevation plot widget. */
export const ELEVATION_PLOT_KEYS = [...Object.keys(ELEVATION_PLOT_DEFAULTS), 'id']

/** Allowed keys for display variant configs during normalization. */
const DISPLAY_VARIANT_FRAME_KEYS = ['width', 'height', 'rotation']

export const DISPLAY_VARIANT_KEYS = {
  heading_tape: [...DISPLAY_VARIANT_FRAME_KEYS, ...Object.keys(HEADING_TAPE_DEFAULTS)],
}
