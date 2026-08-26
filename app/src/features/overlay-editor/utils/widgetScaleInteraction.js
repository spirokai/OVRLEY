import { isFramedWidget } from '@/lib/widget/display-type-behavior'
import { getWidgetIdFromTarget, getWidgetVisualBoundsFromTarget } from './widgetDomHelpers'
import {
  buildFrameInteractionLayout,
  buildScaleInteractionLayout,
  captureWidgetLayout,
  getWidgetInteractionPosition,
} from './widgetInteractionGeometry'
import { buildLiveResizeUpdate, buildResizeUpdate, buildScaleDraft, captureResizeOrigin } from './widgetResizeScaling'

function buildIntrinsicScaleUpdate(origin, scaleFactor, globalScale, translateX, translateY, round = false) {
  const gradientYOffset = origin.widget.type === 'gradient' ? Math.min(0, -origin.data.value_offset) : 0
  const x = origin.x + translateX + origin.renderedMinX * (1 - scaleFactor) * globalScale
  const y = origin.y + translateY + (origin.renderedMinY * globalScale + gradientYOffset) * (1 - scaleFactor)

  return {
    x: round ? Math.round(x) : x,
    y: round ? Math.round(y) : y,
    ...buildScaleDraft(origin.data, scaleFactor, origin.widget, { round }),
  }
}

/**
 * Captures the canonical origin used by single and group scale interactions.
 *
 * @param {HTMLElement} target - Moveable target element.
 * @param {object} widget - Widget definition at interaction start.
 * @param {number} globalScale - Global widget scale.
 * @returns {object} Captured scale origin.
 */
export function captureScaleInteractionOrigin(target, widget, globalScale) {
  const layout = captureWidgetLayout(target, widget, globalScale)
  const position = getWidgetInteractionPosition(widget, layout)
  const visualBounds = getWidgetVisualBoundsFromTarget(target)
  const isFramed = isFramedWidget(widget)

  return {
    data: widget.data,
    id: widget.id,
    isFramed,
    layout,
    renderedMinX: visualBounds?.minX ?? 0,
    renderedMinY: visualBounds?.minY ?? 0,
    resizeOrigin: isFramed ? captureResizeOrigin(widget, widget.data) : null,
    widget,
    x: position.x,
    y: position.y,
  }
}

/**
 * Captures scale origins by widget ID for a Moveable group.
 *
 * @param {HTMLElement[]} targets - Moveable group targets.
 * @param {object[]} widgets - Selected widget definitions.
 * @param {number} globalScale - Global widget scale.
 * @returns {Object<string, object>} Scale origins keyed by widget ID.
 */
export function captureGroupScaleOrigins(targets, widgets, globalScale) {
  const widgetsById = Object.fromEntries(widgets.map((widget) => [widget.id, widget]))

  if (targets.length !== widgets.length) {
    throw new Error(`Cannot start group scale: received ${targets.length} targets for ${widgets.length} widgets`)
  }

  return Object.fromEntries(
    targets.map((target) => {
      const widgetId = getWidgetIdFromTarget(target)
      const widget = widgetsById[widgetId]

      if (!widget) {
        throw new Error(`Cannot start group scale: target widget ${widgetId ?? '(missing id)'} is not selected`)
      }

      return [widgetId, captureScaleInteractionOrigin(target, widget, globalScale)]
    }),
  )
}

/**
 * Builds one live scale draft from a captured interaction origin.
 *
 * @param {object} origin - Captured scale origin.
 * @param {number} scaleFactor - Uniform scale multiplier.
 * @param {object} drag - Moveable child drag event.
 * @param {number} globalScale - Global widget scale.
 * @returns {{data: object, layout: object}} Live scale draft.
 */
export function buildScaleInteractionDraft(origin, scaleFactor, drag, globalScale) {
  const [translateX, translateY] = drag.beforeTranslate

  if (origin.isFramed) {
    const width = Math.max((origin.layout.width / globalScale) * scaleFactor, 8)
    const height = Math.max((origin.layout.height / globalScale) * scaleFactor, 8)
    const framePatch = {
      height,
      width,
      x: origin.x + translateX,
      y: origin.y + translateY,
    }

    return {
      data: buildLiveResizeUpdate(origin.resizeOrigin, framePatch, origin.widget),
      layout: buildFrameInteractionLayout(origin.layout, {
        height: height * globalScale,
        translateX,
        translateY,
        width: width * globalScale,
      }),
    }
  }

  return {
    data: buildIntrinsicScaleUpdate(origin, scaleFactor, globalScale, translateX, translateY),
    layout: buildScaleInteractionLayout(origin.layout, {
      globalScale,
      rotation: origin.layout.rotation,
      scaleFactor,
      translateX,
      translateY,
    }),
  }
}

/**
 * Builds the persisted update for a completed scale interaction.
 *
 * @param {object} origin - Captured scale origin.
 * @param {{data: object, layout: object}} draft - Final live scale draft.
 * @param {number} globalScale - Global widget scale.
 * @returns {object} Commit-ready widget update.
 */
export function buildScaleInteractionCommit(origin, draft, globalScale) {
  if (origin.isFramed) {
    return buildResizeUpdate(
      origin.resizeOrigin,
      {
        height: Math.max(Math.round(draft.data.height), 0),
        width: Math.max(Math.round(draft.data.width), 0),
        x: Math.round(draft.data.x),
        y: Math.round(draft.data.y),
      },
      { round: true },
    )
  }

  return buildIntrinsicScaleUpdate(origin, draft.layout.scaleFactor, globalScale, draft.layout.translateX, draft.layout.translateY, true)
}
