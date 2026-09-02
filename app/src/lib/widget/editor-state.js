import { mergeSceneGlobalDefaults, normalizeTemplateConfig } from '@/lib/template/template-normalization'

/**
 * Creates the canonical durable state of the widget editor.
 *
 * @param {object} state
 * @param {object} state.config - Current committed widget configuration.
 * @param {object} state.globalDefaults - Current editor-wide widget defaults.
 * @returns {{ config: object, globalDefaults: object }} Canonical editor state.
 */
export function createDurableEditorState({ config, globalDefaults }) {
  const nextGlobalDefaults = mergeSceneGlobalDefaults(config.scene, globalDefaults)
  return {
    config: normalizeTemplateConfig(config, nextGlobalDefaults),
    globalDefaults: nextGlobalDefaults,
  }
}
