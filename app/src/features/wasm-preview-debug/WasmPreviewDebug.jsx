import { useWasmPreviewDebug } from './hooks/useWasmPreviewDebug'

/** @returns {JSX.Element} Development renderer diagnostics. */
export default function WasmPreviewDebug() {
  const { canvasRef, status, timing } = useWasmPreviewDebug()
  return (
    <main className="min-h-screen space-y-6 bg-background p-8 text-foreground">
      <h1 className="text-2xl font-semibold">Wasm preview diagnostics</h1>
      <p role="status">{status}</p>
      <canvas ref={canvasRef} className="max-w-full rounded border" aria-label="Rust Skia diagnostic surface" />
      {timing && (
        <p>
          Mean of 120 frames: draw {timing.draw} ms · copy {timing.copy} ms · present {timing.present} ms · total {timing.total} ms
        </p>
      )}
      <p>Edit Rust to rebuild and reload. React edits use normal hot updates.</p>
      <a href="#/">Back to editor</a>
    </main>
  )
}
