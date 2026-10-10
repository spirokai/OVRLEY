import { useCallback, useEffect } from 'react'
import * as backend from '@/api/backend'
import useStore from '@/store/useStore'
import { useRenderStore } from '@/hooks/useAppStoreSelectors'
import { runWithoutEditorHistory } from '@/features/undo-redo/undoHistory'
import { rememberAcceptedRenderOutput } from '@/lib/file-dialog'
import { createPreviewRenderRequest, createRenderRequest, isRendererBusy } from '../utils/renderRequest'
import { createRenderProgress } from '../utils/renderPresentation'
import i18next from 'i18next'

function applyRenderProgress(snapshot) {
  useStore.getState().setRenderProgress(createRenderProgress(snapshot))
}

// The transports are the native boundary. Submission, buffering, recovery and
// listener lifetime are shared; native IDs and terminal contracts stay native.
const transports = {
  current: {
    subscribe: (observe) => backend.subscribeRenderProgress(observe),
    submit: ({ config, parsedActivity, outputPath, overwrite }) => backend.renderVideo(config, parsedActivity, { outputPath, overwrite }),
    read: () => backend.getRenderProgress(),
    id: (snapshot) => snapshot.render_id,
    eventId: (event) => event.render_id,
    activeId: (state) => state.activeRenderId,
    snapshot: (state) => state.renderProgress,
    finished: (snapshot) => ['complete', 'cancelled', 'error'].includes(snapshot.status),
    accept: (accepted) => {
      if (!accepted.started || !accepted.render_id || !accepted.outputPath) throw new Error('Render did not start')
      useStore.getState().startRenderSession(accepted.render_id, accepted.outputPath, {
        message: i18next.t('render-video.startingRender', 'Starting render...'),
      })
      void rememberAcceptedRenderOutput(accepted.outputPath)
    },
    acceptedId: (accepted) => accepted.render_id,
    apply: applyRenderProgress,
    applyEvent: applyRenderProgress,
    finish: (state) => {
      const { status, message } = state.renderProgress
      const outputPath = state.activeRenderOutputPath
      state.clearRenderSession()
      if (status === 'error') state.setErrorMessage(message)
      if (status === 'complete') {
        void backend.openVideo(outputPath).catch((error) => useStore.getState().setErrorMessage(error.message))
      }
    },
    cancel: () => backend.cancelRender(),
  },
  batch: {
    subscribe: (observe) => backend.subscribeBatchRenderProgress(observe),
    submit: (request) => backend.submitBatchRender(request),
    read: (id) => backend.getBatchRenderSnapshot(id),
    id: (snapshot) => snapshot.batchId,
    eventId: (event) => event.data.batchId,
    activeId: (state) => state.batchSnapshot?.batchId ?? null,
    snapshot: (state) => state.batchSnapshot,
    finished: (snapshot) => !snapshot.rendererBusy,
    accept: (accepted) => useStore.getState().acceptBatchSnapshot(accepted.snapshot),
    acceptedId: (accepted) => accepted.batchId,
    apply: (snapshot) => useStore.getState().applyBatchSnapshot(snapshot),
    applyEvent: (event) => {
      switch (event.kind) {
        case 'snapshot':
          useStore.getState().applyBatchSnapshot(event.data)
          break
        case 'progress': {
          const state = useStore.getState()
          const snapshot = state.batchSnapshot
          // A tick belongs to one queue transition. If it arrives before that
          // snapshot, wait for the snapshot rather than updating an older queue.
          if (event.data.snapshotRevision === snapshot.snapshotRevision) state.applyBatchSnapshot({ ...snapshot, ...event.data })
          break
        }
        default:
          throw new Error(`Unknown batch render event: ${event.kind}`)
      }
    },
    finish: () => {},
    cancel: async (id) => useStore.getState().applyBatchSnapshot(await backend.cancelBatchRender(id)),
  },
}

// Native work outlives React mounts. Each target has at most one observer,
// released after terminal native cleanup or a rejected submission.
const observers = new Map()

