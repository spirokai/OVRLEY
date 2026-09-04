import { useCallback, useEffect, useRef } from 'react'

/**
 * Finds the nearest scrolling ancestor for an element.
 *
 * @param {HTMLElement} element - Element whose scroll parent is needed.
 * @returns {HTMLElement} Nearest scrolling ancestor.
 */
function findScrollParent(element) {
  let parent = element.parentElement
  while (parent) {
    if (/(auto|scroll|overlay)/.test(window.getComputedStyle(parent).overflowY)) return parent
    parent = parent.parentElement
  }
  return document.scrollingElement || document.documentElement
}

/**
 * Calculates the scroll range after the accordion transition completes.
 *
 * @param {HTMLElement} scrollParent - Scroll container whose final range is needed.
 * @returns {number} Maximum scrollTop after the transition.
 */
function getFinalScrollLimit(scrollParent) {
  const finalScrollHeight = Array.from(scrollParent.querySelectorAll('[role="region"][data-state]')).reduce((height, content) => {
    const currentHeight = content.getBoundingClientRect().height
    const finalHeight = content.dataset.state === 'open' ? content.scrollHeight : 0
    return height + finalHeight - currentHeight
  }, scrollParent.scrollHeight)

  return Math.max(0, finalScrollHeight - scrollParent.clientHeight)
}

/**
 * Keeps an accordion item aligned while its content expands.
 *
 * @returns {{start: Function, stop: Function}} Accordion autoscroll controls.
 */
export function useAccordionAutoscroll() {
  const animationFrameRef = useRef(null)
  const activeWidgetRef = useRef(null)

  const cancel = useCallback(() => {
    if (animationFrameRef.current !== null) cancelAnimationFrame(animationFrameRef.current)
    animationFrameRef.current = null
  }, [])

  const stop = useCallback(
    (widgetId) => {
      if (activeWidgetRef.current !== widgetId) return
      cancel()
      activeWidgetRef.current = null
    },
    [cancel],
  )

  const start = useCallback(
    (widgetId, item) => {
      cancel()
      const scrollParent = findScrollParent(item)
      const finalScrollLimit = getFinalScrollLimit(scrollParent)
      activeWidgetRef.current = widgetId

      const alignItem = () => {
        const parentRect = scrollParent.getBoundingClientRect()
        const itemRect = item.getBoundingClientRect()
        const targetScrollTop = scrollParent.scrollTop + itemRect.top - parentRect.top - scrollParent.clientTop

        scrollParent.scrollTop = Math.min(Math.max(0, targetScrollTop), finalScrollLimit)
        animationFrameRef.current = requestAnimationFrame(alignItem)
      }

      alignItem()
    },
    [cancel],
  )

  useEffect(
    () => () => {
      cancel()
      activeWidgetRef.current = null
    },
    [cancel],
  )

  return { start, stop }
}
