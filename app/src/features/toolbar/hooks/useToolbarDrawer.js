import { useEffect, useEffectEvent, useState } from 'react'
import { hasOpenPopup, matchKeyboardShortcut } from '@/lib/keyboard-shortcuts'
import { useDrawerPreference } from './useDrawerPreference'

/**
 * Owns transient presentation state for the toolbar drawer layout.
 *
 * @param {object} layout - Canonical layout store selection.
 * @returns {object} Render-ready toolbar drawer state and actions.
 */
export function useToolbarDrawer(layout) {
  const initialized = useDrawerPreference()
  const [allocationTransitioning, setAllocationTransitioning] = useState(false)
  const [drawerContentMounted, setDrawerContentMounted] = useState(layout.leftDrawerVisible)

  useEffect(() => {
    if (layout.leftDrawerVisible) setDrawerContentMounted(true)
  }, [layout.leftDrawerVisible])

  const onKeyDown = useEffectEvent((event) => {
    if (event.defaultPrevented) return

    const match = matchKeyboardShortcut(event, 'drawer')
    if (match?.commandId !== 'drawer.close' || !layout.leftDrawerVisible) return

    if (layout.leftDrawerPinned) {
      if (hasOpenPopup()) event.preventDefault()
      return
    }

    event.preventDefault()
    layout.dismissLeftDrawerOverlay()
  })

  useEffect(() => {
    if (typeof window === 'undefined') return undefined

    const handleKeyDown = (event) => onKeyDown(event)
    window.addEventListener('keydown', handleKeyDown, true)
    return () => window.removeEventListener('keydown', handleKeyDown, true)
  }, [])

  const setPinned = (pinned) => {
    setAllocationTransitioning(true)
    layout.setLeftDrawerPinned(pinned)
  }

  const handleAllocationTransitionEnd = (event) => {
    if (event.target !== event.currentTarget || event.propertyName !== 'grid-template-columns') return
    setAllocationTransitioning(false)
  }

  const handleDrawerTransitionEnd = (event) => {
    if (event.target !== event.currentTarget || event.propertyName !== 'transform' || layout.leftDrawerVisible) return
    setDrawerContentMounted(false)
  }

  return {
    activeTool: layout.activeLeftDrawerTool,
    allocationTransitioning,
    dismissOverlay: layout.dismissLeftDrawerOverlay,
    initialized,
    renderDrawerContent: layout.leftDrawerVisible || drawerContentMounted,
    pinned: layout.leftDrawerPinned,
    selectTool: layout.selectLeftDrawerTool,
    setPinned,
    visible: layout.leftDrawerVisible,
    handleAllocationTransitionEnd,
    handleDrawerTransitionEnd,
  }
}
