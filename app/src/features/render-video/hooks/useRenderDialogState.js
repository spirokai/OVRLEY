/**
 * Manages render dialog phase state and settings draft.
 * Composed by useRenderWorkflow.
 *
 * @param {object} params
 * @param {function} params.buildRenderSettingsDraft - Builds the initial settings draft from store state.
 * @param {boolean} params.renderDisabled - Whether the render button is disabled.
 * @param {boolean} params.renderingVideo - Whether a render is currently active.
 * @param {string} params.renderStatus - Current render status from the store.
 */

import { useCallback, useEffect, useState } from 'react'

export default function useRenderDialogState({ buildRenderSettingsDraft, renderDisabled, renderingVideo, renderStatus }) {
  // Local UI state — dialog phase ('closed'|'confirm'|'progress') and the current settings draft
  const [renderDialogPhase, setRenderDialogPhase] = useState('closed')
  const [renderSettingsDraft, setRenderSettingsDraft] = useState(null)

  // Side effects — auto-close the dialog when a render finishes, is cancelled, or errors out
  useEffect(() => {
    if (renderDialogPhase === 'progress' && !renderingVideo && ['complete', 'cancelled', 'error'].includes(renderStatus)) {
      setRenderDialogPhase('closed')
    }
  }, [renderDialogPhase, renderStatus, renderingVideo])

  // Open handler — populates the settings draft from store and transitions to confirm phase
  const openRenderDialog = useCallback(() => {
    if (renderDisabled) {
      return
    }

    setRenderSettingsDraft(buildRenderSettingsDraft())
    setRenderDialogPhase('confirm')
  }, [buildRenderSettingsDraft, renderDisabled])

  // Close handler — resets the dialog to closed phase, blocked while render is active
  const closeRenderDialog = useCallback(() => {
    if (renderDialogPhase === 'progress' || renderingVideo) {
      return
    }

    setRenderDialogPhase('closed')
  }, [renderDialogPhase, renderingVideo])

  // Update handler — merges partial updates into the current settings draft
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
    setRenderDialogPhase,
    openRenderDialog,
    closeRenderDialog,
    updateRenderSettingsDraft,
  }
}
