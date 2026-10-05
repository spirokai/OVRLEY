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
    : ctx.settings.exportRange?.type === 'custom'
      ? ctx.settings.exportRange.to - ctx.settings.exportRange.from
      : ctx.config?.scene?.start !== undefined && ctx.config?.scene?.end !== undefined
        ? ctx.config.scene.end - ctx.config.scene.start
        : null
  const renderSummaryItems = [
    ctx.config?.scene?.width && ctx.config?.scene?.height ? `${ctx.config.scene.width}x${ctx.config.scene.height}` : null,
    `${fps} fps`,
    t('render-video.update1val', { defaultValue: 'Update 1/{{val}}', val: ctx.settings.updateRate }),
    outputFormatLabel || ctx.settings.exportCodec || null,
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
