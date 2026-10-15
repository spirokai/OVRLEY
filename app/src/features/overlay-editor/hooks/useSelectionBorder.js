import { useLayoutEffect, useState } from 'react'

/** @param {HTMLElement} container Scene container. @param {object[][]} borders Scene vertices per border. @returns {object|null} Screen borders. */
export function useSelectionBorder(container, borders) {
  const [border, setBorder] = useState(null)
  useLayoutEffect(() => {
    const viewport = container.closest('[data-overlay-viewport]')
    function updateBorder() {
      const bounds = container.getBoundingClientRect()
      const clip = viewport.getBoundingClientRect()
      setBorder({
        ...getPixelAlignedBorders(borders, bounds, { width: container.offsetWidth, height: container.offsetHeight }, window.devicePixelRatio),
        clipPath: `inset(${clip.top}px calc(100% - ${clip.right}px) calc(100% - ${clip.bottom}px) ${clip.left}px)`,
      })
    }
    updateBorder()
    window.addEventListener('scroll', updateBorder, true)
    window.addEventListener('resize', updateBorder)
    return () => {
      window.removeEventListener('scroll', updateBorder, true)
      window.removeEventListener('resize', updateBorder)
    }
  }, [container, borders])
  return border
}

function getPixelAlignedBorders(borders, bounds, sceneSize, pixelRatio) {
  const strokePixels = Math.max(1, Math.round(pixelRatio))
  const centerOffset = (strokePixels % 2) / 2
  const snap = (value) => (Math.round(value * pixelRatio - centerOffset) + centerOffset) / pixelRatio
  const polygons = borders.map((vertices) =>
    vertices
      .map(({ x, y }) => `${snap(bounds.left + (x * bounds.width) / sceneSize.width)},${snap(bounds.top + (y * bounds.height) / sceneSize.height)}`)
      .join(' '),
  )
  return { polygons, strokeWidth: strokePixels / pixelRatio }
}
