/**
 * Left column of the app header — activity file, video import, template selector, and template CRUD.
 * Pure presentational component.
 */

import { useMemo } from 'react'
import { Button } from '@/components/ui/button'
import { Select, SelectContent, SelectGroup, SelectItem, SelectLabel, SelectSeparator, SelectTrigger, SelectValue } from '@/components/ui/select'
import { SimpleTooltip } from '@/components/ui/simple-tooltip'
import { Activity, FilePlus2, Film, FolderOpen, Save, Sparkles, X } from 'lucide-react'
import { getTemplateGroups } from '../utils/templateGroups'

/**
 * Renders the activity, video, and template management controls in the app header.
 *
 * @param {object} props
 * @param {string} props.activityLabel - Label for the activity file button.
 * @param {function} props.onOpenActivityFile - Opens the activity file picker.
 * @param {boolean} props.debugModeEnabled - Whether debug-only media features are enabled.
 * @param {string|null} props.importedMediaFilename - Filename of the imported background media, or null.
 * @param {function} props.handleImportVideo - Opens the video import picker.
 * @param {function} props.clearImportedVideo - Clears the imported video.
 * @param {string|null} props.loadedTemplateSource - Source of the loaded template ('backend' | 'file' | null).
 * @param {string|null} props.loadedTemplateFilename - Filename of the loaded template.
 * @param {function} props.handleTemplateChange - Handles template selection change.
 * @param {object[]} props.templates - Available templates array.
 * @param {object|null} props.config - Current editor config.
 * @param {boolean} props.showTemplateStatus - Whether to show the save template button.
 * @param {function} props.handleCreateNewTemplate - Creates a new blank template.
 * @param {function} props.handleSaveTemplate - Saves the current template.
 * @param {function} props.handleImportTemplate - Imports a template from file.
 * @returns {JSX.Element} Rendered component.
 */
export default function ActivitySection({
  activityLabel,
  onOpenActivityFile,
  debugModeEnabled,
  importedMediaFilename,
  handleImportVideo,
  clearImportedVideo,
  loadedTemplateSource,
  loadedTemplateFilename,
  handleTemplateChange,
  templates,
  config,
  showTemplateStatus,
  handleCreateNewTemplate,
  handleSaveTemplate,
  handleImportTemplate,
}) {
  const templateGroups = useMemo(() => getTemplateGroups(templates), [templates])

  return (
    <div className="flex min-w-0 items-center gap-6 overflow-hidden">
      <div className="flex shrink-0 items-center gap-3">
        <img src="/logo.svg" alt="OVRLEY" className="h-5" />
      </div>

      <div className="h-8 w-px shrink-0 bg-border/60" />

      <div className="min-w-0 flex-1 overflow-hidden">
        <div className="flex min-w-0 items-center gap-2">
          <Button className="mr-2 h-9 shrink-0 gap-2 border-border/70 px-5" onClick={onOpenActivityFile}>
            <Activity className="h-3.5 w-3.5" />
            <span className="max-w-28 truncate">{activityLabel}</span>
          </Button>

          {importedMediaFilename ? (
            <div className="max-w-[min(14rem,22vw)] mr-2 flex h-9 items-center rounded-md border border-border/70 bg-surface-elevated pl-3 pr-2 text-xs text-foreground justify-between">
              <div className="flex items-center gap-2 truncate">
                <Film className="mr-2 h-4 w-4 text-primary" />
                <span className="truncate">{importedMediaFilename}</span>
              </div>
              <Button
                variant="ghost"
                size="icon"
                className="ml-1 h-6 w-6 text-muted-foreground hover:bg-surface-accent-soft hover:text-foreground"
                onClick={clearImportedVideo}
              >
                <X className="h-3.5 w-3.5" />
              </Button>
            </div>
          ) : (
            <Button
              variant="outline"
              className="w-50 mr-2 h-9 shrink-0 gap-2 border-border/70 px-5 text-muted-foreground hover:text-foreground text-sm"
              onClick={handleImportVideo}
            >
              <Film className="h-3.5 w-3.5" />
              <span className="truncate">{debugModeEnabled ? 'Import Video / Image' : 'Import Video'}</span>
            </Button>
          )}

          <Select value={loadedTemplateSource === 'backend' ? loadedTemplateFilename || '' : ''} onValueChange={handleTemplateChange}>
            <SelectTrigger className="h-8 w-56 max-w-[min(14rem,22vw)] shrink bg-surface text-xs border-border/70">
              <div className="flex items-center gap-2 truncate">
                <Sparkles className="h-3 w-3 shrink-0 text-primary" />
                <SelectValue placeholder={loadedTemplateSource === 'file' ? loadedTemplateFilename || 'Imported Template' : 'Select Template...'} />
              </div>
            </SelectTrigger>
            <SelectContent>
              {templateGroups.map((group) => (
                <SelectGroup key={group.key}>
                  <SelectLabel className="flex items-center gap-2 text-[10px] font-bold uppercase tracking-widest">
                    <span>{group.label}</span>
                  </SelectLabel>
                  <SelectSeparator className="my-0" />
                  {group.templates.map((template) => (
                    <SelectItem key={template.id} value={template.id}>
                      {template.name} {template.type === 'user' && '(User)'}
                    </SelectItem>
                  ))}
                </SelectGroup>
              ))}
            </SelectContent>
          </Select>

          <div className="flex shrink-0 items-center gap-1">
            <SimpleTooltip side="bottom" content="New Template">
              <Button
                variant="ghost"
                size="icon"
                className="h-8 w-8 text-muted-foreground hover:bg-surface-elevated hover:text-foreground"
                onClick={handleCreateNewTemplate}
              >
                <FilePlus2 className="h-4 w-4" />
              </Button>
            </SimpleTooltip>
            {showTemplateStatus && config && (
              <SimpleTooltip side="bottom" content="Save Template">
                <Button
                  variant="ghost"
                  size="icon"
                  className="h-8 w-8 text-muted-foreground hover:bg-surface-accent-soft hover:text-primary"
                  onClick={handleSaveTemplate}
                >
                  <Save className="h-4 w-4" />
                </Button>
              </SimpleTooltip>
            )}
            <SimpleTooltip side="bottom" content="Import Template">
              <Button
                variant="ghost"
                size="icon"
                className="h-8 w-8 text-muted-foreground hover:bg-surface-elevated hover:text-foreground"
                onClick={handleImportTemplate}
              >
                <FolderOpen className="h-4 w-4" />
              </Button>
            </SimpleTooltip>
          </div>
        </div>
      </div>
    </div>
  )
}
