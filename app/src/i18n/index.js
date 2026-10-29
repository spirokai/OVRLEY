import i18n from 'i18next'
import { initReactI18next } from 'react-i18next'
import { DEFAULT_LOCALE, locales, localeValues } from './locales.js'

const translationFiles = import.meta.glob('./locales/*-translation.json', { eager: true, import: 'default' })

const resources = Object.fromEntries(
  locales.map(({ value }) => {
    const translation = translationFiles[`./locales/${value}-translation.json`]

    if (!translation) {
      throw new Error(`Missing translation catalog for locale: ${value}`)
    }

    return [value, { translation }]
  }),
)

i18n.use(initReactI18next).init({
  resources,
  lng: DEFAULT_LOCALE,
  fallbackLng: DEFAULT_LOCALE,
  supportedLngs: localeValues,
  defaultNS: 'translation',
  interpolation: {
    escapeValue: false,
  },
})

/**
 * Translates canonical select-option definitions for presentation.
 *
 * @param {Array<{value: string, labelKey: string, defaultLabel: string}>} options - Canonical option definitions.
 * @param {import('i18next').TFunction} translate - Translation function.
 * @returns {Array<{value: string, label: string}>} Translated select options.
 */
export function translateOptions(options, translate) {
  return options.map(({ value, labelKey, defaultLabel }) => ({ value, label: translate(labelKey, defaultLabel) }))
}

/**
 * Formats tagged backend errors for display while leaving untagged messages unchanged.
 * @param {string} message Backend error or progress message.
 * @returns {string} User-facing message.
 */
export function translateBackendError(message) {
  const projectCode = message.match(/\[project_error:([a-z_]+)\]/)?.[1]
  const projectKeys = {
    archive_size: 'archiveTooLarge',
    invalid_archive: 'invalidArchive',
    raster_unavailable: 'rasterUnavailable',
    save_failed: 'saveFailed',
  }
  if (projectCode && projectKeys[projectCode]) return i18n.t(`projects.${projectKeys[projectCode]}`)
  const match = message.match(/\[raster_error:([a-z_]+):([^\]]+)\]/)
  if (!match) return message
  const keys = {
    invalid_config: 'invalidConfig',
    no_image: 'noImage',
    missing_resource: 'renderUnavailable',
    relative_path: 'relativePath',
    missing: 'missing',
    unreadable: 'unreadable',
    unsupported_type: 'unsupportedType',
    encoded_size: 'encodedSize',
    resolution: 'resolution',
    decode: 'decode',
    missing_asset: 'missingAsset',
    corrupt_asset: 'corruptAsset',
  }
  const key = keys[match[1]]
  if (!key) return message
  const translated = i18n.t(`raster.errors.${key}`, { id: match[2] })
  return key === 'noImage' || key === 'renderUnavailable' || match[2] === 'configuration' ? translated : `${match[2]}: ${translated}`
}

export default i18n
