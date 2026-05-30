import * as React from 'react'

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

export { useAsRef, useIsomorphicLayoutEffect, useLazyRef }
