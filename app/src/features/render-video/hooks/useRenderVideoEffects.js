/**
 * Side effects for RenderVideoDialog: codec auto-switching and update rate
 * normalization. Single concern: synchronization of committed render settings.
 * Composed by useRenderVideoDialogState.
 *
 * @param {object} params
 * @param {object|null} params.settings - Current render settings draft.
 * @param {object} params.derivedState - Output from useRenderVideoDerivedState.
 * @param {function} params.onSettingsChange - Callback to update settings draft.
 * @returns {void}
 */

import { useEffect } from 'react'
import { EXPORT_CODEC_LOOKUP } from '../data/renderConstants'
import { getFirstAvailableMp4ExportCodec } from '../utils/codecUtils'
import { normalizeUpdateRateForFps } from '@/lib/update-rate'

export default function useRenderVideoEffects({ settings, derivedState, onSettingsChange }) {
  const { defaultBitrateForCodec, hasImportedVideo, platformOs, availableCodecs, selectedCodecIsMp4, selectedExportCodecAvailable, updateRateFps } =
    derivedState

  // Auto-select MP4 codec when video is imported or codec availability changes
  useEffect(() => {
    if (!settings) {
      return
    }

    if (!hasImportedVideo && selectedCodecIsMp4) {
      onSettingsChange({
        exportCodec: 'prores_ks',
        exportAcceleration: 'cpu',
        exportBitrate: undefined,
      })
      return
    }

    if (!hasImportedVideo) {
      return
    }

    const firstAvailableMp4Codec = getFirstAvailableMp4ExportCodec(platformOs, availableCodecs)

    if (!selectedCodecIsMp4 || !selectedExportCodecAvailable) {
      if (firstAvailableMp4Codec) {
        onSettingsChange({
          exportCodec: firstAvailableMp4Codec,
          exportAcceleration: EXPORT_CODEC_LOOKUP[firstAvailableMp4Codec]?.acceleration || 'cpu',
          exportBitrate: defaultBitrateForCodec(firstAvailableMp4Codec),
        })
      }
      return
    }

    if (!Number.isFinite(settings.exportBitrate)) {
      onSettingsChange({
        exportBitrate: defaultBitrateForCodec(settings.exportCodec),
      })
    }
  }, [
    hasImportedVideo,
    defaultBitrateForCodec,
    onSettingsChange,
    platformOs,
    selectedCodecIsMp4,
    selectedExportCodecAvailable,
    settings,
    availableCodecs,
  ])

  // Normalize update rate when FPS changes
  useEffect(() => {
    if (!settings) {
      return
    }

    const normalizedUpdateRate = normalizeUpdateRateForFps(updateRateFps, settings.updateRate)
    if (normalizedUpdateRate !== settings.updateRate) {
      onSettingsChange({ updateRate: normalizedUpdateRate })
    }
  }, [settings, updateRateFps, onSettingsChange])
}
