import { describe, expect, test, vi, beforeEach } from 'vitest'

let createCachedPromise

beforeEach(async () => {
  vi.resetModules()
  const mod = await import('@/lib/cached-promise')
  createCachedPromise = mod.createCachedPromise
})

describe('createCachedPromise', () => {
  test('calls the underlying async function once on first invocation', async () => {
    const fn = vi.fn().mockResolvedValue('result')
    const cached = createCachedPromise(fn)

    const result = await cached()

    expect(result).toBe('result')
    expect(fn).toHaveBeenCalledTimes(1)
  })

  test('returns cached result on subsequent calls without calling fn again', async () => {
    const fn = vi.fn().mockResolvedValue('result')
    const cached = createCachedPromise(fn)

    await cached()
    const result = await cached()

    expect(result).toBe('result')
    expect(fn).toHaveBeenCalledTimes(1)
  })

  test('deduplicates concurrent calls — only one fn invocation while first is pending', async () => {
    let resolveFirst
    const firstPromise = new Promise((resolve) => {
      resolveFirst = resolve
    })
    const fn = vi.fn().mockReturnValueOnce(firstPromise)
    const cached = createCachedPromise(fn)

    const first = cached()
    const second = cached()

    resolveFirst('result')
    const [firstResult, secondResult] = await Promise.all([first, second])

    expect(firstResult).toBe('result')
    expect(secondResult).toBe('result')
    expect(fn).toHaveBeenCalledTimes(1)
  })

  test('retries on error — resets pending promise and calls fn again on next invocation', async () => {
    let callCount = 0
    const fn = vi.fn().mockImplementation(() => {
      callCount++
      if (callCount === 1) return Promise.reject(new Error('fail'))
      return Promise.resolve('success')
    })
    const cached = createCachedPromise(fn)

    await expect(cached()).rejects.toThrow('fail')
    const result = await cached()

    expect(result).toBe('success')
    expect(fn).toHaveBeenCalledTimes(2)
  })

  test('does not cache error results — only successful results are cached', async () => {
    const fn = vi.fn().mockRejectedValueOnce(new Error('fail')).mockResolvedValueOnce('recovered')
    const cached = createCachedPromise(fn)

    await expect(cached()).rejects.toThrow('fail')
    const result = await cached()

    expect(result).toBe('recovered')
  })
})
