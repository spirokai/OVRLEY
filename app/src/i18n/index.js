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

export default i18n
