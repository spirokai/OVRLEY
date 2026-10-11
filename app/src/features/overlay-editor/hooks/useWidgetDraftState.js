/**
 * Live widget draft state - temporary edits during drag/resize/scale/rotate.
 *
 * Maintains mutable refs for synchronous access during interaction callbacks
 * and publishes a rendering snapshot once per animation frame. Each draft contains
 * the live data patch shown by editors and, when a Moveable interaction is active, the live
 * DOM layout used by the canvas. Drafts are committed to config only when the
 * interaction ends.
 */

import { useState, useSyncExternalStore } from 'react'
import { clearLiveWidgetDraft, clearLiveWidgetDrafts } from '../utils/widgetDomHelpers'

function createWidgetLiveEdits() {
  const draftWidgetsRef = { current: {} }
  const widgetNodesRef = { current: {} }
  const widgetNodeCallbacks = new Map()
  const activeWidgetInteractionRef = { current: null }
  const listeners = new Set()
  let frame = null
  let snapshot = {
    liveWidgetDrafts: {},
    activeWidgetInteraction: null,
  }

  const notify = () => {
    listeners.forEach((listener) => listener())
  }

  const updateSnapshot = (nextSnapshot) => {
    if (frame !== null) {
      cancelAnimationFrame(frame)
      frame = null
    }
    snapshot = nextSnapshot
    notify()
  }

  const publishDrafts = () =>
    updateSnapshot({
      activeWidgetInteraction: activeWidgetInteractionRef.current,
      liveWidgetDrafts: { ...draftWidgetsRef.current },
    })

  const scheduleSnapshot = () => {
    if (frame !== null) return
    frame = requestAnimationFrame(() => {
      frame = null
      publishDrafts()
    })
  }

  const setWidgetNode = (widgetId, node) => {
    if (node && widgetNodesRef.current[widgetId] === node) return
    if (!node && !widgetNodesRef.current[widgetId]) return

    if (node) widgetNodesRef.current[widgetId] = node
    else delete widgetNodesRef.current[widgetId]
  }

  return {
    beginWidgetInteraction(widgetId, type) {
      draftWidgetsRef.current[widgetId] = {
        data: {},
        layout: null,
      }
      activeWidgetInteractionRef.current = { widgetId, type }
    },
    clearWidgetDraft(widgetId) {
      const hadDraft = Object.hasOwn(draftWidgetsRef.current, widgetId)
      clearLiveWidgetDraft(draftWidgetsRef, widgetId)
      if (!hadDraft && !Object.hasOwn(snapshot.liveWidgetDrafts, widgetId)) return

      updateSnapshot({
        ...snapshot,
        activeWidgetInteraction: activeWidgetInteractionRef.current?.widgetId === widgetId ? null : activeWidgetInteractionRef.current,
        liveWidgetDrafts: { ...draftWidgetsRef.current },
      })
    },
    clearWidgetDrafts(widgetIds) {
      const changed = widgetIds.some(
        (widgetId) => Object.hasOwn(draftWidgetsRef.current, widgetId) || Object.hasOwn(snapshot.liveWidgetDrafts, widgetId),
      )
      clearLiveWidgetDrafts(draftWidgetsRef, widgetIds)

      if (changed) {
        const activeWidgetInteraction = widgetIds.includes(activeWidgetInteractionRef.current?.widgetId) ? null : activeWidgetInteractionRef.current
        updateSnapshot({ ...snapshot, activeWidgetInteraction, liveWidgetDrafts: { ...draftWidgetsRef.current } })
      }
    },
    draftWidgetsRef,
    getSnapshot() {
      return snapshot
    },
    getWidgetNode(widgetId) {
      return widgetNodesRef.current[widgetId] ?? null
    },
    getWidgetNodeRef(widgetId) {
      const current = widgetNodeCallbacks.get(widgetId)
      if (current) return current

      const callback = (node) => {
        setWidgetNode(widgetId, node)
        if (node) widgetNodeCallbacks.set(widgetId, callback)
        else widgetNodeCallbacks.delete(widgetId)
      }
      return callback
    },
    resetWidgetDrafts() {
      draftWidgetsRef.current = {}
      activeWidgetInteractionRef.current = null
      updateSnapshot({ ...snapshot, activeWidgetInteraction: null, liveWidgetDrafts: {} })
    },
    setLiveWidgetDraft(widgetId, nextData, nextLayout) {
      const currentDraft = draftWidgetsRef.current[widgetId] || { data: {}, layout: null }
      const nextDraft = {
        data: {
          ...currentDraft.data,
          ...nextData,
        },
        layout: nextLayout === undefined ? currentDraft.layout : nextLayout,
      }
      draftWidgetsRef.current[widgetId] = nextDraft
      scheduleSnapshot()
    },
    setLiveWidgetDraftsBatch(nextDraftsById) {
      Object.entries(nextDraftsById).forEach(([widgetId, { data, layout }]) => {
        const currentDraft = draftWidgetsRef.current[widgetId] || { data: {}, layout: null }
        const nextDraft = {
          data: {
            ...currentDraft.data,
            ...data,
          },
          layout: layout === undefined ? currentDraft.layout : layout,
        }
        draftWidgetsRef.current[widgetId] = nextDraft
      })

      scheduleSnapshot()
    },
    endWidgetInteraction(widgetId) {
      if (activeWidgetInteractionRef.current?.widgetId !== widgetId) return
      activeWidgetInteractionRef.current = null
      updateSnapshot({ ...snapshot, activeWidgetInteraction: null, liveWidgetDrafts: { ...draftWidgetsRef.current } })
    },
    setWidgetNode,
    subscribe(listener) {
      listeners.add(listener)
      return () => {
        listeners.delete(listener)
        if (listeners.size === 0 && frame !== null) publishDrafts()
      }
    },
  }
}

export default function useWidgetDraftState() {
  const [controller] = useState(createWidgetLiveEdits)
  return controller
}

/**
 * Subscribes a component to a shared live-edit controller.
 *
 * @param {object} controller - Shared live-edit controller.
 * @returns {object} Controller methods and current render snapshot.
 */
export function useWidgetDraftView(controller) {
  const snapshot = useSyncExternalStore(controller.subscribe, controller.getSnapshot, controller.getSnapshot)
  return { ...controller, ...snapshot }
}
