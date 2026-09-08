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

export default i18n
