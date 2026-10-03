import { useRasterPreview } from './useRasterPreview'

/** @param {{widget: object, globalOpacity: number}} props */
export default function RasterPreview({ widget, globalOpacity }) {
  const previewPng = useRasterPreview(widget.data.resourceId)
  const opacity = widget.data.opacity * globalOpacity
  if (widget.data.path === null || widget.data.resourceErrorCode) {
    return <div data-testid="raster-placeholder" className="h-full w-full bg-white" style={{ opacity: opacity * 0.5 }} />
  }
  if (!widget.data.resourceId) throw new Error(`Raster ${widget.id} has no loaded image resource`)
  if (!previewPng) return null
  return (
    <img
      src={`data:image/png;base64,${previewPng}`}
      alt=""
      draggable="false"
      className="block h-full w-full"
      style={{ opacity, objectFit: 'fill' }}
    />
  )
}
