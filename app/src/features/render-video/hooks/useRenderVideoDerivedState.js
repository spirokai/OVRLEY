/**
 * Reads render store state and computes all derived values needed by
 * RenderVideoDialog. Single concern: data access + transformation.
 * Composed by useRenderVideoDialogState.
 *
 * @param {object} params
 * @param {object|null} params.settings - Current render settings draft.
 * @returns {object} Store values and derived render dialog state.
 */

import { useCallback, useMemo } from 'react'
import useStore from '@/store/useStore'
import { EXPORT_CODEC_LOOKUP, OUTPUT_FORMATS_BY_VALUE } from '../data/renderConstants'
import {
  getAccelerationValueForSettings,
  getOutputFormatForExportCodec,
  getVisibleAccelerationOptions,
  isMp4Codec,
  resolutionsMismatch,
} from '../utils/codecUtils'
import { getContainerFps, getUpdateRateOptions, sanitizeIntegerFps } from '@/lib/update-rate'
import { getDefaultBitrate } from '../data/bitrateDefaults'

export default function useRenderVideoDerivedState({ settings }) {
  const renderingVideo = useStore((state) => state.renderingVideo)
  const platformOs = useStore((state) => state.platformOs)
  const availableCodecs = useStore((state) => state.availableCodecs)
  const config = useStore((state) => state.config)
  const importedVideoPath = useStore((state) => state.importedVideoPath)
  const importedVideoDuration = useStore((state) => state.importedVideoDuration)
  const importedVideoFps = useStore((state) => state.importedVideoFps)
  const importedVideoResolution = useStore((state) => state.importedVideoResolution)
  const videoSyncOffsetSeconds = useStore((state) => state.videoSyncOffsetSeconds)
  const renderProgress = useStore((state) => state.renderProgress)

  const hasImportedVideo = Boolean(importedVideoPath)
  const exportMode = settings?.exportMode || (hasImportedVideo ? 'composite' : 'transparent')
  const updateRateFps = useMemo(
    () => (exportMode === 'composite' && importedVideoFps ? sanitizeIntegerFps(Math.round(importedVideoFps)) : settings?.fps),
    [exportMode, importedVideoFps, settings?.fps],
  )
  const updateRateOptions = useMemo(() => getUpdateRateOptions(updateRateFps), [updateRateFps])
  const containerFps = useMemo(() => getContainerFps(updateRateFps, settings?.updateRate), [updateRateFps, settings?.updateRate])
  const selectedOutputFormat = getOutputFormatForExportCodec(settings?.exportCodec)
  const selectedOutputFormatValue = selectedOutputFormat?.value || 'prores'
  const selectedAccelerationValue = getAccelerationValueForSettings(settings)
  const selectedAccelerationOptions = useMemo(
    () => getVisibleAccelerationOptions(OUTPUT_FORMATS_BY_VALUE[selectedOutputFormatValue], platformOs, availableCodecs),
    [availableCodecs, platformOs, selectedOutputFormatValue],
  )
  const selectedCodecIsMp4 = isMp4Codec(settings?.exportCodec)
  const selectedAccelerationAvailable = Boolean(selectedAccelerationOptions.find((option) => option.value === selectedAccelerationValue)?.available)
  const selectedExportCodecAvailable = Boolean(EXPORT_CODEC_LOOKUP[settings?.exportCodec]) && selectedAccelerationAvailable
  const resolutionMismatch = resolutionsMismatch(config?.scene, importedVideoResolution)
  const renderStartDisabled =
    renderingVideo ||
    resolutionMismatch ||
    (exportMode === 'composite' && (!selectedCodecIsMp4 || !selectedExportCodecAvailable)) ||
    (exportMode !== 'composite' && selectedCodecIsMp4)

  const defaultBitrateForCodec = useCallback(
    (codec) =>
      getDefaultBitrate(
        importedVideoResolution?.width || config?.scene?.width,
        importedVideoResolution?.height || config?.scene?.height,
        importedVideoFps || settings?.fps,
        codec,
      ),
    [config?.scene?.height, config?.scene?.width, importedVideoFps, importedVideoResolution?.height, importedVideoResolution?.width, settings?.fps],
  )

  return {
    availableCodecs,
    config,
    containerFps,
    defaultBitrateForCodec,
    exportMode,
    hasImportedVideo,
    importedVideoDuration,
    importedVideoFps,
    importedVideoResolution,
    platformOs,
    renderProgress,
    renderStartDisabled,
    renderingVideo,
    resolutionMismatch,
    selectedAccelerationOptions,
    selectedAccelerationValue,
    selectedCodecIsMp4,
    selectedExportCodecAvailable,
    selectedOutputFormatValue,
    updateRateFps,
    updateRateOptions,
    videoSyncOffsetSeconds,
  }
}
