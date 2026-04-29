/**
 * Implements the use as ref hook and related behavior for the app.
 */

import * as React from 'react'

import { useIsomorphicLayoutEffect } from '@/hooks/use-isomorphic-layout-effect'

/**
 * Provides as ref state and actions.
 *
 * @param {*} props - Component props.
 * @returns {*} Result produced by the helper.
 */
function useAsRef(props) {
  const ref = React.useRef(props)

  useIsomorphicLayoutEffect(() => {
    ref.current = props
  })

  return ref
}

export { useAsRef }
