import { useWasmCanvas } from '../hooks/useWasmCanvas'

/** @param {object} props Standalone widget canvas registration; hidden canvases stay editable. @returns {JSX.Element} Rust-positioned canvas. */
export function WasmWidgetCanvas({ widget, attach, registerNode, handleWidgetMouseDown, isHidden }) {
  const separateRaster = widget.type === 'course' || widget.type === 'elevation' || widget.type === 'diving_profile'
  const { wrapperRef, canvasRef } = useWasmCanvas({ id: widget.id, attach, registerNode, separateRaster })
  const canvas = (
    <canvas
      ref={canvasRef}
      data-wasm-widget-id={widget.id}
      aria-label={`${widget.type} preview`}
      className={`pointer-events-none absolute ${separateRaster ? 'z-3' : ''} ${isHidden ? 'invisible' : ''}`}
    />
  )
  return (
    <>
      <div
        ref={wrapperRef}
        data-widget-id={widget.id}
        className={`absolute cursor-move select-none overflow-visible ${widget.type === 'backdrop' ? 'z-1' : widget.type === 'raster' ? 'z-2' : separateRaster ? 'z-3' : 'z-10'}`}
        style={{ visibility: 'hidden' }}
        onMouseDown={(event) => handleWidgetMouseDown(event, widget.id)}
      >
        {separateRaster ? null : canvas}
      </div>
      {separateRaster ? canvas : null}
    </>
  )
}
