/**
 * Runs the dialogs that follow a video import one at a time (see `videoImportFollowUp`).
 */

import { useState } from 'react'
import { changeVideoCollection } from '@/features/undo-redo/undoHistory'
import { NO_TRANSITION, XFADE_TRANSITIONS } from '@/lib/video-program/transition-catalog'
import { filenameFromSelectedPath } from '@/lib/utils'
import useStore from '@/store/useStore'
import i18next from 'i18next'
import { useShallow } from 'zustand/react/shallow'
import { createImportFollowUpSteps, dropSettledSteps, listImportOverlaps, listPlacementCandidates } from '../utils/videoImportFollowUp'

const TRANSITION_TYPES = [NO_TRANSITION, ...XFADE_TRANSITIONS]

function formatSecondsInput(seconds) {
  return String(seconds)
}

function parseSecondsInput(value) {
  const trimmed = value.trim()
  return trimmed === '' ? Number.NaN : Number(trimmed)
}

function createTransitionForm(defaults) {
  return {
    type: defaults.type,
    duration: formatSecondsInput(defaults.durationSeconds),
    threshold: formatSecondsInput(defaults.splitGapThresholdSeconds),
    error: null,
  }
}

/**
 * Owns the follow-up step queue and the small forms of its dialogs.
 * @returns {{start: function, dialogs: object}} Queue entry point and dialog view models.
 */
export default function useVideoImportFollowUp() {
  const [steps, setSteps] = useState([])
  // Form state is tagged with the step it belongs to, so a new step starts from fresh defaults.
  const [placementChoice, setPlacementChoice] = useState(null)
  const [transitionForm, setTransitionForm] = useState(null)
  const collection = useStore(
    useShallow((state) => ({ importedVideos: state.importedVideos, videoEdits: state.videoEdits, transitionDefaults: state.transitionDefaults })),
  )

  const current = steps[0] ?? null
  const advance = () => setSteps(dropSettledSteps(steps.slice(1), useStore.getState()))

  /** Starts the follow-up of a finished import. */
  const start = (result) => setSteps(dropSettledSteps(createImportFollowUpSteps(result), useStore.getState()))

  // Placement prompt.
  const placementStep = current?.kind === 'placement' ? current : null
  const candidates = placementStep ? listPlacementCandidates(collection, placementStep.videoId, steps.slice(1)) : []
  const choice =
    placementStep && placementChoice?.step === placementStep
      ? placementChoice
      : { step: placementStep, anchorVideoId: candidates.at(-1)?.id ?? null, side: 'after' }
  const placedVideo = placementStep ? collection.importedVideos.find((video) => video.id === placementStep.videoId) : null

  // Transition defaults form.
  const transitionStep = current?.kind === 'transitionDefaults' ? current : null
  const form =
    transitionStep && transitionForm?.step === transitionStep
      ? transitionForm
      : { step: transitionStep, ...createTransitionForm(collection.transitionDefaults) }
  const updateForm = (changes) => setTransitionForm({ ...form, ...changes, error: null })

  const confirmTransitionDefaults = () => {
    const durationSeconds = parseSecondsInput(form.duration)
    const splitGapThresholdSeconds = parseSecondsInput(form.threshold)
    try {
      useStore.getState().setTransitionDefaults({ type: form.type, durationSeconds, splitGapThresholdSeconds })
    } catch (error) {
      setTransitionForm({ ...form, error: error.message })
      return
    }
    advance()
  }

  const dismissTransitionDefaults = () => {
    const { type, durationSeconds, splitGapThresholdSeconds } = useStore.getState().transitionDefaults
    useStore.getState().setTransitionDefaults({ type, durationSeconds, splitGapThresholdSeconds })
    advance()
  }

  // Overlap warning.
  const overlapStep = current?.kind === 'overlaps' ? current : null
  const overlaps = overlapStep ? listImportOverlaps(collection, overlapStep.videoIds) : []

  return {
    start,
    dialogs: {
      report: {
        open: current?.kind === 'report',
        report: current?.kind === 'report' ? current.report : null,
        onClose: advance,
      },
      placement: {
        open: placementStep !== null,
        videoName: placedVideo ? filenameFromSelectedPath(placedVideo.path) : '',
        candidates,
        anchorVideoId: choice.anchorVideoId,
        side: choice.side,
        onAnchorChange: (anchorVideoId) => setPlacementChoice({ ...choice, anchorVideoId }),
        onSideChange: (side) => setPlacementChoice({ ...choice, side }),
        onConfirm: () => {
          changeVideoCollection(useStore, () => useStore.getState().anchorVideo(placementStep.videoId, choice.anchorVideoId, choice.side))
          advance()
        },
        onSkip: advance,
      },
      transitionDefaults: {
        open: transitionStep !== null,
        typeOptions: TRANSITION_TYPES.map((type) => ({
          value: type,
          label: type === NO_TRANSITION ? i18next.t('video-preview.noTransition', 'No transition') : type,
        })),
        type: form.type,
        duration: form.duration,
        threshold: form.threshold,
        durationDisabled: form.type === NO_TRANSITION,
        error: form.error,
        onTypeChange: (type) => updateForm({ type }),
        onDurationChange: (duration) => updateForm({ duration }),
        onThresholdChange: (threshold) => updateForm({ threshold }),
        onConfirm: confirmTransitionDefaults,
        onDismiss: dismissTransitionDefaults,
      },
      overlaps: {
        open: overlapStep !== null,
        overlaps,
        onClose: advance,
      },
    },
  }
}
