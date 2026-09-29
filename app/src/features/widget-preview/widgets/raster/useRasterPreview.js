import { useEffect, useState } from 'react'
import { rasterPreviewPng } from '@/api/backend'

/** Load display bytes from the immutable image held by Rust. */
export function useRasterPreview(resourceId) {
  const [preview, setPreview] = useState({ resourceId: null, pngBase64: null, error: null })

  useEffect(() => {
    if (!resourceId) return
    let active = true
    rasterPreviewPng(resourceId).then(
      (pngBase64) => {
        if (active) setPreview({ resourceId, pngBase64, error: null })
      },
      (error) => {
        if (active) setPreview({ resourceId, pngBase64: null, error })
      },
    )
    return () => {
      active = false
    }
  }, [resourceId])

  if (preview.resourceId !== resourceId) return null
  if (preview.error) throw preview.error
  return preview.pngBase64
}
