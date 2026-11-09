import { createElement } from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import { describe, expect, test } from 'vite-plus/test'
import { DISPLAY_TYPE_DEFINITIONS, WIDGET_TYPE_DEFINITIONS, getWidgetDisplayTypeOptions, isFramedWidget } from '@/lib/widget/widget-catalog'
import { createWidgetDefaults } from '@/lib/widget/widget-defaults'
import { GROUPED_QUICKMENU_ITEMS, WIDGET_ICONS } from '@/lib/widget/widget-icons'
import { buildConfigWidgets, getWidgetTypeName } from '@/lib/widget/widget-presentation'
import { createDurableEditorState, createEditorEffectiveConfig } from '@/lib/template/template-state'
import { DEFAULT_GLOBAL_DEFAULTS } from '@/lib/template/template-constants'
import i18next from '@/i18n'

describe('manifest-backed widgets', () => {
  test('offers seven scuba text metrics with diving units and conventional names', () => {
    const group = GROUPED_QUICKMENU_ITEMS.find((group) => group.items.some((item) => item.key === 'depth'))
    const types = ['depth', 'first_stop_depth', 'first_stop_time', 'time_to_surface', 'average_ppo2', 'gas_mix', 'water_temperature']
    expect(group.items.map((item) => item.key).sort()).toEqual([...types, 'diving_profile'].sort())
    for (const type of types) {
      expect(WIDGET_TYPE_DEFINITIONS[type].category).toBe('scuba_diving')
      // Depth also offers the shared tape under its own name.
      const tape = type === 'depth' ? [{ value: 'altitude_tape', label: 'Depth Tape' }] : []
      expect(getWidgetDisplayTypeOptions(type, i18next.t)).toEqual([{ value: 'text', label: 'Text' }, ...tape])
    }
    expect(i18next.t('widgets.types.depth.shortName')).toBe('Depth')
    expect(i18next.t('widgets.types.first_stop_depth.shortName')).toBe('Stop Depth')
    expect(i18next.t('widgets.types.first_stop_time.shortName')).toBe('Stop Time')
    expect(WIDGET_TYPE_DEFINITIONS.depth.supportedDisplayUnits.map((unit) => unit.value)).toEqual(['m', 'ft'])
    expect(WIDGET_TYPE_DEFINITIONS.water_temperature.supportedDisplayUnits.map((unit) => unit.value)).toEqual(['celsius', 'fahrenheit'])
    expect(createWidgetDefaults({ type: 'average_ppo2' }, DEFAULT_GLOBAL_DEFAULTS)).toMatchObject({ display_unit: 'ata', decimals: 2 })
    expect(WIDGET_TYPE_DEFINITIONS.average_ppo2.supportedDisplayUnits[0]).toEqual({ value: 'ata', label: 'atm', renderLabel: 'ATM' })
    for (const [type, decimals] of [
      ['depth', 1],
      ['first_stop_depth', 1],
      ['first_stop_time', 0],
      ['time_to_surface', 0],
    ]) {
      expect(WIDGET_TYPE_DEFINITIONS[type].hasDecimalControl).toBe(true)
      expect(createWidgetDefaults({ type }, DEFAULT_GLOBAL_DEFAULTS).decimals).toBe(decimals)
    }
    expect(WIDGET_TYPE_DEFINITIONS.gas_mix.unitsMode).toBe('hidden')
  })

  test('uses the same display choices in the inspector and widget drawer', () => {
    expect(getWidgetDisplayTypeOptions('heading', i18next.t)).toEqual([
      { value: 'text', label: 'Text' },
      { value: 'heading_tape', label: 'Heading Tape' },
      { value: 'heading_compass', label: 'Heading Compass' },
    ])
    expect(getWidgetDisplayTypeOptions('heartrate', i18next.t).at(-1)).toEqual({ value: 'heart_zones', label: 'Heart Zones' })
    expect(getWidgetDisplayTypeOptions('altitude', i18next.t).at(-1)).toEqual({ value: 'altitude_tape', label: 'Altitude Tape' })
    expect(getWidgetDisplayTypeOptions('backdrop', i18next.t)).toEqual([
      { value: 'circle', label: 'Circle' },
      { value: 'rectangle', label: 'Rectangle' },
    ])
    const items = GROUPED_QUICKMENU_ITEMS.flatMap((group) => group.items)
    expect(items.find((item) => item.key === 'depth').options.at(-1).labelKey).toBe('widgets.displayTypes.depthTape')
    expect(items.find((item) => item.key === 'heading').options.map((option) => option.selection)).toEqual([
      { type: 'heading', displayType: 'text' },
      { type: 'heading', displayType: 'heading_tape' },
      { type: 'heading', displayType: 'heading_compass' },
    ])
    expect(items.find((item) => item.key === 'lap_timer').options.map((option) => option.selection)).toEqual([
      { type: 'lap_timer', lapTimerMode: 'current_lap' },
      { type: 'lap_timer', lapTimerMode: 'best_lap' },
      { type: 'lap_timer', lapTimerMode: 'delta' },
      { type: 'lap_timer', lapTimerMode: 'lap_log' },
    ])
  })

  test('offers route, map tiles and 3D route as one map entry', () => {
    const items = GROUPED_QUICKMENU_ITEMS.flatMap((group) => group.items)
    const map = items.find((item) => item.key === 'map')
    expect(items.some((item) => item.key === 'course')).toBe(false)
    expect(map.types).toEqual(['course', 'map'])
    expect(map.options.map((option) => i18next.t(option.labelKey))).toEqual(['Route', 'Map Tiles', '3D Route'])
    expect(map.options.map((option) => option.selection)).toEqual([
      { type: 'course' },
      { type: 'map', preset: { camera_mode: 'fixed' } },
      { type: 'map' },
    ])
    const [, tiles, route3d] = map.options.map((option) => createWidgetDefaults(option.selection, DEFAULT_GLOBAL_DEFAULTS))
    expect(tiles.camera_mode).toBe('fixed')
    expect(route3d).toMatchObject({ camera_mode: 'follow', pitch: 50 })
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
