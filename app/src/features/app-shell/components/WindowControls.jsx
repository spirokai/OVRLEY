import { Minus, Square, X } from 'lucide-react'
import { useEffect, useState } from 'react'

const noopWindow = {
  minimize: () => {},
  toggleMaximize: () => {},
  close: () => {},
}

/**
 * Window chrome controls for desktop environments.
 * Lazily loads the Tauri window API and falls back to no-op buttons in the browser.
 * Returns nothing on macOS — the OS provides native traffic light buttons.
 */
export default function WindowControls() {
  const [appWindow, setAppWindow] = useState(noopWindow)

  useEffect(() => {
    let cancelled = false
    import('@tauri-apps/api/window')
      .then((mod) => {
        if (!cancelled) setAppWindow(mod.getCurrentWindow())
      })
      .catch(() => {})
    return () => {
      cancelled = true
    }
  }, [])

  if (/mac/i.test(navigator.platform)) {
    return null
  }

  return (
    <div className="ml-6 flex items-center">
      <div className="flex h-9 items-center">
        <button
          className="flex h-full w-10 items-center justify-center text-muted-foreground transition-colors hover:bg-surface-elevated hover:text-foreground"
          onClick={() => appWindow.minimize()}
          aria-label="Minimize"
          type="button"
        >
          <Minus className="h-4 w-4" />
        </button>
        <div className="h-6 w-px bg-border/50" />
        <button
          className="flex h-full w-10 items-center justify-center text-muted-foreground transition-colors hover:bg-surface-elevated hover:text-foreground"
          onClick={() => appWindow.toggleMaximize()}
          aria-label="Maximize"
          type="button"
        >
          <Square className="h-3.5 w-3.5" />
        </button>
        <div className="h-6 w-px bg-border/50" />
        <button
          className="flex h-full w-10 items-center justify-center text-muted-foreground transition-colors hover:bg-destructive hover:text-white"
          onClick={() => appWindow.close()}
          aria-label="Close"
          type="button"
        >
          <X className="h-4 w-4" />
        </button>
      </div>
    </div>
  )
}
