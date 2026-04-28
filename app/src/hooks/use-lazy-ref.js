import * as React from 'react'

function useLazyRef(fn) {
  const ref = React.useRef(null)

  if (ref.current === null) {
    ref.current = fn()
  }

  return ref
}

export { useLazyRef }
