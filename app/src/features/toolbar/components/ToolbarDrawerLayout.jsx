import { Pin, PinOff } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { VerticalToolbar } from './VerticalToolbar'

const DRAWER_WIDTH = '20rem'
const TOOLBAR_WIDTH = '3rem'

/**
 * Composes the toolbar, shared drawer allocation, workspace, and trailing panel.
 *
 * @param {object} props - Component props.
 * @param {string} props.activeTool - Canonical active tool identifier.
 * @param {boolean} props.allocationTransitioning - Whether pinned allocation is moving.
 * @param {React.ReactNode} props.controlPanel - Trailing shell control panel.
 * @param {React.ReactNode} props.drawerContent - Active drawer tool content.
 * @param {boolean} props.pinned - Whether the drawer occupies its grid track.
 * @param {boolean} props.visible - Whether the drawer is open.
 * @param {React.ReactNode} props.workspace - Scene canvas and timeline content.
 * @param {() => void} props.dismissOverlay - Dismisses a temporary drawer.
 * @param {(tool: string) => void} props.selectTool - Selects a toolbar tool.
 * @param {(pinned: boolean) => void} props.setPinned - Changes drawer mode.
 * @param {(event: React.TransitionEvent) => void} props.handleAllocationTransitionEnd - Completes pinned allocation movement.
 * @param {(event: React.TransitionEvent) => void} props.handleDrawerTransitionEnd - Completes the drawer exit lifecycle.
 * @returns {JSX.Element} Rendered shell row.
 */
export function ToolbarDrawerLayout({
  activeTool,
  allocationTransitioning,
  controlPanel,
  drawerContent,
  pinned,
  visible,
  workspace,
  dismissOverlay,
  selectTool,
  setPinned,
  handleAllocationTransitionEnd,
  handleDrawerTransitionEnd,
}) {
  const PinIcon = pinned ? PinOff : Pin
  const pinLabel = pinned ? 'Unpin drawer' : 'Pin drawer'

  return (
    <div
      className="relative grid min-h-0 flex-1 overflow-hidden transition-[grid-template-columns] duration-200 ease-in-out"
      style={{ gridTemplateColumns: `${TOOLBAR_WIDTH} ${pinned ? DRAWER_WIDTH : '0rem'} minmax(0, 1fr) auto` }}
      onTransitionEnd={handleAllocationTransitionEnd}
    >
      <VerticalToolbar activeTool={activeTool} drawerVisible={visible} width={TOOLBAR_WIDTH} onSelectTool={selectTool} />
      <div className="relative h-full min-w-0">
        <div
          className={`absolute inset-y-0 left-0 z-60 flex flex-col overflow-hidden border-r border-border/70 bg-card shadow-lg shadow-black/80 transition-transform duration-300 ease-in-out ${
            visible ? 'pointer-events-auto translate-x-0' : 'pointer-events-none -translate-x-full'
          }`}
          style={{ width: DRAWER_WIDTH }}
          aria-hidden={!visible}
          onTransitionEnd={handleDrawerTransitionEnd}
        >
          <div className="absolute top-2 right-2 z-10">
            <Button
              type="button"
              variant="ghost"
              size="icon"
              className="h-6 w-6"
              aria-label={pinLabel}
              disabled={allocationTransitioning}
              onClick={() => setPinned(!pinned)}
            >
              <PinIcon className="size-3.5" />
            </Button>
          </div>
          <div className="flex min-h-0 flex-1 flex-col pt-10">{drawerContent}</div>
        </div>
      </div>
      {visible && !pinned ? (
        <div className="absolute inset-y-0 right-0 z-55" style={{ left: TOOLBAR_WIDTH }} data-slot="left-drawer-backdrop" onClick={dismissOverlay} />
      ) : null}
      <div className="relative flex min-w-0 flex-col bg-surface-darken">
        {allocationTransitioning ? <div className="absolute inset-0 z-55" data-slot="workspace-transition-blocker" aria-hidden="true" /> : null}
        {workspace}
      </div>
      {controlPanel}
    </div>
  )
}
