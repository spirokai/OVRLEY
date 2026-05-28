import { getCurrentWindow } from '@tauri-apps/api/window'
import { Minus, Square, X } from 'lucide-react'

/**
 * Renders the custom title bar for the Tauri window with minimize, maximize, and close buttons.
 * @returns {JSX.Element} Rendered component.
 */
export default function TitleBar() {
  const appWindow = getCurrentWindow()

  return (
    <div data-tauri-drag-region className="flex h-6 shrink-0 items-center justify-between bg-background select-none">
      <div className="flex items-center pl-4"></div>

      <div className="flex h-full items-center" style={{ pointerEvents: 'auto' }}>
        <button
          className="flex h-full w-12 items-center justify-center text-muted-foreground hover:bg-surface-elevated hover:text-foreground transition-colors"
          onClick={() => appWindow.minimize()}
          aria-label="Minimize"
        >
          <Minus className="h-4 w-4" />
        </button>
        <button
          className="flex h-full w-12 items-center justify-center text-muted-foreground hover:bg-surface-elevated hover:text-foreground transition-colors"
          onClick={() => appWindow.toggleMaximize()}
          aria-label="Maximize"
        >
          <Square className="h-3.5 w-3.5" />
        </button>
        <button
          className="flex h-full w-12 items-center justify-center text-muted-foreground hover:bg-destructive hover:text-white transition-colors"
          onClick={() => appWindow.close()}
          aria-label="Close"
        >
          <X className="h-4 w-4" />
        </button>
      </div>
    </div>
  )
}
