/**
 * Template-wide metadata, defaults, and scene fields for serialization.
 * Widget contracts are compiled by widget-catalog.js.
 *
 * @module template-constants
 */

import standardTemplateManifest from '../../../../assets/standard-template.json'

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

/** Native scene presentation fields, independent of source timing and widget defaults. */
export const SCENE_PRESENTATION_KEYS = [...standardTemplateManifest.scenePresentationKeys]

/** Durable keys persisted on the scene config. */
export const SCENE_DURABLE_KEYS = ['width', 'height', 'fps', 'updateRate']
