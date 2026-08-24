/**
 * Scale handler group for OverlayMoveable.
 */

import { applyLiveWidgetStyles, getWidgetIdFromTarget, updateLiveWidgetDraft } from '../utils/widgetDomHelpers'
import {
  buildScaleInteractionCommit,
  buildScaleInteractionDraft,
  captureGroupScaleOrigins,
  captureScaleInteractionOrigin,
} from '../utils/widgetScaleInteraction'

/**
 * Creates single- and group-scale Moveable handlers.
 *
 * @param {object} ctx - Shared handler context.
 * @param {object} ctx.interactionStartRef - Mutable interaction origin ref.
 * @param {object} ctx.draftWidgetsRef - Mutable live draft ref.
 * @param {object} ctx.selectedWidget - Primary selected widget.
 * @param {object} ctx.selectedTarget - Primary selected DOM target.
 * @param {Array<object>} ctx.selectedWidgets - Selected widget definitions.
 * @param {number} ctx.globalScale - Global widget scale.
 * @param {Function} ctx.setLiveWidgetDraft - Single live draft updater.
 * @param {Function} ctx.setLiveWidgetDraftsBatch - Batch live draft updater.
 * @param {Function} ctx.commitWidgetUpdate - Single config commit.
 * @param {Function} ctx.commitWidgetUpdates - Batch config commit.
 * @param {Function} ctx.clearWidgetDraft - Clears one live draft.
 * @param {Function} ctx.clearWidgetDrafts - Clears multiple live drafts.
 * @param {Function} ctx.beginWidgetInteraction - Starts the live interaction.
 * @param {Function} ctx.endWidgetInteraction - Ends the live interaction.
 * @returns {object} Scale handler methods.
 */
export function useScaleHandlers({
  interactionStartRef,
  draftWidgetsRef,
  selectedWidget,
  selectedTarget,
  selectedWidgets,
  globalScale,
  setLiveWidgetDraft,
  setLiveWidgetDraftsBatch,
  commitWidgetUpdate,
  commitWidgetUpdates,
  clearWidgetDraft,
  clearWidgetDrafts,
  beginWidgetInteraction,
  endWidgetInteraction,
}) {
  return {
    onScaleStart: ({ dragStart, target }) => {
      if (!selectedWidget || !selectedTarget) return

      dragStart?.set([0, 0])
      const origin = captureScaleInteractionOrigin(target ?? selectedTarget, selectedWidget, globalScale)

      interactionStartRef.current = { ...origin, type: 'scale' }
      beginWidgetInteraction(origin.id, 'scale')
    },
    onScale: ({ scale, drag, target }) => {
      const origin = interactionStartRef.current
      if (origin?.type !== 'scale') return

      const draft = buildScaleInteractionDraft(origin, scale[0] / globalScale, drag, globalScale)

      updateLiveWidgetDraft({
        draftWidgetsRef,
        setLiveWidgetDraft,
        widgetId: origin.id,
        widget: origin.widget,
        updates: draft.data,
        target: target ?? selectedTarget,
        globalScale,
        layout: draft.layout,
      })
    },
    onScaleEnd: () => {
      const origin = interactionStartRef.current
      if (origin?.type !== 'scale') return

      const draft = draftWidgetsRef.current[origin.id]
      if (draft?.layout) {
        commitWidgetUpdate(origin.id, buildScaleInteractionCommit(origin, draft, globalScale))
      }

      clearWidgetDraft(origin.id)
      endWidgetInteraction(origin.id)
      interactionStartRef.current = null
    },
    onScaleGroupStart: ({ events = [], targets = [] } = {}) => {
      events.forEach((event) => event.dragStart?.set([0, 0]))

      const originsById = captureGroupScaleOrigins(targets, selectedWidgets, globalScale)
      const widgetIds = Object.keys(originsById)
      if (!widgetIds.length) return

      widgetIds.forEach((widgetId) => {
        draftWidgetsRef.current[widgetId] = { data: {}, layout: null }
      })
      beginWidgetInteraction(widgetIds[0], 'group-scale')
      interactionStartRef.current = {
        originsById,
        type: 'group-scale',
        widgetIds,
      }
    },
    onScaleGroup: ({ dist, events = [] } = {}) => {
      const interaction = interactionStartRef.current
      if (interaction?.type !== 'group-scale') return

      const nextDraftsById = {}
      events.forEach((event) => {
        const widgetId = getWidgetIdFromTarget(event.target)
        const origin = interaction.originsById[widgetId]
        if (!origin) return

        const draft = buildScaleInteractionDraft(origin, dist[0], event.drag, globalScale)
        nextDraftsById[widgetId] = draft
        applyLiveWidgetStyles(event.target, draft.layout, globalScale)
      })

      if (Object.keys(nextDraftsById).length) {
        setLiveWidgetDraftsBatch(nextDraftsById)
      }
    },
    onScaleGroupEnd: () => {
      const interaction = interactionStartRef.current
      if (interaction?.type !== 'group-scale') return

      const updatesById = Object.fromEntries(
        interaction.widgetIds.flatMap((widgetId) => {
          const draft = draftWidgetsRef.current[widgetId]

          return draft?.layout ? [[widgetId, buildScaleInteractionCommit(interaction.originsById[widgetId], draft, globalScale)]] : []
        }),
      )

      if (Object.keys(updatesById).length) {
        commitWidgetUpdates(updatesById)
      }
      clearWidgetDrafts(interaction.widgetIds)
      endWidgetInteraction(interaction.widgetIds[0])
      interactionStartRef.current = null
    },
  }
}
