/**
 * Public overlay player adapter.
 */

import useOverlayPlayer from '../hooks/useOverlayPlayer'
import PlayerToolbar from './PlayerToolbar'
import TimelineSurface from './TimelineSurface'

/**
 * Renders the overlay player portion of the application interface.
 *
 * @param {{ activeKeyboardWorkspace: string, backgroundMode: string, onActivateKeyboardWorkspace: Function }} props
 */
export default function OverlayPlayer({ activeKeyboardWorkspace, backgroundMode, onActivateKeyboardWorkspace }) {
  const player = useOverlayPlayer({ activeKeyboardWorkspace, backgroundMode })

  if (!player.isVisible) {
    return null
  }

  return (
    <div
      className="shrink-0 border-border/70 bg-black/30 px-8 py-2 backdrop-blur-sm"
      onFocusCapture={onActivateKeyboardWorkspace}
      onPointerDownCapture={onActivateKeyboardWorkspace}
    >
      <PlayerToolbar toolbar={player.toolbar} />
      <TimelineSurface timeline={player.timeline} />
    </div>
  )
}
