import { describe, expect, test, beforeEach, afterEach } from 'vitest'

describe('hasTauriRuntime', () => {
  let hasTauriRuntime

  beforeEach(() => {
    delete window.__TAURI_INTERNALS__
  })

  afterEach(() => {
    delete window.__TAURI_INTERNALS__
  })

  test('returns false when window.__TAURI_INTERNALS__ is undefined', async () => {
    const mod = await import('@/api/backend')
    hasTauriRuntime = mod.hasTauriRuntime
    expect(hasTauriRuntime()).toBe(false)
  })

  test('returns true when window.__TAURI_INTERNALS__ is an object (typeof null === object)', async () => {
    window.__TAURI_INTERNALS__ = {}
    const mod = await import('@/api/backend')
    hasTauriRuntime = mod.hasTauriRuntime
    expect(hasTauriRuntime()).toBe(true)
  })
})
