/**
 * Template manager feature — public API.
 * Template lifecycle: create, save, import, switch, and dirty tracking.
 */

export { default as useTemplateManagement } from './hooks/useTemplateManagement'
export { DEFAULT_EXPORT_RANGE } from '@/lib/template/template-constants'
export { normalizeTemplateConfig } from './utils/templateSnapshot'
