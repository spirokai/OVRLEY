import { Activity, Blocks, Film, FolderKanban } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { SimpleTooltip } from '@/components/ui/simple-tooltip'
import { useTranslation } from 'react-i18next'
import { ACTIVITY_TOOL, PROJECTS_TOOL, VIDEO_TOOL, WIDGETS_TOOL } from '@/store/slices/createLayoutSlice'

/**
 * Canonical toolbar tool definitions. Components resolve `labelKey` through
 * i18n at render time so this data contains no user-facing copy.
 */
const TOOL_DEFINITIONS = [
  { id: PROJECTS_TOOL, labelKey: 'toolbar.projects', icon: FolderKanban },
  { id: ACTIVITY_TOOL, labelKey: 'toolbar.activity', icon: Activity },
  { id: VIDEO_TOOL, labelKey: 'toolbar.video', icon: Film },
  { id: WIDGETS_TOOL, labelKey: 'toolbar.widgets', icon: Blocks },
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
  const { t } = useTranslation()

  return (
    <div className="z-70 flex h-full shrink-0 flex-col items-center border-r border-border bg-card py-4 gap-2" style={{ width }}>
      {TOOL_DEFINITIONS.map((tool) => {
        const Icon = tool.icon
        const label = t(tool.labelKey)
        const selected = drawerVisible && activeTool === tool.id
        return (
          <SimpleTooltip key={tool.id} side="right" content={label}>
            <Button
              type="button"
              variant={selected ? 'default' : 'ghost'}
              size="icon"
              className="h-9 w-9"
              aria-label={label}
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
