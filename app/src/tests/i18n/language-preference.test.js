import { beforeEach, describe, expect, test, vi } from 'vitest'
import i18n from '@/i18n/index.js'
import { changeLanguagePreference, hydrateLanguagePreference } from '@/i18n/language-preference'

const getPreference = vi.hoisted(() => vi.fn())
const setPreference = vi.hoisted(() => vi.fn())

vi.mock('@/lib/preferences-store', () => ({ getPreference, setPreference }))

describe('language preference', () => {
  beforeEach(async () => {
    vi.clearAllMocks()
    await i18n.changeLanguage('en')
  })

  test('hydrates lng before the application mounts', async () => {
    getPreference.mockResolvedValue('de')

    await hydrateLanguagePreference()

    expect(getPreference).toHaveBeenCalledWith('lng')
    expect(i18n.resolvedLanguage).toBe('de')
  })

  test('keeps the configured default when lng is absent', async () => {
    getPreference.mockResolvedValue(undefined)

    await hydrateLanguagePreference()

    expect(i18n.resolvedLanguage).toBe('en')
  })

  test('rejects malformed persisted locales', async () => {
    getPreference.mockResolvedValue('unsupported')

    await expect(hydrateLanguagePreference()).rejects.toThrow('Unsupported locale: unsupported')
  })

  test('changes and persists the selected locale', async () => {
    setPreference.mockResolvedValue(undefined)

    await changeLanguagePreference('fr')

    expect(i18n.resolvedLanguage).toBe('fr')
    expect(setPreference).toHaveBeenCalledWith('lng', 'fr')
  })
})
