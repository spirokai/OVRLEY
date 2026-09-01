/**
 * Left column of the app header — file menu for project and media import actions.
 * Pure presentational component.
 */

import { useState } from 'react'
import { Button } from '@/components/ui/button'
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover'
import { Separator } from '@/components/ui/separator'
import { Activity, FilePlus2, Film, FolderOpen, Menu, Save } from 'lucide-react'
import { useTranslation } from 'react-i18next'

/**
 * Renders a full-width action row inside the menu popover.
 *
 * @param {object} props
 * @param {object} props.icon - Lucide icon component.
 * @param {string} props.label - Item label.
 * @param {function} props.onClick - Action invoked on click.
 * @param {boolean} [props.disabled] - Renders the item as disabled.
 * @param {string} [props.shortcut] - ARIA keyboard shortcut.
 * @returns {JSX.Element} Rendered component.
 */
function MenuItem({ icon: Icon, label, onClick, disabled, shortcut }) {
  return (
    <Button
      variant="ghost"
      className="w-full justify-start gap-3 px-2 text-[0.7rem] font-semibold uppercase hover:bg-muted-foreground/10"
      onClick={onClick}
      disabled={disabled}
      aria-keyshortcuts={shortcut}
    >
      <Icon className="size-3.5" />
      {label}
    </Button>
  )
}

/**
 * Renders the project and media import menu in the app header.
 *
 * @param {object} props
 * @param {string|null} props.appVersion - Build-time app version display label.
 * @param {function} props.onImportActivity - Opens the activity file picker.
 * @param {function} props.onImportVideo - Opens the video import picker.
 * @param {function} props.onNewProject - Starts a blank project from the current template.
 * @param {function} props.onLoadProject - Opens the project file picker.
 * @param {function} props.onSaveProject - Saves the project to its current path.
 * @param {string} props.status - Project document status: 'Unsaved' | 'Saved' | 'Modified'.
 * @param {function} props.onSaveProjectAs - Saves the project to a new path.
 * @returns {JSX.Element} Rendered component.
 */
export default function ActivitySection({
  appVersion,
  onImportActivity,
  onImportVideo,
  onNewProject,
  onLoadProject,
  onSaveProject,
  onSaveProjectAs,
  status,
}) {
  const { t } = useTranslation()
  const [menuOpen, setMenuOpen] = useState(false)

  const run = (action) => {
    setMenuOpen(false)
    action()
  }

  return (
    <div className="flex min-w-0 items-center gap-6 overflow-hidden">
      <div className="flex shrink-0 items-center gap-2">
        <img src="/logo.svg" alt="OVRLEY" className="h-5 pr-3" />
        <div className="h-8 w-px shrink-0 bg-border/60" />
        {appVersion ? (
          <div className="text-[0.8rem] font-semibold tabular-nums text-muted-foreground/70 normal-case leading-none pt-1 pl-3">{appVersion}</div>
        ) : null}
      </div>

      <div className="h-8 w-px shrink-0 bg-border/60" />

      <Popover open={menuOpen} onOpenChange={setMenuOpen}>
        <PopoverTrigger asChild>
          <Button type="button" className="h-9 w-21 gap-3" aria-label={t('app-shell.fileMenu', 'File menu')}>
            <Menu className="h-4 w-4" />
            {t('app-shell.file', 'File')}
          </Button>
        </PopoverTrigger>
        <PopoverContent align="start" sideOffset={6} className="w-44 p-1">
          <div className="flex flex-col">
            <MenuItem icon={FilePlus2} label={t('app-shell.newProject', 'New Project')} shortcut="Mod+N" onClick={() => run(onNewProject)} />
            <MenuItem icon={FolderOpen} label={t('app-shell.loadProject', 'Load Project')} shortcut="Mod+O" onClick={() => run(onLoadProject)} />
            <MenuItem icon={Save} label={t('app-shell.saveProject', 'Save Project')} shortcut="Mod+S" disabled={status === 'Saved'} onClick={() => run(onSaveProject)} />
            <MenuItem icon={Save} label={t('app-shell.saveProjectAs', 'Save Project As')} shortcut="Mod+Shift+S" onClick={() => run(onSaveProjectAs)} />
          </div>

          <Separator className="my-1.5" />

          <div className="flex flex-col gap-0.5">
            <MenuItem icon={Activity} label={t('app-shell.importActivity', 'Import Activity')} onClick={() => run(onImportActivity)} />
          </div>

          <Separator className="my-1.5" />

          <div className="flex flex-col gap-0.5">
            <MenuItem icon={Film} label={t('app-shell.importVideo', 'Import Video')} onClick={() => run(onImportVideo)} />
          </div>
        </PopoverContent>
      </Popover>
    </div>
  )
}
