/**
 * Composes all OverlayMoveable handler groups (drag, resize, scale, rotate)
 * into a single handlers object passed to react-moveable.
 */

import { useDragHandlers } from './useDragHandlers'
import { useResizeHandlers } from './useResizeHandlers'
import { useScaleHandlers } from './useScaleHandlers'
import { useRotateHandlers } from './useRotateHandlers'

/**
 * Provides overlay moveable handlers state and actions.
 *
 * @param {object} options - Structured options for the helper.
 * @param {*} options.clearWidgetDraft - Value for clear widget draft.
 * @param {*} options.clearWidgetDrafts - Value for clear widget drafts.
 * @param {*} options.commitWidgetUpdate - Value for commit widget update.
 * @param {*} options.commitWidgetUpdates - Value for commit widget updates.
 * @param {*} options.draftWidgetsRef - Value for draft widgets ref.
 * @param {*} options.effectiveSelectedWidgetIds - Value for effective selected widget ids.
 * @param {*} options.globalScale - Scale factor applied to the overlay preview.
 * @param {*} options.groupDragSelectionIds - Value for group drag selection ids.
 * @param {*} options.interactionStartRef - Value for interaction start ref.
 * @param {*} options.renderedWidgetMap - Value for rendered widget map.
 * @param {*} options.scalePreviewFrameRef - Value for scale preview frame ref.
 * @param {*} options.selectedTarget - Value for selected target.
 * @param {*} options.selectedWidget - Value for selected widget.
 * @param {*} options.selectedWidgets - Value for selected widgets.
 * @param {*} options.setGroupDragSelectionIds - Value for set group drag selection ids.
 * @param {*} options.setIsGroupDragActive - Value for set is group drag active.
 * @param {*} options.setLiveWidgetDraft - Value for set live widget draft.
 * @param {*} options.setLiveWidgetDraftsBatch - Value for set live widget drafts batch.
 * @returns {object} Combined handlers for react-moveable.
 */
export default function useOverlayMoveableHandlers(options) {
  const dragHandlers = useDragHandlers(options)
  const resizeHandlers = useResizeHandlers(options)
  const scaleHandlers = useScaleHandlers(options)
  const rotateHandlers = useRotateHandlers(options)

  return {
    ...dragHandlers,
    ...resizeHandlers,
    ...scaleHandlers,
    ...rotateHandlers,
  }
}
