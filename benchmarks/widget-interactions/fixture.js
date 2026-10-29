import { createWidgetDefaults } from '@/lib/widget/widget-defaults'
import { mergeWidgetData } from '@/lib/widget/widget-resolver'
import { WIDGET_COLLECTIONS, requireWidgetTypeDefinition } from '@/lib/widget/widget-catalog'
import { DEFAULT_GLOBAL_DEFAULTS } from '@/lib/template/template-constants'
import { buildConfigWidgets } from '@/lib/widget/widget-presentation'
import { createEditorEffectiveConfig } from '@/lib/template/template-state'
import { widgetFonts } from '@/features/wasm-widgets/utils/previewSession'

const mixed = [
  ['speed', 'text'],
  ['time', 'text'],
  ['label', undefined],
  ['speed', 'linear'],
  ['speed', 'arc'],
  ['backdrop', 'rectangle'],
  ['gradient', undefined],
  ['speed', 'text'],
  ['heading', 'heading_tape'],
  ['course', undefined],
  ['speed', 'corner'],
  ['label', undefined],
  ['backdrop', 'circle'],
  ['elevation', undefined],
  ['speed', 'text'],
]

/** @param {object} input Workload name, widget count, parsed activity, and HUD template. @returns {object} Canonical editor config and font requests. */
export function createFixture({ workload, count, activity, template }) {
  const globals = { ...DEFAULT_GLOBAL_DEFAULTS, ...template.settings.globalDefaults }
  const config = {
    scene: { ...template.config.scene },
    ...Object.fromEntries(Object.keys(WIDGET_COLLECTIONS).map((key) => [key, []])),
  }
  for (let index = 0; index < count; index++) {
    const [type, displayType] = workload === 'text' ? ['speed', 'text'] : mixed[index % mixed.length]
    const data = createWidgetDefaults({ type, displayType }, globals, { coursePoints: activity.sample_course_points, sceneFontSize: 19.5 })
    const overrides = {
      id: `profile-${index}`,
      x: 200 + (index % 5) * 600,
      y: 200 + Math.floor(index / 5) * 500,
      ...(type === 'label' ? { text: `OVRLEY ${index}`, font_size: 45 } : {}),
      ...(type === 'speed' && displayType === 'text' ? { font_size: 45 } : {}),
      ...(type === 'course' ? { width: 230, height: 140, show_full_activity: true } : {}),
      ...(type === 'elevation' ? { width: 250, height: 130, show_full_activity: true } : {}),
    }
    config[requireWidgetTypeDefinition(type).collection].push(mergeWidgetData(data, overrides))
  }
  const widgets = buildConfigWidgets(createEditorEffectiveConfig({ config, globalDefaults: globals }))
  const fonts = [
    ...new Map([{ font: globals.font_values }, ...widgets.flatMap(widgetFonts)].map((request) => [JSON.stringify(request), request])).values(),
  ]
  return { workload, config, globals, fonts }
}
