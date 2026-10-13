import { isTimeDependentWasmWidget } from '@/lib/widget/wasm-widget'
import { applyWidgetDrafts } from '@/lib/widget/widget-draft'

const targets = new WeakMap()

/** @param {HTMLElement} target Rendered widget wrapper. @returns {object} Its preview interaction controller. */
export function getWidgetInteraction(target) {
  const controller = targets.get(target)
  if (!controller) throw new Error('WASM widget has not rendered yet')
  return controller
}

/** @param {HTMLElement} target Widget wrapper. @param {object} controller Session-owned interaction methods. */
export function registerWidgetInteraction(target, controller) {
  targets.set(target, controller)
}

/** @param {HTMLElement} target Detached wrapper. */
export function unregisterWidgetInteraction(target) {
  targets.delete(target)
}

/**
 * One active gesture per editor session. Pointer events replace intent; the preview's
 * existing frame scheduler resolves and renders the latest intent before publishing drafts.
 * @param {object} options Session scheduling, snapshots, geometry and Rust resize operation.
 * @returns {object} Gesture lifecycle and effective render configuration.
 */
export function createWidgetInteraction({ schedule, flush, getSnapshot, getGeometry, resize }) {
  let active = null
  return {
    forWidget(id) {
      return {
        begin(onDraft) {
          flush()
          const snapshot = getSnapshot()
          const widget = snapshot.widgets.find((item) => item.id === id)
          if (!widget) throw new Error(`Missing WASM widget: ${id}`)
          const geometry = getGeometry(id)
          active = { id, type: widget.type, data: widget.data, scene: snapshot.scene, intent: null, patch: null, onDraft, resolvedIntent: null, second: null }
          return { data: widget.data, bounds: geometry.content }
        },
        update(intent) {
          if (!active || active.id !== id) return
          active.intent = intent
          schedule()
        },
        finish() {
          if (!active || active.id !== id) return null
          flush() // Mouseup can precede the pending animation frame.
          const patch = active.patch
          active = null
          return patch
        },
        cancel() {
          if (!active || active.id !== id) return
          active = null
          schedule()
        },
      }
    },
    resolve(snapshot) {
      if (!active || !active.intent) return snapshot.widgets
      const { intent } = active
      if (active.resolvedIntent !== intent || (intent.type === 'resize' && isTimeDependentWasmWidget(active) && active.second !== snapshot.second)) {
        const patch =
          intent.type === 'drag'
            ? { x: active.data.x + intent.translate[0], y: active.data.y + intent.translate[1] }
            : resize({ type: active.type, widget: active.data, scene: active.scene, second: snapshot.second, ...intent.geometry })
        const changed = JSON.stringify(patch) !== JSON.stringify(active.patch)
        active.patch = patch
        active.resolvedIntent = intent
        active.second = snapshot.second
        if (changed) active.onDraft(patch)
      }
      const originWidgets = snapshot.widgets.map((widget) => (widget.id === active.id ? { ...widget, data: active.data } : widget))
      return applyWidgetDrafts(originWidgets, { [active.id]: { data: active.patch } })
    },
    dispose() {
      active = null
    },
  }
}

