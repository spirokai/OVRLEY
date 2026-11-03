import { describe, expect, test, vi, beforeEach } from 'vite-plus/test'

const parseCsvActivity = vi.hoisted(() => vi.fn())
const parseVboActivity = vi.hoisted(() => vi.fn())
const parseTcxActivity = vi.hoisted(() => vi.fn())
const parseFitActivity = vi.hoisted(() => vi.fn())
const parseGpxActivity = vi.hoisted(() => vi.fn())
const parseIgcActivity = vi.hoisted(() => vi.fn())
const parseSrtActivity = vi.hoisted(() => vi.fn())
const parseUddfActivity = vi.hoisted(() => vi.fn())
const parseSubsurfaceActivity = vi.hoisted(() => vi.fn())

vi.mock('@/api/backend', () => ({
  parseCsvActivity,
  parseVboActivity,
  parseTcxActivity,
  parseFitActivity,
  parseGpxActivity,
  parseIgcActivity,
  parseSrtActivity,
  parseUddfActivity,
  parseSubsurfaceActivity,
  openVideo: vi.fn(),
}))

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
    parseCsvActivity.mockReset()
    parseVboActivity.mockReset()
    parseTcxActivity.mockReset()
    parseFitActivity.mockReset()
    parseGpxActivity.mockReset()
    parseIgcActivity.mockReset()
    parseSrtActivity.mockReset()
    parseUddfActivity.mockReset()
    parseSubsurfaceActivity.mockReset()
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

  test('imports a native FIT path and activates the Rust-finalized activity', async () => {
    const { importActivityPath } = await import('@/lib/import-activity')
    const store = storeActions()
    const parsedActivity = { metadata: { duration_seconds: 5 } }
    parseFitActivity.mockResolvedValue({ parsed_activity: parsedActivity })

    await importActivityPath('C:\\activities\\ride.FIT', store)

    expect(parseFitActivity).toHaveBeenCalledWith('C:\\activities\\ride.FIT')
    expect(store.activateActivityFile).toHaveBeenCalledWith(parsedActivity)
    expect(store.setActivitySource).toHaveBeenCalledWith({ kind: 'file', path: 'C:\\activities\\ride.FIT' })
  })

  test('rejects a path without a native parser', async () => {
    const { parseActivityPath } = await import('@/lib/import-activity')

    await expect(parseActivityPath('C:\\activities\\notes.txt')).rejects.toThrow('Unsupported activity file format: notes.txt')
  })

  test('imports a native IGC path and activates the Rust-finalized activity', async () => {
    const { importActivityPath } = await import('@/lib/import-activity')
    const store = storeActions()
    const parsedActivity = { metadata: { duration_seconds: 5 } }
    parseIgcActivity.mockResolvedValue({ parsed_activity: parsedActivity })

    await importActivityPath('C:\\activities\\flight.IGC', store)

    expect(parseIgcActivity).toHaveBeenCalledWith('C:\\activities\\flight.IGC')
    expect(store.activateActivityFile).toHaveBeenCalledWith(parsedActivity)
    expect(store.setActivitySource).toHaveBeenCalledWith({ kind: 'file', path: 'C:\\activities\\flight.IGC' })
  })

  test('imports a native SRT path and activates the Rust-finalized activity', async () => {
    const { importActivityPath } = await import('@/lib/import-activity')
    const store = storeActions()
    const parsedActivity = { metadata: { duration_seconds: 5 } }
    parseSrtActivity.mockResolvedValue({ parsed_activity: parsedActivity })

    await importActivityPath('C:\\activities\\flight.SRT', store)

    expect(parseSrtActivity).toHaveBeenCalledWith('C:\\activities\\flight.SRT')
    expect(store.activateActivityFile).toHaveBeenCalledWith(parsedActivity)
    expect(store.setActivitySource).toHaveBeenCalledWith({ kind: 'file', path: 'C:\\activities\\flight.SRT' })
  })

  test('imports a native CSV path through its Rust parser', async () => {
    const { importActivityPath } = await import('@/lib/import-activity')
    const store = storeActions()

    await importActivityPath('C:\\activities\\sample Racebox.csv', store)

    expect(parseCsvActivity).toHaveBeenCalledWith('C:\\activities\\sample Racebox.csv')
    expect(store.setActivitySource).toHaveBeenCalledWith({ kind: 'file', path: 'C:\\activities\\sample Racebox.csv' })
    expect(store.activateActivityFile).toHaveBeenCalledWith({
      metadata: {
        duration_seconds: 12.8,
      },
    })
    expect(store.setEndSecond).toHaveBeenCalledWith(12)
  })

  test('imports a native TCX path and activates the Rust-finalized activity', async () => {
    const { importActivityPath } = await import('@/lib/import-activity')
    const store = storeActions()
    const parsedActivity = { metadata: { duration_seconds: 5 } }
    parseTcxActivity.mockResolvedValue({ parsed_activity: parsedActivity })

    await importActivityPath('C:\\activities\\ride.TCX', store)

    expect(parseTcxActivity).toHaveBeenCalledWith('C:\\activities\\ride.TCX')
    expect(store.activateActivityFile).toHaveBeenCalledWith(parsedActivity)
    expect(store.setActivitySource).toHaveBeenCalledWith({ kind: 'file', path: 'C:\\activities\\ride.TCX' })
  })

  test('imports a native GPX path and activates the Rust-finalized activity', async () => {
    const { importActivityPath } = await import('@/lib/import-activity')
    const store = storeActions()
    const parsedActivity = { metadata: { duration_seconds: 5 } }
    parseGpxActivity.mockResolvedValue({ parsed_activity: parsedActivity })

    await importActivityPath('C:\\activities\\ride.GPX', store)

    expect(parseGpxActivity).toHaveBeenCalledWith('C:\\activities\\ride.GPX')
    expect(store.activateActivityFile).toHaveBeenCalledWith(parsedActivity)
    expect(store.setActivitySource).toHaveBeenCalledWith({ kind: 'file', path: 'C:\\activities\\ride.GPX' })
  })

  test.each([
    ['dive.UDDF', parseUddfActivity],
    ['dive.SSRF', parseSubsurfaceActivity],
    ['dive.XML', parseSubsurfaceActivity],
  ])('imports %s through its native Rust parser', async (filename, parser) => {
    const { importActivityPath } = await import('@/lib/import-activity')
    const store = storeActions()
    const parsedActivity = { metadata: { duration_seconds: 60 } }
    parser.mockResolvedValue({ parsed_activity: parsedActivity })
    const path = `C:\\activities\\${filename}`

    await importActivityPath(path, store)

    expect(parser).toHaveBeenCalledWith(path)
    expect(store.activateActivityFile).toHaveBeenCalledWith(parsedActivity)
    expect(store.setActivitySource).toHaveBeenCalledWith({ kind: 'file', path })
  })

  test('imports a native VBO path through its Rust parser', async () => {
    const { importActivityPath } = await import('@/lib/import-activity')
    const store = storeActions()

    await importActivityPath('C:\\activities\\session.vbo', store)

    expect(parseVboActivity).toHaveBeenCalledWith('C:\\activities\\session.vbo')
    expect(store.setActivitySource).toHaveBeenCalledWith({ kind: 'file', path: 'C:\\activities\\session.vbo' })
    expect(store.activateActivityFile).toHaveBeenCalledWith({
      metadata: {
        duration_seconds: 4.2,
      },
    })
    expect(store.setEndSecond).toHaveBeenCalledWith(4)
  })
})
