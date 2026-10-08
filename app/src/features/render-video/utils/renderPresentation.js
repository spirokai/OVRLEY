import { DEFAULT_RENDER_PROGRESS } from '@/store/store-utils'
import i18next from 'i18next'
import { resolveBatchVideoTiming } from '@/lib/video-sync'
import { formatClockDuration } from '@/lib/time-format'
import { formatVideoCreationTime } from '@/features/scene-settings/utils/sceneSettingsUtils'
import { captureBatchSync } from './renderRequest'

/** @param {number} durationSeconds Imported video duration. @param {number} offsetSeconds Committed activity offset. @returns {object} Activity-timeline export range. */
export function getImportedVideoExportRange(durationSeconds, offsetSeconds) {
  return { type: 'custom', from: offsetSeconds, to: offsetSeconds + durationSeconds }
}

/** @param {string} value Editable range time. @returns {string} Whole-second input preserving its optional negative sign. */
export function sanitizeRangeTimeInput(value) {
  const input = value.trim()
  const sign = input.startsWith('-') ? '-' : ''
  const sanitized = input
    .split(':')
    .map((part) => part.split(/[.,]/)[0].replace(/\D/g, ''))
    .join(':')
  return `${sign}${sanitized}`
}

/** @param {'transparent'|'composite'} exportMode Render mode. @returns {string} Native container extension. */
export function getRenderOutputExtension(exportMode) {
  return exportMode === 'composite' ? 'mp4' : 'mov'
}

/** @param {string} outputPath User-selected path. @param {'transparent'|'composite'} exportMode Render mode. @returns {string} Path with its container extension. */
export function normalizeRenderOutputPath(outputPath, exportMode) {
  const extension = getRenderOutputExtension(exportMode)
  const separatorIndex = Math.max(outputPath.lastIndexOf('/'), outputPath.lastIndexOf('\\'))
  const directory = outputPath.slice(0, separatorIndex + 1)
  const filename = outputPath.slice(separatorIndex + 1)
  return `${directory}${filename.replace(/\.[^.]*$/, '')}.${extension}`
}

/**
 * Translates backend progress into the shared frontend progress model.
 * @param {object} data Backend render progress payload.
 * @returns {object} Render progress for the current-video and batch panels.
 */
export function createRenderProgress(data) {
  return {
    renderId: data.render_id,
    current: data.current,
    total: data.total,
    percent: data.total > 0 ? (data.current / data.total) * 100 : 0,
    encoded: data.encoded,
    status: data.status,
    message: data.message,
    estimatedSecondsRemaining: data.estimated_seconds_remaining,
    renderingFps: data.rendering_fps,
    filename: data.filename,
  }
}

/**
 * Presents authoritative processed work without counting failures as rendered frames.
 * @param {object|null} snapshot Native batch snapshot.
 * @returns {object} Batch panel progress.
 */
export function createBatchProgress(snapshot) {
  if (snapshot === null) return DEFAULT_RENDER_PROGRESS
  return {
    current: snapshot.processedFrames,
    total: snapshot.plannedFrames,
    percent: snapshot.plannedFrames > 0 ? (snapshot.processedFrames / snapshot.plannedFrames) * 100 : 0,
    encoded: snapshot.encodedFrames,
    status: snapshot.phase,
    estimatedSecondsRemaining: snapshot.estimatedSecondsRemaining,
    renderingFps: snapshot.currentItemProgress?.renderingFps ?? null,
  }
}

/**
 * Formats native current-item progress for the row presentation.
 * @param {object|null} snapshot Native batch snapshot.
 * @returns {object} Active row progress.
 */
export function createBatchItemProgress(snapshot) {
  const progress = snapshot?.currentItemProgress
  if (!progress) return DEFAULT_RENDER_PROGRESS
  return {
    current: progress.currentFrames,
    total: progress.plannedFrames,
    percent: progress.plannedFrames > 0 ? (progress.currentFrames / progress.plannedFrames) * 100 : 0,
    encoded: progress.encodedFrames,
    status: snapshot.phase,
    estimatedSecondsRemaining: progress.estimatedSecondsRemaining,
    renderingFps: progress.renderingFps,
  }
}

/** @param {number} percent Completion percentage. @returns {string} Whole-percent display label. */
export function formatProgressPercent(percent) {
  return Math.round(percent).toString()
}

