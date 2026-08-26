import { act, renderHook } from '@testing-library/react'
import { beforeEach, describe, expect, test, vi } from 'vitest'

const fileFromSelectedPath = vi.hoisted(() => vi.fn())
const importActivityFile = vi.hoisted(() => vi.fn())
const importCsvActivityPath = vi.hoisted(() => vi.fn())
const importVboActivityPath = vi.hoisted(() => vi.fn())
const openSinglePath = vi.hoisted(() => vi.fn())
const setErrorMessage = vi.hoisted(() => vi.fn())
const setProcessing = vi.hoisted(() => vi.fn())

vi.mock('@/api/backend', () => ({ hasTauriRuntime: () => true }))
vi.mock('@/hooks/useAppStoreSelectors', () => ({
  useActivityStore: () => ({ activityFilename: null, setErrorMessage, setProcessing }),
}))
vi.mock('@/lib/activity/import-activity', () => ({ default: importActivityFile, importCsvActivityPath, importVboActivityPath }))
vi.mock('@/lib/file-dialog', () => ({
  fileFromSelectedPath,
  openSinglePath,
  selectBrowserFile: vi.fn(),
}))
vi.mock('@/store/useStore', () => ({
  default: {
    getState: () => ({}),
    temporal: {
      getState: () => ({ isTracking: false }),
    },
  },
}))

describe('useActivityImport native picker boundary', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    vi.spyOn(console, 'error').mockImplementation(() => {})
    fileFromSelectedPath.mockResolvedValue(new File(['activity'], 'activity.gpx'))
  })

  test('passes a selected CSV path directly to the activity importer', async () => {
    openSinglePath.mockResolvedValue('C:\\activities\\sample Racebox.csv')
    const { default: useActivityImport } = await import('@/features/app-shell/hooks/useActivityImport')
    const { result } = renderHook(() => useActivityImport())

    await act(() => result.current.handleActivityFileOpen())

    expect(importCsvActivityPath.mock.calls[0][0]).toBe('C:\\activities\\sample Racebox.csv')
    expect(importActivityFile).not.toHaveBeenCalled()
    expect(fileFromSelectedPath).not.toHaveBeenCalled()
  })

  test('keeps browser-parsed native paths on the File adapter route', async () => {
    openSinglePath.mockResolvedValue('C:\\activities\\activity.gpx')
    const { default: useActivityImport } = await import('@/features/app-shell/hooks/useActivityImport')
    const { result } = renderHook(() => useActivityImport())

    await act(() => result.current.handleActivityFileOpen())

    expect(fileFromSelectedPath).toHaveBeenCalledWith('C:\\activities\\activity.gpx', 'activity')
    expect(importActivityFile.mock.calls[0][0]).toBeInstanceOf(File)
  })

  test('passes a selected VBO path directly to the native activity importer', async () => {
    openSinglePath.mockResolvedValue('C:\\activities\\session.vbo')
    const { default: useActivityImport } = await import('@/features/app-shell/hooks/useActivityImport')
    const { result } = renderHook(() => useActivityImport())

    await act(() => result.current.handleActivityFileOpen())

    expect(importVboActivityPath.mock.calls[0][0]).toBe('C:\\activities\\session.vbo')
    expect(importActivityFile).not.toHaveBeenCalled()
    expect(fileFromSelectedPath).not.toHaveBeenCalled()
  })

  test('reports native CSV structural errors through the existing activity error path', async () => {
    openSinglePath.mockResolvedValue('C:\\activities\\broken.csv')
    importCsvActivityPath.mockRejectedValue(new Error("CSV import 'broken.csv': CSV row 3 canonical time must not decrease"))
    const { default: useActivityImport } = await import('@/features/app-shell/hooks/useActivityImport')
    const { result } = renderHook(() => useActivityImport())

    await act(() => result.current.handleActivityFileOpen())

    expect(setErrorMessage).toHaveBeenCalledWith("Activity selection failed: CSV import 'broken.csv': CSV row 3 canonical time must not decrease")
  })

  test('routes a dropped native VBO path through the same import boundary', async () => {
    const { default: useActivityImport } = await import('@/features/app-shell/hooks/useActivityImport')
    const { result } = renderHook(() => useActivityImport())

    await act(() => result.current.handleActivityFilesDrop(['C:\\activities\\session.vbo']))

    expect(importVboActivityPath.mock.calls[0][0]).toBe('C:\\activities\\session.vbo')
    expect(importActivityFile).not.toHaveBeenCalled()
  })
})
