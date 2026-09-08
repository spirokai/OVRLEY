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
import { useTranslation } from 'react-i18next'

/**
 * Renders the template management controls in the app header.
 *
 * @param {object} props
 * @param {object|null} props.loadedTemplateSource - Canonical template source descriptor.
 * @param {function} props.handleTemplateChange - Handles template selection change.
 * @param {object[]} props.templates - Available templates array.
 * @param {object|null} props.config - Current editor config.
 * @param {boolean} props.showTemplateStatus - Whether to show the save template button.
 * @param {function} props.handleCreateNewTemplate - Creates a new blank template.
 * @param {function} props.handleSaveTemplate - Saves the current template.
 * @param {function} props.handleImportTemplate - Imports a template from file.
 * @param {boolean} props.open - Whether the template selector is open.
 * @param {function} props.onOpenChange - Updates template selector visibility.
 * @returns {JSX.Element} Rendered component.
 */
export default function TemplateSection({
  loadedTemplateSource,
  handleTemplateChange,
  templates,
  config,
  showTemplateStatus,
  handleCreateNewTemplate,
  handleSaveTemplate,
  handleImportTemplate,
  open,
  onOpenChange,
  className = '',
}) {
  const { t } = useTranslation()
  const templateGroups = useMemo(() => getTemplateGroups(templates), [templates])

  return (
    <div className={`flex min-w-0 items-center justify-start gap-2 ${className}`.trim()}>
      <Select
        open={open}
        onOpenChange={onOpenChange}
        value={loadedTemplateSource?.kind === 'bundled' ? loadedTemplateSource.templateId : ''}
        onValueChange={(value) => {
          handleTemplateChange(value)
          onOpenChange(false)
        }}
      >
        <SelectTrigger className="h-8 w-56 max-w-[min(14rem,22vw)] shrink bg-surface text-xs border-border/70" aria-keyshortcuts="Mod+T">
          <div className="flex items-center gap-2 truncate">
            <Sparkles className="h-3 w-3 shrink-0 text-primary" />
            <SelectValue
              placeholder={
                loadedTemplateSource?.kind === 'file'
                  ? loadedTemplateSource.path.split(/[/\\]/).at(-1) || 'Imported Template'
                  : t('app-shell.selectTemplate', 'Select Template...')
              }
            />
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
        <SimpleTooltip side="bottom" content={t('app-shell.newTemplate', 'New Template')}>
          <Button
            variant="ghost"
            size="icon"
            className="h-8 w-8 text-muted-foreground hover:bg-surface-elevated hover:text-foreground"
            onClick={handleCreateNewTemplate}
            aria-keyshortcuts="Mod+Shift+T"
          >
            <FilePlus2 className="h-4 w-4" />
          </Button>
        </SimpleTooltip>
        <SimpleTooltip side="bottom" content={t('app-shell.importTemplate', 'Import Template')}>
          <Button
            variant="ghost"
            size="icon"
            className="h-8 w-8 text-muted-foreground hover:bg-surface-elevated hover:text-foreground"
            onClick={handleImportTemplate}
            aria-keyshortcuts="Mod+Shift+L"
          >
            <FolderOpen className="h-4 w-4" />
          </Button>
        </SimpleTooltip>
        {config && (
          <SimpleTooltip side="bottom" content={t('app-shell.saveTemplate', 'Save Template')}>
            <Button
              variant="ghost"
              size="icon"
              disabled={!showTemplateStatus}
              className="h-8 w-8 text-muted-foreground hover:bg-surface-accent-soft hover:text-primary"
              onClick={handleSaveTemplate}
              aria-keyshortcuts="Mod+Shift+P"
            >
              <Save className="h-4 w-4" />
            </Button>
          </SimpleTooltip>
        )}
      </div>
    </div>
  )
}
