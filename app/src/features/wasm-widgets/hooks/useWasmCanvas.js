import { useCallback } from 'react'

/** @param {object} props Widget identity and native canvas registration. @returns {Function} Canvas ref. */
export function useWasmCanvas({ id, attach }) {
  return useCallback(
    (node) => {
      attach(id, node)
    },
    [attach, id],
  )
}
