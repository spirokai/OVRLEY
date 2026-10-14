import { isTimeDependentWasmWidget } from '@/lib/widget/wasm-widget'
import { applyWidgetDrafts } from '@/lib/widget/widget-draft'
import { normalizeRotation } from './widgetGestureGeometry'

const targets = new WeakMap()

function rotationPatch(widget, rotation, translate = [0, 0], geometry) {
  const { data } = widget
  const patch = {
    x: data.x + translate[0],
    y: data.y + translate[1],
    rotation: normalizeRotation(widget.type === 'course' ? Math.round(rotation) : rotation),
  }
  if (widget.type === 'course') {
    const center = [geometry.content.left + geometry.rotation_origin[0], geometry.content.top + geometry.rotation_origin[1]]
    const angle = ((patch.rotation - data.rotation) * Math.PI) / 180
    const dx = data.x - center[0]
    const dy = data.y - center[1]
    patch.x = center[0] + dx * Math.cos(angle) - dy * Math.sin(angle) + translate[0]
    patch.y = center[1] + dx * Math.sin(angle) + dy * Math.cos(angle) + translate[1]
  }
  return patch
}

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
        rotate(rotation) {
          flush()
          const widget = getSnapshot().widgets.find((item) => item.id === id)
          if (!widget) throw new Error(`Missing WASM widget: ${id}`)
          return rotationPatch(widget, rotation, [0, 0], getGeometry(id))
        },
        resize(geometry) {
          const snapshot = getSnapshot()
          const widget = snapshot.widgets.find((item) => item.id === id)
          if (!widget) throw new Error(`Missing WASM widget: ${id}`)
          return resize(
            {
              type: widget.type,
              widget: widget.data,
              scene: snapshot.scene,
              second: snapshot.second,
              fixed: [0, 0],
              point: [widget.data.x, widget.data.y],
              ...geometry,
            },
            id,
          )
        },
        begin(onDraft) {
          flush()
          const snapshot = getSnapshot()
          const widget = snapshot.widgets.find((item) => item.id === id)
          if (!widget) throw new Error(`Missing WASM widget: ${id}`)
          const geometry = getGeometry(id)
          active = {
            id,
            type: widget.type,
            data: widget.data,
            scene: snapshot.scene,
            geometry,
            intent: null,
            patch: null,
            onDraft,
            resolvedIntent: null,
            second: null,
            committing: false,
          }
          return { data: widget.data, bounds: geometry.content, rotationOrigin: geometry.rotation_origin }
        },
        update(intent) {
          if (!active || active.id !== id) return
          active.intent = intent
          schedule()
        },
        finish() {
          if (!active || active.id !== id) return null
          active.committing = active.type === 'course' && active.intent?.type === 'resize'
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
    isResizing(id) {
      return active?.id === id && active.intent?.type === 'resize' && !active.committing
    },
    resolve(snapshot) {
      if (!active || !active.intent) return snapshot.widgets
      const { intent } = active
      if (
        active.resolvedIntent !== intent ||
        active.committing ||
        (intent.type === 'resize' && isTimeDependentWasmWidget(active) && active.second !== snapshot.second)
      ) {
        const patch =
          intent.type === 'drag'
            ? { x: active.data.x + intent.translate[0], y: active.data.y + intent.translate[1] }
            : intent.type === 'rotate'
              ? rotationPatch(active, intent.rotation, intent.translate, active.geometry)
              : resize(
                  {
                    type: active.type,
                    widget: active.data,
                    scene: active.scene,
                    second: snapshot.second,
                    ...(active.type === 'course' ? { commit: active.committing } : {}),
                    ...intent.geometry,
                  },
                  active.id,
                )
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
