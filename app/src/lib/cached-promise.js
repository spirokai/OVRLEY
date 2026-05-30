/**
 * Wraps an async function with caching and request deduplication.
 * On success the result is cached forever. On error the pending promise is
 * cleared so the next call retries.
 *
 * @param {function(): Promise<*>} fn — Async function whose result should be memoized.
 * @returns {function(): Promise<*>} A function that resolves with the cached result or invokes fn once.
 */
export function createCachedPromise(fn) {
  let cached = null
  let pending = null

  return async function cachedCall() {
    if (cached) {
      return cached
    }

    if (pending) {
      return pending
    }

    pending = fn()
      .then((result) => {
        cached = result
        return result
      })
      .catch((error) => {
        pending = null
        throw error
      })

    return pending
  }
}
