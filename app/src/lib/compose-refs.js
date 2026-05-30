/**
 * Provides shared compose refs utilities for the app.
 */

/* eslint-disable react-hooks/exhaustive-deps */

import * as React from 'react'

/**
 * Sets ref.
 *
 * @param {React.Ref<*>} ref - Forwarded React ref.
 * @param {*} value - Input value processed by the helper.
 * @returns {*} Result produced by the helper.
 */
function setRef(ref, value) {
  if (typeof ref === 'function') {
    return ref(value)
  }

  if (ref !== null && ref !== undefined) {
    ref.current = value
  }
}

/**
 * Handles compose refs.
 *
 * @param {*} refs - Value for refs.
 * @returns {*} Result produced by the helper.
 */
function composeRefs(...refs) {
  return (node) => {
    let hasCleanup = false
    const cleanups = refs.map((ref) => {
      const cleanup = setRef(ref, node)
      if (!hasCleanup && typeof cleanup === 'function') {
        hasCleanup = true
      }
      return cleanup
    })

    // React <19 will log an error to the console if a callback ref returns a
    // value. We don't use ref cleanups internally so this will only happen if a
    // user's ref callback returns a value, which we only expect if they are
    // using the cleanup functionality added in React 19.
    if (hasCleanup) {
      return () => {
        for (let i = 0; i < cleanups.length; i++) {
          const cleanup = cleanups[i]
          if (typeof cleanup === 'function') {
            cleanup()
          } else {
            setRef(refs[i], null)
          }
        }
      }
    }
  }
}

/**
 * Provides composed refs state and actions.
 *
 * @param {*} refs - Value for refs.
 * @returns {*} Result produced by the helper.
 */
function useComposedRefs(...refs) {
  // biome-ignore lint/correctness/useExhaustiveDependencies: we want to memoize by all values
  return React.useCallback(composeRefs(...refs), refs)
}

const useIsomorphicLayoutEffect = typeof window !== 'undefined' ? React.useLayoutEffect : React.useEffect

function useAsRef(props) {
  const ref = React.useRef(props)

  useIsomorphicLayoutEffect(() => {
    ref.current = props
  })

  return ref
}

function useLazyRef(fn) {
  const ref = React.useRef(null)

  if (ref.current === null) {
    ref.current = fn()
  }

  return ref
}

export { useAsRef, useComposedRefs, useIsomorphicLayoutEffect, useLazyRef }
