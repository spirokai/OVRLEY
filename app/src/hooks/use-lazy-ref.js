/**
 * Implements the use lazy ref hook and related behavior for the app.
 */

import * as React from 'react'

/**
 * Provides lazy ref state and actions.
 *
 * @param {*} fn - Value for fn.
 * @returns {*} Result produced by the helper.
 */
function useLazyRef(fn) {
  const ref = React.useRef(null)

  if (ref.current === null) {
    ref.current = fn()
  }

  return ref
}

export { useLazyRef }
