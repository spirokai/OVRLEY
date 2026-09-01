import { describe, expect, test } from 'vitest'
import { DEFAULT_LOCALE, locales, localeValues, requireLocale } from '@/i18n/locales'

describe('locale registry', () => {
  test('is the canonical source for supported codes and native labels', () => {
    expect(DEFAULT_LOCALE).toBe('en')
    expect(locales).toContainEqual({ value: 'zh-CN', label: '简体中文' })
    expect(locales).toContainEqual({ value: 'de', label: 'Deutsch' })
    expect(locales).toContainEqual({ value: 'pt-BR', label: 'Português (Brasil)' })
    expect(localeValues).toEqual(locales.map(({ value }) => value))
    expect(new Set(localeValues).size).toBe(locales.length)
  })

  test('rejects unsupported locale codes', () => {
    expect(() => requireLocale('en-US')).toThrow('Unsupported locale: en-US')
  })
})
