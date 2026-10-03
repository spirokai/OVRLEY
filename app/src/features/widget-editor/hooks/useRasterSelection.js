import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { loadSelectedRaster } from '@/api/backend'
import { openSinglePath } from '@/lib/file-dialog'
import useStore from '@/store/useStore'

const RASTER_ERROR_KEYS = {
  relative_path: 'relativePath',
  missing: 'missing',
  unreadable: 'unreadable',
  unsupported_type: 'unsupportedType',
  encoded_size: 'encodedSize',
  resolution: 'resolution',
  decode: 'decode',
  missing_asset: 'missingAsset',
  corrupt_asset: 'corruptAsset',
}

/** @param {string} widgetId - Raster widget being edited. */
export function useRasterSelection(widgetId) {
  const { t } = useTranslation()
  const setRasterImage = useStore((state) => state.setRasterImage)
  const resourceErrorCode = useStore((state) => state.config.rasters.find((raster) => raster.id === widgetId)?.resourceErrorCode)
  const [selectionError, setSelectionError] = useState(null)
  const [isLoading, setIsLoading] = useState(false)

  const selectImage = async () => {
    setSelectionError(null)
    setIsLoading(true)
    try {
      const path = await openSinglePath([{ name: t('raster.imageFiles'), extensions: ['png', 'jpeg', 'jpg', 'bmp', 'tiff'] }])
      if (!path) return
      const result = await loadSelectedRaster(path)
      setRasterImage(widgetId, path, result)
    } catch (cause) {
      if (!cause.code) throw cause
      const key = RASTER_ERROR_KEYS[cause.code]
      const message = t(`raster.errors.${key}`)
      setSelectionError(message)
    } finally {
      setIsLoading(false)
    }
  }

  const resourceErrorKey = RASTER_ERROR_KEYS[resourceErrorCode]
  const error = selectionError || (resourceErrorKey ? t(`raster.errors.${resourceErrorKey}`) : null)
  return { error, isLoading, selectImage }
}