/** @param {object} inputs Editor synchronization inputs. @returns {object} Review automatic synchronization inputs or an actionable error. */
export function reviewBatchSync(inputs) {
  try {
    return { ...captureBatchSync(inputs), error: null }
  } catch (error) {
    return { error: error.message }
  }
}

/** @param {object[]} choices User queue. @param {object|null} inspection Current descriptors. @param {object} sync Automatic synchronization context. @param {string} locale Display language. @returns {object[]} Reviewed rows. */
export function reviewBatchQueue(choices, inspection, sync, locale) {
  return choices.map((item) => {
    if (inspection?.error && inspection.error !== 'reinspectionRequired') return { ...item, status: 'blocked', error: inspection.error }
    const result = inspection?.sources.get(item.path)
    if (!result) return { ...item, status: 'checking', error: null }
    if (result.error || sync.error) return { ...item, status: 'blocked', error: result.error ?? sync.error }
    const { metadata } = result.source
    const inspectedItem = { ...item, source: result.source, durationLabel: formatClockDuration(metadata.duration) }
    try {
      const [creationDateLabel, creationTimeLabel] = formatVideoCreationTime(
        metadata.creationTime,
        metadata.timeSource,
        sync.activitySummary?.timezone ?? null,
        sync.timezoneMode,
        locale,
      ).split(/(?=\d{2}:\d{2}:\d{2}$)/)
      inspectedItem.creationDateLabel = creationDateLabel.trimEnd()
      inspectedItem.creationTimeLabel = creationTimeLabel
      const { timing, hasPositiveOverlap } = resolveBatchVideoTiming(metadata, sync.activitySummary, sync.timezoneMode)
      return {
        ...inspectedItem,
        timing,
        status: hasPositiveOverlap === false ? 'blocked' : 'pending',
        error: hasPositiveOverlap === false ? i18next.t('store.videoCouldNotBeSyncedWithActivity', 'Video could not be synced with activity') : null,
      }
    } catch (error) {
      return { ...inspectedItem, status: 'blocked', error: error.message }
    }
  })
}

/** @param {object[]} items Native queue items. @returns {object[]} Submitted item presentation; frontend job IDs are validated source paths. */
export function batchResultQueue(items) {
  return items.map((item) => {
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

/**
 * Formats the current render's settings for the shared progress panel.
 * @param {object} ctx Render dialog state.
 * @param {function} t Translation function.
 * @returns {string[]} Compact render summary fragments.
 */
export function getRenderSummaryItems(ctx, t) {
  const isCompositeExport = ctx.exportMode === 'composite'
  const fps = isCompositeExport && ctx.importedVideoFps !== null ? Math.round(ctx.importedVideoFps) : ctx.settings.fps
  const outputFormatLabel = ctx.OUTPUT_FORMATS.find((option) => option.value === ctx.selectedOutputFormatValue)?.label
  const accelerationLabel = ctx.selectedAccelerationOptions.find(
    (option) => option.value === ctx.selectedAccelerationValue && option.available && option.value !== 'cpu',
  )?.label
  const durationSeconds = isCompositeExport
    ? ctx.importedVideoDuration
    : ctx.settings.range?.type === 'custom'
      ? ctx.settings.range.to - ctx.settings.range.from
      : ctx.config?.scene?.start !== undefined && ctx.config?.scene?.end !== undefined
        ? ctx.config.scene.end - ctx.config.scene.start
        : null
  const renderSummaryItems = [
    ctx.config?.scene?.width && ctx.config?.scene?.height ? `${ctx.config.scene.width}x${ctx.config.scene.height}` : null,
    `${fps} fps`,
    t('render-video.update1val', { defaultValue: 'Update 1/{{val}}', val: ctx.settings.widgetUpdateRate }),
    outputFormatLabel || ctx.settings.codec || null,
    accelerationLabel || null,
    durationSeconds !== null ? formatDurationSummary(durationSeconds, t) : null,
  ].filter(Boolean)
  return renderSummaryItems
}

function formatDurationSummary(durationSeconds, t) {
  const roundedSeconds = Math.round(durationSeconds)
  const minutes = Math.floor(roundedSeconds / 60)
  const seconds = roundedSeconds % 60

  if (minutes > 0) {
    return t('render-video.minutesMinSecondsSec', { defaultValue: '{{minutes}} min {{seconds}} sec', minutes, seconds })
  }

  return t('render-video.secondsSec', { defaultValue: '{{seconds}} sec', seconds })
}
