import { useLayoutEffect, useMemo, useState } from 'react'
import { projectSelectionBorders } from '../utils/selectionBorderGeometry'

/** @param {HTMLElement} container Scene container. @param {object[][]} borders Scene vertices per border. @param {number} zoom Moveable's inverse viewport scale. @returns {object|null} Screen borders. */
export function useSelectionBorder(container, borders, zoom) {
  const [viewportGeometry, setViewportGeometry] = useState(null)
  // Moving vertices only need projection. Measuring and setting state for each
  // new borders array would add layout reads and another React commit per render.
  useLayoutEffect(() => {
    const viewport = container.closest('[data-overlay-viewport]')
    function updateViewportGeometry() {
      const bounds = container.getBoundingClientRect()
      const clip = viewport.getBoundingClientRect()
      setViewportGeometry({
        bounds,
        sceneSize: { width: container.offsetWidth, height: container.offsetHeight },
        pixelRatio: window.devicePixelRatio,
        clipPath: `inset(${clip.top}px calc(100% - ${clip.right}px) calc(100% - ${clip.bottom}px) ${clip.left}px)`,
      })
    }
    updateViewportGeometry()
    const observer = new ResizeObserver(updateViewportGeometry)
    observer.observe(container)
    observer.observe(viewport)
    window.addEventListener('scroll', updateViewportGeometry, true)
    window.addEventListener('resize', updateViewportGeometry)
    return () => {
      observer.disconnect()
      window.removeEventListener('scroll', updateViewportGeometry, true)
      window.removeEventListener('resize', updateViewportGeometry)
    }
  }, [container, zoom])
  return useMemo(
    () =>
      viewportGeometry === null
        ? null
        : {
            ...projectSelectionBorders(borders, viewportGeometry.bounds, viewportGeometry.sceneSize, viewportGeometry.pixelRatio),
            clipPath: viewportGeometry.clipPath,
          },
    [borders, viewportGeometry],
  )
}
