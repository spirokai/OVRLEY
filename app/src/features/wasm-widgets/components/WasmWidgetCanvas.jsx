import { useWasmCanvas } from '../hooks/useWasmCanvas'

/** Stacking matches export: backdrops, maps, rasters, profiles, then everything else. */
const WIDGET_LAYERS = { backdrop: 'z-1', map: 'z-2', raster: 'z-3', course: 'z-4', elevation: 'z-4', diving_profile: 'z-4' }

/** @param {object} props Standalone widget canvas registration. @returns {JSX.Element} Rust-positioned canvas. */
export function WasmWidgetCanvas({ widget, attach, attachMapLayer, registerNode, handleWidgetMouseDown }) {
  const separateRaster = widget.type === 'course' || widget.type === 'elevation' || widget.type === 'diving_profile'
  const { wrapperRef, canvasRef, mapLayerRef } = useWasmCanvas({ id: widget.id, attach, attachMapLayer, registerNode, separateRaster })
  const layer = WIDGET_LAYERS[widget.type] ?? 'z-10'
  const canvas = (
    <canvas
      ref={canvasRef}
      data-wasm-widget-id={widget.id}
      aria-label={`${widget.type} preview`}
      className={`pointer-events-none absolute ${separateRaster ? layer : ''}`}
    />
  )
  return (
    <>
      <div
        ref={wrapperRef}
        data-widget-id={widget.id}
        className={`absolute cursor-move select-none overflow-visible ${layer}`}
        style={{ visibility: 'hidden' }}
        onMouseDown={(event) => handleWidgetMouseDown(event, widget.id)}
      >
        {widget.type === 'map' ? <div ref={mapLayerRef} className="pointer-events-none absolute inset-0" /> : null}
        {separateRaster ? null : canvas}
      </div>
      {separateRaster ? canvas : null}
    </>
  )
}
