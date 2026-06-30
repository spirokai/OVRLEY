/**
 * Regression tests for preview performance diagnostics.
 *
 * Preview perf counters are useful during development, but the enable/disable
 * switch must no longer persist across app restarts. These tests lock in the
 * new rule: old browser-storage flags are ignored and diagnostics remain
 * disabled unless explicitly enabled for the current session.
 */

import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'

/**
 * Loads a fresh copy of the preview perf module.
 *
 * The module owns timer and counter state, so each spec needs a clean module
 * instance instead of reusing state from a previous test.
 *
 * @returns {Promise<typeof import('@/lib/previewPerf')>} Fresh preview perf module.
 */
async function loadPreviewPerfModule() {
  vi.resetModules()
  return import('@/lib/previewPerf')
}

describe('previewPerf', () => {
  beforeEach(() => {
    vi.useFakeTimers()
    localStorage.clear()
    delete window.__OVRLEY_PREVIEW_PERF__
    delete window.__OVRLEY_PREVIEW_PERF_ENABLED__
    vi.spyOn(console, 'info').mockImplementation(() => {})
  })

  afterEach(() => {
    vi.useRealTimers()
    vi.restoreAllMocks()
    localStorage.clear()
    delete window.__OVRLEY_PREVIEW_PERF__
    delete window.__OVRLEY_PREVIEW_PERF_ENABLED__
  })

  test('ignores legacy browser-storage flags and stays disabled by default', async () => {
    localStorage.setItem('ovrley:preview-perf', 'true')
    const getItemSpy = vi.spyOn(Storage.prototype, 'getItem')
    const { incrementPreviewPerfCounter, previewPerfCounterName, setPreviewPerfValue } = await loadPreviewPerfModule()

    incrementPreviewPerfCounter(previewPerfCounterName('video frame callbacks'))
    setPreviewPerfValue('preview clock mode', 'auto')
    vi.advanceTimersByTime(1000)

    expect(getItemSpy).not.toHaveBeenCalled()
    expect(window.__OVRLEY_PREVIEW_PERF__).toBeUndefined()
  })

  test('publishes counters when the current session explicitly enables preview perf', async () => {
    window.__OVRLEY_PREVIEW_PERF_ENABLED__ = true
    const { incrementPreviewPerfCounter, previewPerfCounterName, setPreviewPerfValue } = await loadPreviewPerfModule()

    incrementPreviewPerfCounter(previewPerfCounterName('video frame callbacks'), 2)
    setPreviewPerfValue('preview clock mode', 'raf')
    vi.advanceTimersByTime(1000)

    expect(window.__OVRLEY_PREVIEW_PERF__).toEqual({
      'preview clock mode': 'raf',
      'video frame callbacks/s': 2,
    })
  })
})
