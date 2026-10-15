import { isTimeDependentWasmWidget } from '@/lib/widget/wasm-widget'
import { applyWidgetDrafts } from '@/lib/widget/widget-draft'
import { normalizeRotation } from './widgetGestureGeometry'

const targets = new WeakMap()

function rotationPatch(widget, rotation, translate = [0, 0], geometry) {
  const { data } = widget
  const centeredRotation = widget.type === 'course' || widget.type === 'raster'
  const patch = {
    x: data.x + translate[0],
    y: data.y + translate[1],
    rotation: normalizeRotation(centeredRotation ? Math.round(rotation) : rotation),
  }
  if (centeredRotation) {
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

  function begin(ids, onDraft) {
    flush()
    const snapshot = getSnapshot()
    active = {
      onDraft,
      widgets: new Map(
        ids.map((id) => {
          const widget = snapshot.widgets.find((item) => item.id === id)
          if (!widget) throw new Error(`Missing WASM widget: ${id}`)
          return [
            id,
            {
              id,
              type: widget.type,
              data: widget.data,
              scene: snapshot.scene,
              geometry: getGeometry(id),
              intent: null,
              patch: null,
              resolvedIntent: null,
              second: null,
              committing: false,
            },
          ]
        }),
      ),
    }
    return Object.fromEntries(
      [...active.widgets].map(([id, { data, geometry }]) => [id, { data, bounds: geometry.content, rotationOrigin: geometry.rotation_origin }]),
    )
  }

  function update(intents) {
    if (!active) return
    for (const [id, intent] of Object.entries(intents)) active.widgets.get(id).intent = intent
    schedule()
  }

  function finish() {
    if (!active) return null
    for (const widget of active.widgets.values()) {
      widget.committing = ['course', 'elevation'].includes(widget.type) && widget.intent?.type === 'resize'
    }
    flush() // Mouseup can precede the pending animation frame.
    const patches = [...active.widgets].filter(([, widget]) => widget.patch !== null).map(([id, widget]) => [id, widget.patch])
    active = null
    return patches.length ? Object.fromEntries(patches) : null
  }

  function cancel() {
    if (!active) return
    active = null
    schedule()
  }

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
          return begin([id], (patches) => onDraft(patches[id]))[id]
        },
        update(intent) {
          if (!active?.widgets.has(id)) return
          update({ [id]: intent })
        },
        finish() {
          if (!active?.widgets.has(id)) return null
          const patches = finish()
          return patches === null ? null : patches[id]
        },
        cancel() {
          if (active?.widgets.has(id)) cancel()
        },
        group(ids) {
          return { begin: (onDraft) => begin(ids, onDraft), update, finish, cancel }
        },
      }
    },
    isResizing(id) {
      const widget = active?.widgets.get(id)
      return widget?.intent?.type === 'resize' && !widget.committing
    },
    resolve(snapshot) {
      if (!active) return snapshot.widgets
      const patches = {}
      let changed = false
      for (const widget of active.widgets.values()) {
        const { intent } = widget
        if (!intent) continue
        if (
          widget.resolvedIntent !== intent ||
          widget.committing ||
          (intent.type === 'resize' && isTimeDependentWasmWidget(widget) && widget.second !== snapshot.second)
        ) {
          const patch =
            intent.type === 'drag'
              ? { x: widget.data.x + intent.translate[0], y: widget.data.y + intent.translate[1] }
              : intent.type === 'rotate'
                ? rotationPatch(widget, intent.rotation, intent.translate, widget.geometry)
                : resize(
                    {
                      type: widget.type,
                      widget: widget.data,
                      scene: widget.scene,
                      second: snapshot.second,
                      ...(['course', 'elevation'].includes(widget.type) ? { commit: widget.committing } : {}),
                      ...intent.geometry,
                    },
                    widget.id,
                  )
          changed ||= JSON.stringify(patch) !== JSON.stringify(widget.patch)
          widget.patch = patch
          widget.resolvedIntent = intent
          widget.second = snapshot.second
        }
        patches[widget.id] = widget.patch
      }
      if (changed) active.onDraft(patches)
      const originWidgets = snapshot.widgets.map((widget) => {
        const origin = active.widgets.get(widget.id)
        return origin === undefined ? widget : { ...widget, data: origin.data }
      })
      return applyWidgetDrafts(originWidgets, patches)
    },
    dispose() {
      active = null
    },
  }
}
