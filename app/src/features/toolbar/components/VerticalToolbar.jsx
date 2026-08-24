import { Blocks, FolderOpen, Activity, Film } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { SimpleTooltip } from '@/components/ui/simple-tooltip'
import { WIDGETS_TOOL } from '@/store/slices/createLayoutSlice'

const TOOLS = [
  {
    id: 'PROJECTS',
    label: 'Projects',
    icon: FolderOpen,
  },
  { id: 'ACTIVITY', label: 'Activity', icon: Activity },
  { id: 'VIDEOS', label: 'Videos', icon: Film },
  {
    id: WIDGETS_TOOL,
    label: 'Widgets',
    icon: Blocks,
  },
]

/**
 * Renders the full-height application tool rail.
 *
 * @param {object} props - Component props.
 * @param {string} props.activeTool - Canonical active tool identifier.
 * @param {boolean} props.drawerVisible - Whether the shared drawer is visible.
 * @param {string} props.width - Toolbar width allocated by the shell layout.
 * @param {(tool: string) => void} props.onSelectTool - Selects a toolbar tool.
 * @returns {JSX.Element} Rendered toolbar.
 */
export function VerticalToolbar({ activeTool, drawerVisible, width, onSelectTool }) {
  return (
    <div className="z-70 flex h-full shrink-0 flex-col items-center border-r border-border bg-card py-4 gap-2" style={{ width }}>
      {TOOLS.map((tool) => {
        const Icon = tool.icon
        const selected = drawerVisible && activeTool === tool.id

        return (
          <SimpleTooltip key={tool.id} side="right" content={tool.label}>
            <Button
              type="button"
              variant={selected ? 'default' : 'ghost'}
              size="icon"
              className="h-9 w-9"
              aria-label={tool.label}
              aria-pressed={selected}
              onClick={() => onSelectTool(tool.id)}
            >
              <Icon className="size-4" />
            </Button>
          </SimpleTooltip>
        )
      })}
    </div>
  )
}
