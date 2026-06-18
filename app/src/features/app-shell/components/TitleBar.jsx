import WindowControls from './WindowControls'

/**
 * Legacy wrapper for the desktop window controls.
 */
export default function TitleBar() {
  return (
    <div data-tauri-drag-region className="flex h-6 shrink-0 items-center justify-between bg-background select-none">
      <div className="flex items-center pl-4"></div>
      <div className="flex h-full items-center" style={{ pointerEvents: 'auto' }}>
        <WindowControls />
      </div>
    </div>
  )
}
