/**
 * Implements the use isomorphic layout effect hook and related behavior for the app.
 */

import * as React from 'react'

const useIsomorphicLayoutEffect =
  typeof window !== 'undefined' ? React.useLayoutEffect : React.useEffect

export { useIsomorphicLayoutEffect }
