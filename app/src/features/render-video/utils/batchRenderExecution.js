import * as backend from '@/api/backend'
import useStore from '@/store/useStore'

/**
 * Observes one native lifecycle independently of dialog mounting. Listening
 * precedes submission; buffered events and the recovery read share revision ordering.
 * @param {object} request Captured native request.
 * @returns {Promise<object>} Acceptance after the observer is attached.
 */
export async function submitObservedBatch(request) {
  let batchId = null
  const pending = new Map()
  let unlisten
  const observe = (snapshot) => {
    if (batchId === null) {
      const previous = pending.get(snapshot.batchId)
      if (!previous || snapshot.revision > previous.revision) pending.set(snapshot.batchId, snapshot)
      return
    }
    if (snapshot.batchId !== batchId) return
    useStore.getState().applyBatchSnapshot(snapshot)
    if (!snapshot.rendererBusy) unlisten()
  }
  unlisten = await backend.subscribeBatchRenderProgress(observe)
  let accepted
  try {
    accepted = await backend.submitBatchRender(request)
  } catch (error) {
    unlisten()
    throw error
  }
  batchId = accepted.batchId
  useStore.getState().acceptBatchSnapshot(accepted.snapshot)
  observe(pending.get(batchId) ?? accepted.snapshot)
  pending.clear()
  try {
    observe(await backend.getBatchRenderSnapshot(batchId))
  } catch (error) {
    useStore.getState().setErrorMessage(error.message)
  }
  return accepted
}
