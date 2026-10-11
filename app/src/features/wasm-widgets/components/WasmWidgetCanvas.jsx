import { useWasmCanvas } from '../hooks/useWasmCanvas'

/** @param {object} props Standalone widget canvas registration. @returns {JSX.Element} Rust-positioned canvas. */
export function WasmWidgetCanvas({ widget, attach, registerNode }) {
  const ref = useWasmCanvas({ id: widget.id, attach, registerNode })
  return <canvas ref={ref} data-wasm-widget-id={widget.id} aria-label="Speed preview" className="pointer-events-none absolute z-10" />
}
