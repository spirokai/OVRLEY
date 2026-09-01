import i18n from 'i18next'
import { getPreference, setPreference } from '@/lib/preferences-store'
import { requireLocale } from './locales.js'

const LANGUAGE_PREFERENCE_KEY = 'lng'

/** Hydrates the optional persisted language before React mounts. */
export async function hydrateLanguagePreference() {
  const locale = await getPreference(LANGUAGE_PREFERENCE_KEY)
  if (locale === null || locale === undefined) return
  await i18n.changeLanguage(requireLocale(locale).value)
}

/**
 * Applies and persists a supported language.
 * @param {string} locale Locale code selected by the user.
 * @returns {Promise<void>}
 */
export async function changeLanguagePreference(locale) {
  const { value } = requireLocale(locale)
  await i18n.changeLanguage(value)
  await setPreference(LANGUAGE_PREFERENCE_KEY, value)
}
