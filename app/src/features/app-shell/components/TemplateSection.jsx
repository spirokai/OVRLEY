/**
 * Middle column of the app header — template selector and template CRUD actions.
 * Pure presentational component.
 */

import { useMemo } from 'react'
import { Button } from '@/components/ui/button'
import { Select, SelectContent, SelectGroup, SelectItem, SelectLabel, SelectSeparator, SelectTrigger, SelectValue } from '@/components/ui/select'
import { SimpleTooltip } from '@/components/ui/simple-tooltip'
import { FilePlus2, FolderOpen, Save, Sparkles } from 'lucide-react'
import { getTemplateGroups } from '../utils/templateGroups'

/**
 * Renders the template management controls in the app header.
 *
 * @param {object} props
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
export default function TemplateSection({
  loadedTemplateSource,
  loadedTemplateFilename,
  handleTemplateChange,
  templates,
  config,
  showTemplateStatus,
  handleCreateNewTemplate,
  handleSaveTemplate,
  handleImportTemplate,
  className = '',
}) {
  const templateGroups = useMemo(() => getTemplateGroups(templates), [templates])

  return (
    <div className={`flex min-w-0 items-center justify-start gap-2 ${className}`.trim()}>
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
        {showTemplateStatus && config ? (
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
        ) : null}
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
  )
}
