/**
 * Shared inspector and global default patches, published once per animation frame.
 * Canvas gestures publish separately: the WASM preview already presents them, so only
 * the open inspector subscribes, and the editor and sidebars do not re-render per frame.
 * Both channels contain only canonical widget data.
 */

import { useMemo, useState, useSyncExternalStore } from 'react'
import { applyWidgetPatch, mergeWidgetData } from '@/lib/widget/widget-resolver'

function notify(state) {
  for (const listener of state.listeners) listener()
}

function publishDrafts(state) {
  if (state.frame !== null) cancelAnimationFrame(state.frame)
  state.frame = null
  // Global default drafts are replaced, never mutated, so widget-only publishes keep their identity.
  state.snapshot = { ...state.snapshot, liveWidgetDrafts: { ...state.drafts }, liveGlobalDefaults: state.globalDefaults }
  notify(state)
}

function scheduleDrafts(state) {
  if (state.frame === null) state.frame = requestAnimationFrame(publishDrafts.bind(null, state))
}

function mergeDraft(state, widgetId, updates) {
  state.drafts[widgetId] = mergeWidgetData(state.drafts[widgetId], updates)
}

function setLiveWidgetDraft(state, widgetId, updates) {
  mergeDraft(state, widgetId, updates)
  scheduleDrafts(state)
}

function setLiveWidgetDrafts(state, updatesById) {
  for (const [widgetId, updates] of Object.entries(updatesById)) mergeDraft(state, widgetId, updates)
  scheduleDrafts(state)
}

function clearWidgetDrafts(state, widgetIds) {
  let changed = false
  for (const widgetId of widgetIds) {
    changed ||= Object.hasOwn(state.drafts, widgetId) || Object.hasOwn(state.snapshot.liveWidgetDrafts, widgetId)
    delete state.drafts[widgetId]
  }
  if (changed) publishDrafts(state)
}

function clearWidgetDraft(state, widgetId) {
  clearWidgetDrafts(state, [widgetId])
}

function resetWidgetDrafts(state) {
  state.drafts = {}
  publishDrafts(state)
}

function setLiveGlobalDefault(state, key, value) {
  state.globalDefaults = { ...state.globalDefaults, [key]: value }
  scheduleDrafts(state)
}

function clearGlobalDefaultDrafts(state) {
  if (Object.keys(state.globalDefaults).length === 0) return
  state.globalDefaults = {}
  publishDrafts(state)
}

function getGlobalDefaultDrafts(state) {
  return state.globalDefaults
}

function getWidgetDraft(state, widgetId) {
  return state.drafts[widgetId] ?? null
}

function getWidgetNode(state, widgetId) {
  return state.snapshot.widgetNodes[widgetId] ?? null
}

function registerWidgetNode(registration, node) {
  const { state, widgetId } = registration
  const widgetNodes = { ...state.snapshot.widgetNodes }
  if (node) {
    state.nodeCallbacks.set(widgetId, registration.callback)
    if (widgetNodes[widgetId] === node) return
    widgetNodes[widgetId] = node
  } else {
    state.nodeCallbacks.delete(widgetId)
    if (!Object.hasOwn(widgetNodes, widgetId)) return
    delete widgetNodes[widgetId]
  }
  state.snapshot = { ...state.snapshot, widgetNodes }
  notify(state)
}

function getWidgetNodeRef(state, widgetId) {
  if (!state.nodeCallbacks.has(widgetId)) {
    const registration = { state, widgetId }
    registration.callback = registerWidgetNode.bind(null, registration)
    state.nodeCallbacks.set(widgetId, registration.callback)
  }
  return state.nodeCallbacks.get(widgetId)
}

function setGesturePatches(state, patchesById) {
  state.gesturePatches = patchesById
  for (const listener of state.gestureListeners) listener()
}

function clearGesturePatches(state) {
  if (Object.keys(state.gesturePatches).length === 0) return
  setGesturePatches(state, {})
}

function getGesturePatch(state, widgetId) {
  return state.gesturePatches[widgetId] ?? null
}

function subscribeGesture(state, listener) {
  state.gestureListeners.add(listener)
  return () => state.gestureListeners.delete(listener)
}

function getSnapshot(state) {
  return state.snapshot
}

function unsubscribe(state, listener) {
  state.listeners.delete(listener)
  if (state.listeners.size === 0 && state.frame !== null) publishDrafts(state)
}

function subscribe(state, listener) {
  state.listeners.add(listener)
  return unsubscribe.bind(null, state, listener)
}

function createWidgetLiveEdits() {
  const globalDefaults = {}
  const state = {
    drafts: {},
    globalDefaults,
    frame: null,
    listeners: new Set(),
    gesturePatches: {},
    gestureListeners: new Set(),
    nodeCallbacks: new Map(),
    snapshot: { liveWidgetDrafts: {}, liveGlobalDefaults: globalDefaults, widgetNodes: {} },
  }
  return {
    clearGesturePatches: clearGesturePatches.bind(null, state),
    clearGlobalDefaultDrafts: clearGlobalDefaultDrafts.bind(null, state),
    clearWidgetDraft: clearWidgetDraft.bind(null, state),
    clearWidgetDrafts: clearWidgetDrafts.bind(null, state),
    getGesturePatch: getGesturePatch.bind(null, state),
    getGlobalDefaultDrafts: getGlobalDefaultDrafts.bind(null, state),
    getSnapshot: getSnapshot.bind(null, state),
    getWidgetDraft: getWidgetDraft.bind(null, state),
    getWidgetNode: getWidgetNode.bind(null, state),
    getWidgetNodeRef: getWidgetNodeRef.bind(null, state),
    resetWidgetDrafts: resetWidgetDrafts.bind(null, state),
    setGesturePatches: setGesturePatches.bind(null, state),
    setLiveGlobalDefault: setLiveGlobalDefault.bind(null, state),
    setLiveWidgetDraft: setLiveWidgetDraft.bind(null, state),
    setLiveWidgetDrafts: setLiveWidgetDrafts.bind(null, state),
    subscribe: subscribe.bind(null, state),
    subscribeGesture: subscribeGesture.bind(null, state),
  }
}

/** @returns {object} Shared controller for widget and global default drafts and canvas registration. */
export default function useWidgetDraftState() {
  const [controller] = useState(createWidgetLiveEdits)
  return controller
}

/** @param {object} controller Shared controller. @returns {object} Controller methods and current drafts/nodes. */
export function useWidgetDraftView(controller) {
  const snapshot = useSyncExternalStore(controller.subscribe, controller.getSnapshot, controller.getSnapshot)
  return { ...controller, ...snapshot }
}

/** @param {object} controller Shared controller. @param {object} widget Config widget. @returns {object} The widget with its live canvas gesture applied. */
export function useGestureWidget(controller, widget) {
  const patch = useSyncExternalStore(controller.subscribeGesture, () => controller.getGesturePatch(widget.id))
  return useMemo(() => (patch === null ? widget : applyWidgetPatch(widget, patch)), [patch, widget])
}
