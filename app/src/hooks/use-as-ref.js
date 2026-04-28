import * as React from 'react'

import { useIsomorphicLayoutEffect } from '@/hooks/use-isomorphic-layout-effect'

function useAsRef(props) {
  const ref = React.useRef(props)

  useIsomorphicLayoutEffect(() => {
    ref.current = props
  })

  return ref
}

export { useAsRef }
