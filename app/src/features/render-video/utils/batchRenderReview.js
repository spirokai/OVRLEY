import i18next from 'i18next'
import { resolveBatchVideoTiming } from '@/lib/video-sync'
import { captureBatchSync } from './batchRenderRequest'

/** @param {object} inputs Editor synchronization inputs. @returns {object} Review calibration or an actionable error. */
export function reviewBatchSync(inputs) {
  try {
    return { ...captureBatchSync(inputs), error: null }
  } catch (error) {
    return { error: error.message }
  }
}

/** @param {object[]} choices User queue. @param {object|null} inspection Current descriptors. @param {object} sync Review calibration. @returns {object[]} Reviewed rows. */
export function reviewBatchQueue(choices, inspection, sync) {
  return choices.map((item) => {
    if (inspection?.error && inspection.error !== 'reinspectionRequired') return { ...item, status: 'blocked', error: inspection.error }
    const result = inspection?.sources.get(item.path)
    if (!result) return { ...item, status: 'checking', error: null }
    if (result.error || sync.error) return { ...item, status: 'blocked', error: result.error ?? sync.error }
    try {
      const { timing, hasPositiveOverlap } = resolveBatchVideoTiming(
        result.source.metadata,
        sync.activitySummary,
        sync.calibration,
        result.source.sourceId === inspection.calibrationSource?.sourceId,
      )
      return {
        ...item,
        source: result.source,
        timing,
        status: hasPositiveOverlap === false ? 'blocked' : 'pending',
        error: hasPositiveOverlap === false ? i18next.t('store.videoCouldNotBeSyncedWithActivity', 'Video could not be synced with activity') : null,
      }
    } catch (error) {
      return { ...item, status: 'blocked', error: error.message }
    }
  })
}

/** @param {object} snapshot Native lifecycle. @returns {object[]} Submitted item presentation; frontend job IDs are validated source paths. */
export function batchResultQueue(snapshot) {
  return snapshot.items.map((item) => {
    return {
      id: item.id,
      path: item.id,
      filename: item.id.split(/[/\\]/).at(-1),
      status: item.outcome?.status ?? item.phase,
      error: item.outcome?.status === 'failed' ? item.outcome.message : null,
      outputPath: item.outcome?.status === 'succeeded' ? item.outcome.outputPath : null,
    }
  })
}

/** @param {object} snapshot Native snapshot. @returns {boolean} Whether native cleanup has completed. */
export function isBatchFinished(snapshot) {
  return snapshot !== null && ['completed', 'completedWithErrors', 'failed', 'cancelled'].includes(snapshot.phase)
}
