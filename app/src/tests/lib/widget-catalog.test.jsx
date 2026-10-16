import { createElement } from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import { describe, expect, test } from 'vitest'
import { DISPLAY_TYPE_DEFINITIONS, WIDGET_TYPE_DEFINITIONS, getWidgetDisplayTypeOptions, isFramedWidget } from '@/lib/widget/widget-catalog'
import { createWidgetDefaults } from '@/lib/widget/widget-defaults'
import { GROUPED_QUICKMENU_ITEMS, WIDGET_ICONS } from '@/lib/widget/widget-icons'
import { buildConfigWidgets, getWidgetTypeName } from '@/lib/widget/widget-presentation'
import { createDurableEditorState, createEditorEffectiveConfig } from '@/lib/template/template-state'
import { DEFAULT_GLOBAL_DEFAULTS } from '@/lib/template/template-constants'
import i18next from '@/i18n'

describe('manifest-backed widgets', () => {
  test('uses the same display choices in the inspector and widget drawer', () => {
    expect(getWidgetDisplayTypeOptions('heading', i18next.t)).toEqual([
      { value: 'text', label: 'Text' },
      { value: 'heading_tape', label: 'Heading Tape' },
    ])
    expect(getWidgetDisplayTypeOptions('backdrop', i18next.t)).toEqual([
      { value: 'circle', label: 'Circle' },
      { value: 'rectangle', label: 'Rectangle' },
    ])
    const items = GROUPED_QUICKMENU_ITEMS.flatMap((group) => group.items)
    expect(items.find((item) => item.type === 'heading').options.map((option) => option.selection)).toEqual([
      { displayType: 'text' },
      { displayType: 'heading_tape' },
    ])
    expect(items.find((item) => item.type === 'lap_timer').options.map((option) => option.selection)).toEqual([
      { lapTimerMode: 'current_lap' },
      { lapTimerMode: 'best_lap' },
      { lapTimerMode: 'delta' },
      { lapTimerMode: 'lap_log' },
    ])
  })

  test('renders every registered icon through the production component', () => {
    for (const type of Object.keys(WIDGET_TYPE_DEFINITIONS)) {
      expect(renderToStaticMarkup(createElement(WIDGET_ICONS[type]))).toContain('<svg')
    }
    expect(renderToStaticMarkup(createElement(WIDGET_ICONS.brake_position))).toContain('fill="currentColor"')
  })

  test('preserves every widget, display, and display mode through creation, save/load, and editor materialization', () => {
    for (const [type, definition] of Object.entries(WIDGET_TYPE_DEFINITIONS)) {
      const displayTypes = definition.displayTypes.length > 0 ? definition.displayTypes : [definition.defaults.display_type]
      for (const displayType of displayTypes) {
        const display = displayType === undefined ? null : DISPLAY_TYPE_DEFINITIONS[displayType]
        const modes = display !== null && display.modes.length > 0 ? display.modes : [null]
        for (const mode of modes) {
          const selection = { type }
          if (display !== null) selection.displayType = displayType
          if (mode !== null) selection[display.modeSelectionField] = mode.value
          const data = createWidgetDefaults(selection, DEFAULT_GLOBAL_DEFAULTS)
          const config = { scene: {}, rasters: [], [definition.collection]: [data] }
          const durable = createDurableEditorState({ config, globalDefaults: DEFAULT_GLOBAL_DEFAULTS })
          const reloaded = createDurableEditorState(durable)
          expect(reloaded).toEqual(durable)
          const [widget] = buildConfigWidgets(createEditorEffectiveConfig(reloaded))
          expect(widget.type).toBe(type)
          if (display !== null) expect(widget.data.display_type).toBe(displayType)
          if (definition.metric) expect(widget.data.display_unit).toBe(definition.defaults.display_unit)
          expect(isFramedWidget(widget)).toBe((display === null ? definition.layoutMode : display.layoutMode) === 'boxed')
        }
      }
    }
  })

  test('creates backdrop geometry from its display definition and keeps it in the active variant', () => {
    for (const displayType of WIDGET_TYPE_DEFINITIONS.backdrop.displayTypes) {
      const definition = DISPLAY_TYPE_DEFINITIONS[displayType]
      const data = createWidgetDefaults({ type: 'backdrop', displayType: displayType })
      expect(data.display_variants[displayType]).toEqual(definition.variantDefaults)
      for (const key of definition.variantKeys) expect(data).not.toHaveProperty(key)
    }
    expect(createWidgetDefaults({ type: 'backdrop' }).display_type).toBe('rectangle')
  })

  test('rejects unknown widget identities and unsupported lap timer modes', () => {
    expect(() => getWidgetTypeName('missing', i18next.t)).toThrow('Unknown widget type: missing')
    expect(() => createWidgetDefaults({ type: 'lap_timer', lapTimerMode: 'missing' }, DEFAULT_GLOBAL_DEFAULTS)).toThrow(
      'Unsupported lap_timer mode: missing',
    )
  })
})
