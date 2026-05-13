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
import { getContainerFps, getUpdateRateOptions } from '@/lib/update-rate'
import { getDefaultBitrate } from '@/lib/bitrateDefaults'

export default function useRenderVideoDerivedState({ settings }) {
  // Store selectors
  const renderingVideo = useStore((state) => state.renderingVideo)
  const platformOs = useStore((state) => state.platformOs)
  const availableCodecs = useStore((state) => state.availableCodecs)
  const config = useStore((state) => state.config)
  const importedVideoPath = useStore((state) => state.importedVideoPath)
  const importedVideoFps = useStore((state) => state.importedVideoFps)
  const importedVideoResolution = useStore((state) => state.importedVideoResolution)
  const renderProgress = useStore((state) => state.renderProgress)

  // Derived values — computed FPS options, codec availability flags, and render readiness state
  const updateRateOptions = useMemo(() => getUpdateRateOptions(settings?.fps), [settings?.fps])
  const containerFps = useMemo(() => getContainerFps(settings?.fps, settings?.updateRate), [settings?.fps, settings?.updateRate])
  const hasImportedVideo = Boolean(importedVideoPath)
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
    (hasImportedVideo && (!selectedCodecIsMp4 || !selectedExportCodecAvailable)) ||
    (!hasImportedVideo && selectedCodecIsMp4)

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
    hasImportedVideo,
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
    updateRateOptions,
  }
}
