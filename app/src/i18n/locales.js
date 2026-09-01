export const DEFAULT_LOCALE = 'en'

export const locales = [
  { value: 'en', label: 'English' },
  { value: 'zh-CN', label: '简体中文' },
  { value: 'ja', label: '日本語' },
  { value: 'de', label: 'Deutsch' },
  { value: 'fr', label: 'Français' },
  { value: 'tr', label: 'Türkçe' },
  { value: 'it', label: 'Italiano' },
  { value: 'ko', label: '한국어' },
  { value: 'es', label: 'Español' },
  { value: 'ru', label: 'Русский' },
  { value: 'pl', label: 'Polski' },
  { value: 'pt-BR', label: 'Português (Brasil)' },
  { value: 'nl', label: 'Nederlands' },
  { value: 'cs', label: 'Čeština' },
  { value: 'sv', label: 'Svenska' },
]

export const localeValues = locales.map(({ value }) => value)

/**
 * Resolves a supported locale through the canonical locale registry.
 * @param {string} value Locale value to resolve.
 * @returns {{value: string, label: string}} Canonical locale definition.
 */
export function requireLocale(value) {
  const locale = locales.find((candidate) => candidate.value === value)
  if (!locale) {
    throw new Error(`Unsupported locale: ${String(value)}`)
  }
  return locale
}
