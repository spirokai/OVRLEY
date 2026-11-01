import { readFile } from 'node:fs/promises'
import path from 'node:path'

import { describe, expect, test, vi, beforeEach } from 'vite-plus/test'

const finalizeActivity = vi.hoisted(() => vi.fn())
const parseCsvActivity = vi.hoisted(() => vi.fn())
const parseVboActivity = vi.hoisted(() => vi.fn())
const parseTcxActivity = vi.hoisted(() => vi.fn())
const parseGpxActivity = vi.hoisted(() => vi.fn())
const parseIgcActivity = vi.hoisted(() => vi.fn())
const parseSrtActivity = vi.hoisted(() => vi.fn())
const parseUddfActivity = vi.hoisted(() => vi.fn())
const parseSubsurfaceActivity = vi.hoisted(() => vi.fn())

vi.mock('@/api/backend', () => ({
  finalizeActivity,
  parseCsvActivity,
  parseVboActivity,
  parseTcxActivity,
  parseGpxActivity,
  parseIgcActivity,
  parseSrtActivity,
  parseUddfActivity,
  parseSubsurfaceActivity,
  writeParseDebugFile: vi.fn().mockResolvedValue('debug-path.json'),
  openVideo: vi.fn(),
}))

const fixtureDir = path.resolve('../src-tauri/ovrley_core/tests/fixtures/activity')

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
    parseTcxActivity.mockReset()
    parseGpxActivity.mockReset()
    parseIgcActivity.mockReset()
    parseSrtActivity.mockReset()
    parseUddfActivity.mockReset()
    parseSubsurfaceActivity.mockReset()
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

  test('imports a native IGC path and activates the Rust-finalized activity', async () => {
    const { importActivityPath } = await import('@/lib/activity/import-activity')
    const store = storeActions()
    const parsedActivity = { metadata: { duration_seconds: 5 } }
    parseIgcActivity.mockResolvedValue({ parsed_activity: parsedActivity })

    await importActivityPath('C:\\activities\\flight.IGC', store)

    expect(parseIgcActivity).toHaveBeenCalledWith('C:\\activities\\flight.IGC')
    expect(finalizeActivity).not.toHaveBeenCalled()
    expect(store.activateActivityFile).toHaveBeenCalledWith(parsedActivity)
    expect(store.setActivitySource).toHaveBeenCalledWith({ kind: 'file', path: 'C:\\activities\\flight.IGC' })
  })

  test('imports a native SRT path and activates the Rust-finalized activity', async () => {
    const { importActivityPath } = await import('@/lib/activity/import-activity')
    const store = storeActions()
    const parsedActivity = { metadata: { duration_seconds: 5 } }
    parseSrtActivity.mockResolvedValue({ parsed_activity: parsedActivity })

    await importActivityPath('C:\\activities\\flight.SRT', store)

    expect(parseSrtActivity).toHaveBeenCalledWith('C:\\activities\\flight.SRT')
    expect(finalizeActivity).not.toHaveBeenCalled()
    expect(store.activateActivityFile).toHaveBeenCalledWith(parsedActivity)
    expect(store.setActivitySource).toHaveBeenCalledWith({ kind: 'file', path: 'C:\\activities\\flight.SRT' })
  })

  test('keeps fit imports on the existing RawActivity finalizer route', async () => {
    const { default: saveFile } = await import('@/lib/activity/import-activity')
    const file = new File([await readFile(path.join(fixtureDir, 'test-ride.fit'))], 'activity.fit')

    await saveFile(file, storeActions())

    expect(finalizeActivity.mock.calls[0][0].file_format).toBe('fit')
    expect(parseCsvActivity).not.toHaveBeenCalled()
  })

  test('passes FIT lap boundaries to the shared finalizer', async () => {
    const { parseActivityFile } = await import('@/lib/activity/import-activity')
    const bytes = await readFile(path.join(fixtureDir, 'test-ride.fit'))

    await parseActivityFile(new File([bytes], 'laps.fit'))

    expect(finalizeActivity).toHaveBeenCalledTimes(1)
    const rawActivity = finalizeActivity.mock.calls[0][0]
    expect(rawActivity.sync_time).toBe('2026-05-02T17:00:18.000Z')
    expect(rawActivity.lap_markers.BeaconMarkers).toHaveLength(15)
    expect(rawActivity.lap_markers.BeaconMarkers.slice(0, 3)).toEqual([0, 1248, 1298])
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

  test('imports a native TCX path and activates the Rust-finalized activity', async () => {
    const { importActivityPath } = await import('@/lib/activity/import-activity')
    const store = storeActions()
    const parsedActivity = { metadata: { duration_seconds: 5 } }
    parseTcxActivity.mockResolvedValue({ parsed_activity: parsedActivity })

    await importActivityPath('C:\\activities\\ride.TCX', store)

    expect(parseTcxActivity).toHaveBeenCalledWith('C:\\activities\\ride.TCX')
    expect(finalizeActivity).not.toHaveBeenCalled()
    expect(store.activateActivityFile).toHaveBeenCalledWith(parsedActivity)
    expect(store.setActivitySource).toHaveBeenCalledWith({ kind: 'file', path: 'C:\\activities\\ride.TCX' })
  })

  test('imports a native GPX path and activates the Rust-finalized activity', async () => {
    const { importActivityPath } = await import('@/lib/activity/import-activity')
    const store = storeActions()
    const parsedActivity = { metadata: { duration_seconds: 5 } }
    parseGpxActivity.mockResolvedValue({ parsed_activity: parsedActivity })

    await importActivityPath('C:\\activities\\ride.GPX', store)

    expect(parseGpxActivity).toHaveBeenCalledWith('C:\\activities\\ride.GPX')
    expect(finalizeActivity).not.toHaveBeenCalled()
    expect(store.activateActivityFile).toHaveBeenCalledWith(parsedActivity)
    expect(store.setActivitySource).toHaveBeenCalledWith({ kind: 'file', path: 'C:\\activities\\ride.GPX' })
  })

  test.each([
    ['dive.UDDF', parseUddfActivity],
    ['dive.SSRF', parseSubsurfaceActivity],
    ['dive.XML', parseSubsurfaceActivity],
  ])('imports %s through its native Rust parser', async (filename, parser) => {
    const { importActivityPath } = await import('@/lib/activity/import-activity')
    const store = storeActions()
    const parsedActivity = { metadata: { duration_seconds: 60 } }
    parser.mockResolvedValue({ parsed_activity: parsedActivity })
    const path = `C:\\activities\\${filename}`

    await importActivityPath(path, store)

    expect(parser).toHaveBeenCalledWith(path)
    expect(finalizeActivity).not.toHaveBeenCalled()
    expect(store.activateActivityFile).toHaveBeenCalledWith(parsedActivity)
    expect(store.setActivitySource).toHaveBeenCalledWith({ kind: 'file', path })
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
