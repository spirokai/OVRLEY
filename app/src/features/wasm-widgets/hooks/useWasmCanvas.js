import { useCallback, useRef } from 'react'

/** @param {object} props Widget identity, selection wrapper and canvas registration. @returns {object} Stable DOM refs. */
export function useWasmCanvas({ id, attach, attachMapLayer, registerNode, separateRaster }) {
  const target = useRef(null)
  const wrapperRef = useCallback(
    (node) => {
      target.current = node
      registerNode(node)
    },
    [registerNode],
  )
  const canvasRef = useCallback(
    (node) => {
      if (separateRaster) attach(id, node, target.current)
      else attach(id, node)
    },
    [attach, id, separateRaster],
  )
  const mapLayerRef = useCallback((node) => attachMapLayer(id, node), [attachMapLayer, id])
  return { wrapperRef, canvasRef, mapLayerRef }
}
