import { useCallback, useEffect, useEffectEvent } from 'react'
import { useStore as useZustandStore } from 'zustand'
import { matchKeyboardShortcut } from '@/lib/keyboard-shortcuts'
import useStore from '@/store/useStore'
import { redoHistory, undoHistory } from '../undoHistory'

function isTextEditingElement(target) {
  if (!(target instanceof Element)) return false
  return Boolean(target.closest('input, textarea, [role="textbox"], [contenteditable="true"]'))
}

function hasOpenKeyboardOverlay() {
  if (typeof document === 'undefined') return false

  return Boolean(
    document.querySelector(
      '[data-slot="dialog-content"], [data-slot="select-content"], [data-slot="popover-content"], [data-testid="widget-drawer-backdrop"]',
    ),
  )
}

/**
 * Owns reactive undo/redo availability and global editor shortcuts.
 *
 * @param {object} options - Hook options.
 * @param {boolean} [options.disabled=false] - Whether history commands are temporarily unavailable.
 * @returns {{ canRedo: boolean, canUndo: boolean, redo: Function, undo: Function }} Undo/redo control model.
 */
export default function useUndoRedo({ disabled = false } = {}) {
  const hasPastStates = useZustandStore(useStore.temporal, (state) => state.pastStates.length > 0)
  const hasFutureStates = useZustandStore(useStore.temporal, (state) => state.futureStates.length > 0)

  const undo = useCallback(() => {
    if (disabled) return
    undoHistory(useStore)
  }, [disabled])

  const redo = useCallback(() => {
    if (disabled) return
    redoHistory(useStore)
  }, [disabled])

  const onKeyDown = useEffectEvent((event) => {
    if (disabled || event.defaultPrevented || event.repeat || event.altKey || isTextEditingElement(event.target) || hasOpenKeyboardOverlay()) {
      return
    }

    const match = matchKeyboardShortcut(event, 'history')
    if (match?.commandId === 'history.undo' && hasPastStates) {
      event.preventDefault()
      undo()
    } else if (match?.commandId === 'history.redo' && hasFutureStates) {
      event.preventDefault()
      redo()
    }
  })

  useEffect(() => {
    if (typeof window === 'undefined') return undefined

    const handleKeyDown = (event) => onKeyDown(event)
    window.addEventListener('keydown', handleKeyDown, true)
    return () => window.removeEventListener('keydown', handleKeyDown, true)
  }, [])

  return {
    canRedo: !disabled && hasFutureStates,
    canUndo: !disabled && hasPastStates,
    redo,
    undo,
  }
}
