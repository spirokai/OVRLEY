import { useCallback } from 'react'

/** @param {object} props Widget identity and native canvas registration. @returns {Function} Canvas ref. */
export function useWasmCanvas({ id, attach, registerNode }) {
  return useCallback(
    (node) => {
      attach(id, node)
      registerNode(node)
    },
    [attach, id, registerNode],
  )
}
