/**
 * Manages render dialog phase state and the dialog-scoped settings draft.
 * Composed by useRenderWorkflow.
 *
 * @param {object} params
 * @param {function} params.buildRenderSettingsDraft - Builds the initial settings draft from store state.
 * @param {boolean} params.renderDisabled - Whether the render button is disabled.
 * @param {boolean} params.renderingVideo - Whether a render is currently active.
 * @param {string} params.renderStatus - Current render status from the store.
 */

import { useCallback, useEffect, useRef, useState } from 'react'

export default function useRenderDialogState({
  buildRenderSettingsDraft,
  resolveRenderSettingsDraft,
  onOpenError,
  renderDisabled,
  renderingVideo,
  renderStatus,
}) {
  // Dialog phase and renderSettingsDraft are true dialog-local state, not
  // mirrors of store values. The draft exists only for the lifetime of the
  // open dialog and can intentionally diverge from committed store settings.
  const [renderDialogPhase, setRenderDialogPhase] = useState('closed')
  const [renderSettingsDraft, setRenderSettingsDraft] = useState(null)
  const [opening, setOpening] = useState(false)
  const openingRef = useRef(false)

  // Auto-close the dialog after a render finishes, is cancelled, or errors.
  useEffect(() => {
    if (renderDialogPhase === 'progress' && !renderingVideo && ['complete', 'cancelled', 'error'].includes(renderStatus)) {
      setRenderDialogPhase('closed')
    }
  }, [renderDialogPhase, renderStatus, renderingVideo])

  // Opening creates a fresh draft from the current store-backed defaults.
  const openRenderDialog = useCallback(async () => {
    if (renderDisabled || openingRef.current) {
      return
    }

    openingRef.current = true
    setOpening(true)
    try {
      const draft = resolveRenderSettingsDraft ? await resolveRenderSettingsDraft() : buildRenderSettingsDraft()
      setRenderSettingsDraft(draft)
      setRenderDialogPhase('confirm')
    } catch (error) {
      onOpenError?.(error)
    } finally {
      openingRef.current = false
      setOpening(false)
    }
  }, [buildRenderSettingsDraft, onOpenError, renderDisabled, resolveRenderSettingsDraft])

  // Closing is blocked while an active render is in progress.
  const closeRenderDialog = useCallback(() => {
    if (renderDialogPhase === 'progress' || renderingVideo) {
      return
    }

    setRenderDialogPhase('closed')
  }, [renderDialogPhase, renderingVideo])

  // Draft updates merge partial changes without mutating the current object.
  const updateRenderSettingsDraft = useCallback((updates) => {
    setRenderSettingsDraft((currentDraft) => {
      if (!currentDraft) {
        return currentDraft
      }

      return {
        ...currentDraft,
        ...updates,
      }
    })
  }, [])

  return {
    renderDialogPhase,
    renderSettingsDraft,
    opening,
    setRenderDialogPhase,
    openRenderDialog,
    closeRenderDialog,
    updateRenderSettingsDraft,
  }
}
