import { readFile } from 'node:fs/promises'
import path from 'node:path'

import { describe, expect, test, vi, beforeEach } from 'vitest'

const finalizeActivity = vi.hoisted(() => vi.fn())
const parseCsvActivity = vi.hoisted(() => vi.fn())
const parseVboActivity = vi.hoisted(() => vi.fn())

vi.mock('@/api/backend', () => ({
  finalizeActivity,
  parseCsvActivity,
  parseVboActivity,
  writeParseDebugFile: vi.fn().mockResolvedValue('debug-path.json'),
  openVideo: vi.fn(),
}))

const fixtureDir = path.resolve('../src-tauri/ovrley_core/tests/fixtures/activity')
const minimalGpx = '<gpx><trk><trkseg><trkpt lat="47.1" lon="8.1"><ele>500</ele><time>2026-01-01T00:00:00Z</time></trkpt></trkseg></trk></gpx>'
const minimalSrt = ['1', '00:00:01,000 --> 00:00:01,033', '2025-07-23 10:21:40.000', '[latitude: 47.1] [longitude: 8.1]', ''].join('\n')

function storeActions() {
  return {
    activateActivityFile: vi.fn(),
    clearActivitySummary: vi.fn(),
    setActivitySource: vi.fn(),
    setEndSecond: vi.fn(),
    setFallbackDurationSeconds: vi.fn(),
    setSelectedSecond: vi.fn(),
    setStartSecond: vi.fn(),
  }
}

describe('import-activity store boundary', () => {
  beforeEach(() => {
    vi.resetModules()
    finalizeActivity.mockReset()
    parseCsvActivity.mockReset()
    parseVboActivity.mockReset()
    finalizeActivity.mockResolvedValue({
      parsed_activity: {
        metadata: {
          duration_seconds: 0,
        },
      },
    })
    parseCsvActivity.mockResolvedValue({
      parsed_activity: {
        metadata: {
          duration_seconds: 12.8,
        },
      },
    })
    parseVboActivity.mockResolvedValue({
      parsed_activity: {
        metadata: {
          duration_seconds: 4.2,
        },
      },
    })
  })

  test('saveFile is callable with optional store actions parameter', async () => {
    const { default: saveFile } = await import('@/lib/activity/import-activity')
    expect(typeof saveFile).toBe('function')
    expect(saveFile.length).toBe(2) // fileOrPath + optional storeActions
  })

  test('imports IGC files through the shared finalizer boundary', async () => {
    const { default: saveFile } = await import('@/lib/activity/import-activity')
    const text = await readFile(path.join(fixtureDir, '654G6NG1.IGC'), 'utf8')
    const file = new File([text], '654G6NG1.IGC')
    const store = storeActions()

    await saveFile(file, store)

    expect(finalizeActivity).toHaveBeenCalledTimes(1)
    const rawActivity = finalizeActivity.mock.calls[0][0]
    expect(rawActivity.file_name).toBe('654G6NG1.IGC')
    expect(rawActivity.file_format).toBe('igc')
    expect(rawActivity.raw_samples.length).toBeGreaterThan(0)
    expect(parseCsvActivity).not.toHaveBeenCalled()
    expect(store.setActivitySource).toHaveBeenCalledWith(null)
    expect(store.activateActivityFile).toHaveBeenCalledWith({
      metadata: {
        duration_seconds: 0,
      },
    })
  })

  test.each([
    ['gpx', async () => new File([minimalGpx], 'activity.gpx')],
    ['fit', async () => new File([await readFile(path.join(fixtureDir, 'test-ride.fit'))], 'activity.fit')],
    ['srt', async () => new File([minimalSrt], 'activity.srt')],
  ])('keeps %s imports on the existing RawActivity finalizer route', async (format, createFile) => {
    const { default: saveFile } = await import('@/lib/activity/import-activity')

    await saveFile(await createFile(), storeActions())

    expect(finalizeActivity.mock.calls[0][0].file_format).toBe(format)
    expect(parseCsvActivity).not.toHaveBeenCalled()
  })

  test('imports a native CSV path without creating a browser File or finalizing RawActivity', async () => {
    const { importCsvActivityPath } = await import('@/lib/activity/import-activity')
    const store = storeActions()

    await importCsvActivityPath('C:\\activities\\sample Racebox.csv', store)

    expect(parseCsvActivity).toHaveBeenCalledWith('C:\\activities\\sample Racebox.csv')
    expect(finalizeActivity).not.toHaveBeenCalled()
    expect(store.setActivitySource).toHaveBeenCalledWith({ kind: 'file', path: 'C:\\activities\\sample Racebox.csv' })
    expect(store.activateActivityFile).toHaveBeenCalledWith({
      metadata: {
        duration_seconds: 12.8,
      },
    })
    expect(store.setEndSecond).toHaveBeenCalledWith(12)
  })

  test('imports a native VBO path without creating a browser File or finalizing RawActivity', async () => {
    const { importVboActivityPath } = await import('@/lib/activity/import-activity')
    const store = storeActions()

    await importVboActivityPath('C:\\activities\\session.vbo', store)

    expect(parseVboActivity).toHaveBeenCalledWith('C:\\activities\\session.vbo')
    expect(finalizeActivity).not.toHaveBeenCalled()
    expect(store.setActivitySource).toHaveBeenCalledWith({ kind: 'file', path: 'C:\\activities\\session.vbo' })
    expect(store.activateActivityFile).toHaveBeenCalledWith({
      metadata: {
        duration_seconds: 4.2,
      },
    })
    expect(store.setEndSecond).toHaveBeenCalledWith(4)
  })
})