function createObserver(target, activeId = null) {
  const transport = transports[target]
  let id = activeId
  let closed = false
  let attachment = null
  let unlisten = null
  let unsubscribe = null
  let eventSequence = 0
  const buffered = new Map()
  const observer = {
    id: () => id,
    ready: () => unlisten !== null,
    beginSubmission: () => buffered.clear(),
    dispose: () => {
      if (closed) return
      closed = true
      if (unlisten !== null) unlisten()
      if (unsubscribe !== null) unsubscribe()
      buffered.clear()
      if (observers.get(target) === observer) observers.delete(target)
    },
    attach: () => {
      attachment ??= (async () => {
        observers.set(target, observer)
        unsubscribe = useStore.subscribe(checkCompletion)
        checkCompletion()
        if (closed) return
        unlisten = await transport.subscribe(receive)
        if (closed) unlisten()
      })()
      return attachment
    },
    accept: (accepted) => {
      id = transport.acceptedId(accepted)
      transport.accept(accepted)
      const events = buffered.get(id)
      buffered.clear()
      if (events !== undefined) events.forEach(applyEvent)
    },
    recover: async () => {
      if (closed) return
      const sequence = eventSequence
      try {
        const snapshot = await transport.read(id)
        // Batch reads carry revisions. Single reads have no ordering token,
        // so an event arriving during the read supersedes that read.
        if (target === 'batch' || sequence === eventSequence) apply(snapshot)
      } catch (error) {
        if (!closed) useStore.getState().setErrorMessage(error.message)
      }
    },
  }

  function checkCompletion() {
    if (closed || id === null) return
    const state = useStore.getState()
    if (transport.activeId(state) !== id) {
      observer.dispose()
      return
    }
    if (transport.finished(transport.snapshot(state))) {
      observer.dispose()
      if (state.renderCancellationTarget === target) state.setRenderCancellationTarget(null)
      transport.finish(state)
    }
  }

  function apply(snapshot) {
    if (closed || transport.id(snapshot) !== id) return
    transport.apply(snapshot)
  }

  function applyEvent(event) {
    if (closed || transport.eventId(event) !== id) return
    transport.applyEvent(event)
  }

  function receive(event) {
    if (closed) return
    eventSequence += 1
    if (id === null) {
      const eventId = transport.eventId(event)
      const previous = buffered.get(eventId)
      if (target === 'current') buffered.set(eventId, [event])
      else {
        // Retain the newest full queue plus its newest tick until acceptance.
        // A tick alone cannot reconstruct outcomes from earlier queue items.
        const sameKind = previous?.find((entry) => entry.kind === event.kind)
        if (sameKind === undefined || event.data.revision > sameKind.data.revision) {
          const events = [...(previous ?? []).filter((entry) => entry.kind !== event.kind), event]
          buffered.set(
            eventId,
            events.sort((left, right) => left.data.revision - right.data.revision),
          )
        }
      }
      return
    }
    applyEvent(event)
  }

  return observer
}

/**
 * @param {object} options Optional confirmation target for preparing its progress listener.
 * @returns {object} Shared submission, cancellation, preview and native session recovery.
 */
export default function useRenderExecution({ reviewTarget = null } = {}) {
  const store = useRenderStore()

  useEffect(() => {
    const prepared = []
    for (const [target, id] of [
      ['current', store.activeRenderId],
      ['batch', store.activeBatchId],
    ]) {
      if ((id === null && target !== reviewTarget) || observers.get(target)?.id() === id) continue
      observers.get(target)?.dispose()
      const observer = createObserver(target, id)
      if (id === null) prepared.push({ target, observer })
      void observer
        .attach()
        .then(() => {
          if (id !== null) return observer.recover()
        })
        .catch((error) => {
          observer.dispose()
          useStore.getState().setErrorMessage(error.message)
        })
    }
    return () => {
      for (const { target, observer } of prepared) {
        if (observer.id() === null && useStore.getState().renderSubmissionTarget !== target) observer.dispose()
      }
    }
  }, [store.activeRenderId, store.activeBatchId, reviewTarget])

  const submit = useCallback(async ({ settings, batchReview, overwrite = false, onAccepted }) => {
    const state = useStore.getState()
    if (isRendererBusy(state)) return false
    if (settings.renderTarget === 'batch' && Object.entries(batchReview.reviewedInputs).some(([key, value]) => state[key] !== value)) return false
    const request = createRenderRequest({ editorSnapshot: state, settings, batchReview, overwrite })
    state.setRenderSubmissionTarget(request.target)
    const observer = observers.get(request.target) ?? createObserver(request.target)
    observer.beginSubmission()
    let accepted = false
    try {
      if (!observer.ready()) await observer.attach()
      const result = await transports[request.target].submit(request.payload)
      observer.accept(result)
      accepted = true
      runWithoutEditorHistory(useStore, () => useStore.getState().setRenderSettings(request.settings))
      onAccepted()
      // Acceptance authorizes the progress screen. Recovery only seeds its
      // snapshot and must not delay returning control to the dialog.
      void observer.recover()
      return true
    } catch (error) {
      if (!accepted) observer.dispose()
      throw error
    } finally {
      useStore.getState().setRenderSubmissionTarget(null)
    }
  }, [])

  const cancel = useCallback(async (target) => {
    const state = useStore.getState()
    const transport = transports[target]
    const id = transport.activeId(state)
    if (id === null || (!state.renderingVideo && target === 'current') || (target === 'batch' && !state.batchSnapshot.rendererBusy)) return
    if (state.renderCancellationTarget !== null) return
    state.setRenderCancellationTarget(target)
    try {
      await transport.cancel(id)
    } catch (error) {
      const current = useStore.getState()
      if (transport.activeId(current) !== id) return
      current.setErrorMessage(error.message)
      current.setRenderCancellationTarget(null)
    }
  }, [])

  const renderPreviewFrame = useCallback(async () => {
    const state = useStore.getState()
    if (isRendererBusy(state)) return
    state.setRenderSubmissionTarget('preview')
    try {
      const { config, parsedActivity, second } = createPreviewRenderRequest(state)
      const result = await backend.renderPreviewFrame(config, parsedActivity, second)
      await backend.openVideo(result.path)
    } catch (error) {
      useStore.getState().setErrorMessage(error.message)
    } finally {
      useStore.getState().setRenderSubmissionTarget(null)
    }
  }, [])

  return { ...store, submit, cancel, renderPreviewFrame }
}
