import { act, renderHook, waitFor } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { useWasmPreviewDebug } from '@/features/wasm-preview-debug/hooks/useWasmPreviewDebug'
import { createWasmRenderer } from '@/features/wasm-preview-debug/utils/wasmRenderer'

vi.mock('@/features/wasm-preview-debug/utils/wasmRenderer', () => ({ createWasmRenderer: vi.fn() }))

afterEach(() => {
  vi.restoreAllMocks()
  vi.clearAllMocks()
})

describe('Wasm debug lifecycle', () => {
  it('disposes a renderer that finishes loading after unmount', async () => {
    let complete
    createWasmRenderer.mockReturnValue(
      new Promise((resolve) => {
        complete = resolve
      }),
    )
    const renderer = { render: vi.fn(), dispose: vi.fn() }
    const { unmount } = renderHook(useWasmPreviewDebug)
    unmount()
    await act(async () => complete(renderer))
    expect(renderer.dispose).toHaveBeenCalledOnce()
    expect(renderer.render).not.toHaveBeenCalled()
  })

  it('deduplicates frame indices and cancels animation on cleanup', async () => {
    let tick
    vi.spyOn(window, 'requestAnimationFrame').mockImplementation((callback) => {
      tick = callback
      return 42
    })
    const cancel = vi.spyOn(window, 'cancelAnimationFrame').mockImplementation(() => {})
    const renderer = { render: vi.fn(() => ({ draw: 1, copy: 2, present: 3, total: 6 })), dispose: vi.fn() }
    createWasmRenderer.mockResolvedValue(renderer)
    const { result, unmount } = renderHook(useWasmPreviewDebug)
    await waitFor(() => expect(result.current.status).toContain('Running'))
    act(() => {
      tick(100)
      tick(101)
      tick(117)
    })
    expect(renderer.render.mock.calls).toEqual([[0], [1]])
    unmount()
    expect(cancel).toHaveBeenCalledWith(42)
    expect(renderer.dispose).toHaveBeenCalledOnce()
  })

  it('surfaces initialization failures', async () => {
    createWasmRenderer.mockRejectedValue(new Error('Skia rejected the font.'))
    const { result } = renderHook(useWasmPreviewDebug)
    await waitFor(() => expect(result.current.status).toBe('Skia rejected the font.'))
  })
})
