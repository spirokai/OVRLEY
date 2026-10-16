import { DEFAULT_RENDER_PROGRESS } from '@/store/store-utils'
import i18next from 'i18next'
import { videoOverlapsActivity } from '@/lib/video-timing'
import { formatClockDuration } from '@/lib/time-format'
import { formatVideoCreationTime } from '@/features/scene-settings/utils/sceneSettingsUtils'
import { captureBatchSync } from './renderRequest'

/** @param {{startSecond: number, endSecond: number}} videoBounds Committed interval covered by videos. @returns {object} Activity-timeline export range. */
export function getImportedVideoExportRange(videoBounds) {
  return { type: 'custom', from: videoBounds.startSecond, to: videoBounds.endSecond }
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

/**
 * Formats a retained render path error in the current UI language.
 * @param {object|null} error Structured path error or an error with a diagnostic message.
 * @param {function} translate Current translation function.
 * @returns {string|null} Error text for presentation.
 */
export function formatRenderPathError(error, translate) {
  if (error === null) return null
  return error.reason === undefined ? error.message : translate(`render-video.pathErrors.${error.reason}`, error)
}

/** @param {object} inputs Editor synchronization inputs. @returns {object} Review automatic synchronization inputs or an actionable error. */
export function reviewBatchSync(inputs) {
  try {
    return { ...captureBatchSync(inputs), error: null }
  } catch (error) {
    return { error }
  }
}

// Camera timestamps are external data; an unreadable one only hides the date.
function formatCreationLabels(clip, sync, locale) {
  try {
    const [creationDateLabel, creationTimeLabel] = formatVideoCreationTime(
      clip.creationTime,
      clip.timeSource,
      sync.activitySummary?.timezone ?? null,
      sync.timezoneMode,
      locale,
    ).split(/(?=\d{2}:\d{2}:\d{2}$)/)
    return { creationDateLabel: creationDateLabel.trimEnd(), creationTimeLabel }
  } catch (error) {
    if (!(error instanceof RangeError)) throw error
    return { creationDateLabel: null, creationTimeLabel: null }
  }
}

/**
 * Reviews every imported video for separate-file export. A video renders at its
 * committed timeline offset; with an external activity it must overlap the activity.
 * @param {object[]} clips Timeline clips in timeline order.
 * @param {Object<string, boolean>} skipOverlay Video IDs that render without activity widgets.
 * @param {object|null} inspection Current native descriptors keyed by path.
 * @param {object} sync Activity summary and display timezone mode.
 * @param {string} locale Display language.
 * @returns {object[]} Reviewed rows keyed by video ID.
 */
export function reviewBatchQueue(clips, skipOverlay, inspection, sync, locale) {
  return clips.map((clip) => {
    const item = {
      id: clip.id,
      path: clip.path,
      filename: clip.path.split(/[/\\]/).at(-1),
      offsetSeconds: clip.offsetSeconds,
      skipOverlay: skipOverlay[clip.id] === true,
    }
    if (inspection?.error && inspection.error.message !== 'reinspectionRequired')
      return { ...item, status: 'blocked', error: formatRenderPathError(inspection.error, i18next.getFixedT(locale)) }
    const result = inspection?.sources.get(clip.path)
    if (!result) return { ...item, status: 'checking', error: null }
    if (result.error || sync.error)
      return { ...item, status: 'blocked', error: formatRenderPathError(result.error ?? sync.error, i18next.getFixedT(locale)) }
    const inspectedItem = {
      ...item,
      source: result.source,
      durationLabel: formatClockDuration(clip.durationSeconds),
      ...formatCreationLabels(clip, sync, locale),
    }
    const outsideActivity =
      sync.activitySummary !== null &&
      !videoOverlapsActivity({
        videoStart: clip.offsetSeconds,
        videoDuration: clip.durationSeconds,
        activityEnd: sync.activitySummary.durationSeconds,
      })
    return {
      ...inspectedItem,
      status: outsideActivity ? 'blocked' : 'pending',
      error: outsideActivity ? i18next.t('store.videoOutsideActivity', 'Video is outside the activity') : null,
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
    ? ctx.videoBounds === null
      ? null
      : ctx.videoBounds.endSecond - ctx.videoBounds.startSecond
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
