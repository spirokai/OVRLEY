import { useWasmCanvas } from '../hooks/useWasmCanvas'

/** @param {object} props Standalone widget canvas registration. @returns {JSX.Element} Rust-positioned canvas. */
export function WasmWidgetCanvas({ widget, attach, registerNode, handleWidgetMouseDown }) {
  const ref = useWasmCanvas({ id: widget.id, attach })
  return (
    <div
      ref={registerNode}
      data-widget-id={widget.id}
      className={`absolute cursor-move select-none overflow-visible ${widget.type === 'backdrop' ? 'z-1' : widget.type === 'raster' ? 'z-2' : 'z-10'}`}
      style={{ visibility: 'hidden' }}
      onMouseDown={(event) => handleWidgetMouseDown(event, widget.id)}
    >
      <canvas ref={ref} data-wasm-widget-id={widget.id} aria-label={`${widget.type} preview`} className="pointer-events-none absolute" />
    </div>
  )
}
