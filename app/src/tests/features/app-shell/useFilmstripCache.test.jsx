import { act, renderHook, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, test, vi } from 'vite-plus/test'
import * as backend from '@/api/backend'
import useFilmstripCache from '@/features/app-shell/hooks/useFilmstripCache'
import useStore from '@/store/useStore'

vi.mock('@/api/backend', () => ({
  clearFilmstripCache: vi.fn(),
  getFilmstripCache: vi.fn(),
  hasTauriRuntime: vi.fn(),
  setFilmstripCacheLimit: vi.fn(),
}))

const MIB = 1024 * 1024
const usage = (usedBytes, limitBytes) => ({ usedBytes, limitBytes, minLimitBytes: 128 * MIB, maxLimitBytes: 5120 * MIB, limitStepBytes: 128 * MIB })

describe('useFilmstripCache', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    vi.mocked(backend.hasTauriRuntime).mockReturnValue(true)
    vi.mocked(backend.getFilmstripCache).mockResolvedValue(usage(10 * MIB, 256 * MIB))
    useStore.setState(useStore.getInitialState(), true)
  })

  test('loads the usage, and applies a new limit and a clear', async () => {
    vi.mocked(backend.setFilmstripCacheLimit).mockResolvedValue(usage(10 * MIB, 128 * MIB))
    vi.mocked(backend.clearFilmstripCache).mockResolvedValue(usage(0, 128 * MIB))
    const { result } = renderHook(() => useFilmstripCache())
    await waitFor(() => expect(result.current.usage).toEqual(usage(10 * MIB, 256 * MIB)))

    act(() => result.current.setDraftLimitBytes(128 * MIB))
    expect(result.current.limitBytes).toBe(128 * MIB)
    await act(() => result.current.commitLimit(128 * MIB))
    expect(backend.setFilmstripCacheLimit).toHaveBeenCalledWith(128 * MIB)
    expect(result.current.usage.limitBytes).toBe(128 * MIB)

    await act(() => result.current.clear())
    expect(result.current.usage.usedBytes).toBe(0)
    expect(result.current.pending).toBe(false)
  })

  test('outside the desktop runtime there is no cache', () => {
    vi.mocked(backend.hasTauriRuntime).mockReturnValue(false)
    const { result } = renderHook(() => useFilmstripCache())

    expect(result.current.usage).toBeNull()
    expect(backend.getFilmstripCache).not.toHaveBeenCalled()
  })

  test('a failed command shows its error', async () => {
    vi.mocked(backend.clearFilmstripCache).mockRejectedValue(new Error('disk full'))
    const { result } = renderHook(() => useFilmstripCache())
    await waitFor(() => expect(result.current.usage).not.toBeNull())

    await act(() => result.current.clear())

    expect(useStore.getState().errorMessage).toBe('disk full')
  })
})
