import { describe, expect, test, vi, beforeEach } from 'vitest'

vi.mock('@/api/backend', () => ({
  writeParseDebugFile: vi.fn().mockResolvedValue('debug-path.json'),
  openVideo: vi.fn(),
}))

describe('import-activity store boundary', () => {
  beforeEach(() => {
    vi.resetModules()
  })

  test('saveFile is callable with optional store actions parameter', async () => {
    const { default: saveFile } = await import('@/lib/activity/import-activity')
    expect(typeof saveFile).toBe('function')
    expect(saveFile.length).toBe(2) // fileOrPath + optional storeActions
  })
})
