import i18n from 'i18next'
import { initReactI18next } from 'react-i18next'
import { locales } from './locales.js'

const translationFiles = import.meta.glob('./locales/*-translation.json', { eager: true, import: 'default' })

const resources = Object.fromEntries(
  locales.map((locale) => {
    const translation = translationFiles[`./locales/${locale}-translation.json`]

    if (!translation) {
      throw new Error(`Missing translation catalog for locale: ${locale}`)
    }

    return [locale, { translation }]
  }),
)

i18n.use(initReactI18next).init({
  resources,
  lng: 'en',
  fallbackLng: 'en',
  supportedLngs: locales,
  defaultNS: 'translation',
  interpolation: {
    escapeValue: false,
  },
})

export default i18n
