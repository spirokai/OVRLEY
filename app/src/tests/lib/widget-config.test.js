/**
 * Behavior tests for the widget identity seam.
 *
 * These specs document the durable-id contract the rest of the editor relies
 * on: widget ids live in widget data, legacy templates upgrade on contact, and
 * config-to-widget mapping no longer depends on array position.
 */

import { describe, expect, test } from 'vite-plus/test'
import { buildConfigWidgets, groupWidgetsForSidebar } from '@/lib/widget/widget-presentation'
import {
  deleteWidgetsInConfig,
  duplicateWidgetsInConfig,
  ensureWidgetIdsInConfig,
  replaceWidgetInConfig,
  updateWidgetInConfig,
  updateWidgetsInConfig,
} from '@/lib/widget/widget-config'

function makeConfig({ backdrops = [], labels = [], values = [], plots = [] } = {}) {
  return {
    scene: { width: 1920, height: 1080, fps: 30 },
    backdrops,
    rasters: [],
    labels,
    values,
    plots,
  }
}

describe('widget-config stable identity', () => {
  test('buildConfigWidgets emits backdrops before labels, values, and plots', () => {
    const config = makeConfig({
      backdrops: [{ id: 'widget-backdrop', display_type: 'rectangle', x: 0, y: 0 }],
      labels: [{ id: 'widget-label', text: 'Label', x: 10, y: 10 }],
      values: [{ id: 'widget-value', value: 'speed', x: 20, y: 20 }],
      plots: [{ id: 'widget-plot', value: 'course', x: 30, y: 30 }],
    })

    expect(buildConfigWidgets(config).map((widget) => widget.category)).toEqual(['backdrops', 'labels', 'values', 'plots'])
  })

  test('buildConfigWidgets preserves persisted widget ids instead of deriving ids from array position', () => {
    const config = makeConfig({
      labels: [
        { id: 'widget-7', text: 'Alpha', x: 0, y: 0 },
        { id: 'widget-2', text: 'Beta', x: 10, y: 10 },
      ],
    })

    expect(buildConfigWidgets(config)).toEqual([
      expect.objectContaining({
        id: 'widget-7',
        category: 'labels',
        name: 'Alpha',
      }),
      expect.objectContaining({
        id: 'widget-2',
        category: 'labels',
        name: 'Beta',
      }),
    ])
  })

  test('widget CRUD preserves identities assigned at document ingress', () => {
    const legacyConfig = makeConfig({
      labels: [
        { text: 'Legacy A', x: 0, y: 0 },
        { text: 'Legacy B', x: 10, y: 10 },
      ],
    })

    const config = ensureWidgetIdsInConfig(legacyConfig)
    const [firstWidget, secondWidget] = buildConfigWidgets(config)
    const updatedConfig = updateWidgetInConfig(config, secondWidget.id, { text: 'Updated B' })
    const nextWidgets = buildConfigWidgets(updatedConfig)
    const survivingConfig = deleteWidgetsInConfig(updatedConfig, [firstWidget.id])

    expect(firstWidget.id).toMatch(/^widget-\d+$/)
    expect(secondWidget.id).toMatch(/^widget-\d+$/)
    expect(firstWidget.id).not.toBe(secondWidget.id)
    expect(updatedConfig.labels[1]).toMatchObject({
      id: secondWidget.id,
      text: 'Updated B',
    })
    expect(nextWidgets[0].id).toBe(firstWidget.id)
    expect(nextWidgets[1].id).toBe(secondWidget.id)
    expect(survivingConfig.labels).toEqual([
      expect.objectContaining({
        id: secondWidget.id,
        text: 'Updated B',
      }),
    ])
  })

  test('replacing a widget keeps the same durable id while resetting its data', () => {
    const config = makeConfig({
      labels: [{ id: 'widget-11', text: 'Original', x: 0, y: 0, color: '#ffffff' }],
    })

    const resetConfig = replaceWidgetInConfig(config, 'widget-11', {
      text: 'Reset',
      x: 100,
      y: 200,
      color: '#000000',
    })

    expect(resetConfig.labels).toEqual([
      expect.objectContaining({
        id: 'widget-11',
        text: 'Reset',
        x: 100,
        y: 200,
        color: '#000000',
      }),
    ])
  })
})

/* -------------------------------------------------------------------------- */
/* Batch CRUD                                                                 */
/* -------------------------------------------------------------------------- */

describe('widget-config batch operations', () => {
  test('updateWidgetsInConfig applies multiple updates atomically', () => {
    const config = makeConfig({
      labels: [
        { id: 'widget-1', text: 'A', x: 0, y: 0 },
        { id: 'widget-2', text: 'B', x: 10, y: 10 },
        { id: 'widget-3', text: 'C', x: 20, y: 20 },
      ],
    })

    const result = updateWidgetsInConfig(config, {
      'widget-1': { x: 5, y: 5 },
      'widget-3': { text: 'C-updated' },
    })

    expect(result.labels[0]).toMatchObject({ id: 'widget-1', x: 5, y: 5, text: 'A' })
    expect(result.labels[1]).toMatchObject({ id: 'widget-2', text: 'B' })
    expect(result.labels[2]).toMatchObject({ id: 'widget-3', text: 'C-updated' })
  })

  test('batch updates preserve the document when empty and reject unknown target identities', () => {
    const config = makeConfig()
    expect(updateWidgetsInConfig(config, {})).toBe(config)
    expect(() => updateWidgetsInConfig(config, { nonexistent: { x: 10 } })).toThrow('Unknown widget ID: nonexistent')
  })

  test.each([
    [
      'backdrops',
      'rectangle',
      { corner_radius: 12, round_top_left: true, round_top_right: false, round_bottom_left: true, round_bottom_right: false },
    ],
    ['values', 'linear', { orientation: 'horizontal', track_thickness: 16, track_empty_opacity: 0.5 }],
    ['values', 'linear', { orientation: 'vertical', track_thickness: 16, track_empty_opacity: 0.5 }],
  ])('consecutive batch resizes preserve required %s/%s settings and inactive variants', (category, displayType, settings) => {
    const variant = { width: 200, height: 80, ...settings }
    const original = {
      id: 'widget-resize',
      x: 20,
      y: 30,
      display_type: displayType,
      display_variants: { [displayType]: variant, text: { icon_size: 24 } },
    }
    const config = makeConfig({ [category]: [original], labels: [{ id: 'widget-label', text: 'Untouched' }] })
    const firstPatch = { x: 15.25, display_variants: { [displayType]: { width: 250.5, height: 100.25 } } }
    const first = updateWidgetsInConfig(config, { 'widget-resize': firstPatch })
    const secondPatch = { x: 10.125, display_variants: { [displayType]: { width: 300.75, height: 120.5 } } }
    const second = updateWidgetsInConfig(first, { 'widget-resize': secondPatch })

    expect(first).toEqual(updateWidgetInConfig(config, 'widget-resize', firstPatch))
    expect(second[category][0].display_variants).toEqual({
      [displayType]: { ...variant, width: 300.75, height: 120.5 },
      text: { icon_size: 24 },
    })
    expect(second.labels).toBe(config.labels)
    expect(config[category][0]).toBe(original)
    expect(variant).toEqual({ width: 200, height: 80, ...settings })
  })

  test('deleteWidgetsInConfig removes multiple widgets at once', () => {
    const config = makeConfig({
      labels: [
        { id: 'widget-1', text: 'A' },
        { id: 'widget-2', text: 'B' },
        { id: 'widget-3', text: 'C' },
      ],
    })

    const result = deleteWidgetsInConfig(config, ['widget-1', 'widget-3'])

    expect(result.labels).toEqual([expect.objectContaining({ id: 'widget-2', text: 'B' })])
  })

  test('deleteWidgetsInConfig preserves an empty selection', () => {
    const config = makeConfig()
    expect(deleteWidgetsInConfig(config, [])).toBe(config)
  })

  test('duplicateWidgetsInConfig appends a duplicated widget with a new id and offset', () => {
    const config = makeConfig({
      labels: [
        { id: 'widget-1', text: 'A', x: 10, y: 20 },
        { id: 'widget-2', text: 'B', x: 30, y: 40 },
      ],
    })

    const result = duplicateWidgetsInConfig(config, [{ category: 'labels', data: config.labels[0] }])

    expect(result.config.labels).toHaveLength(3)
    expect(result.config.labels[2]).toMatchObject({
      text: 'A',
      x: 34,
      y: 44,
    })
    expect(result.config.labels[2].id).not.toBe('widget-1')
    expect(result.insertedWidgetIds).toEqual([result.config.labels[2].id])
  })
})

/* -------------------------------------------------------------------------- */
/* Sidebar grouping                                                           */
/* -------------------------------------------------------------------------- */

describe('groupWidgetsForSidebar', () => {
  test('groups widgets by type and sorts alphabetically', () => {
    const typeLabels = { label: 'Text', speed: 'Speed', heartrate: 'Heart Rate', cadence: 'Cadence' }
    const widgets = [
      { id: 'w1', type: 'label', name: 'My Label', category: 'labels' },
      { id: 'w2', type: 'heartrate', name: 'HR', category: 'values' },
      { id: 'w3', type: 'speed', name: 'Speed', category: 'values' },
      { id: 'w4', type: 'cadence', name: 'Cad', category: 'values' },
    ]

    const result = groupWidgetsForSidebar(widgets, (type) => typeLabels[type])

    const types = [...new Set(result.map((w) => w.type))]

    expect(types).toEqual(['cadence', 'heartrate', 'speed', 'label'])
  })

  test('preserves label text within a group', () => {
    const typeLabels = { label: 'Text' }
    const widgets = [
      { id: 'w1', type: 'label', name: 'A' },
      { id: 'w2', type: 'label', name: 'B' },
    ]

    const result = groupWidgetsForSidebar(widgets, (type) => typeLabels[type])

    expect(result.map((widget) => widget.name)).toEqual(['A', 'B'])
  })

  test('propagates a missing widget type contract failure', () => {
    const widgets = [{ id: 'w1', type: 'unknown_type', name: 'X' }]

    expect(() =>
      groupWidgetsForSidebar(widgets, (type) => {
        throw new Error(`Unknown widget type: ${type}`)
      }),
    ).toThrow('Unknown widget type: unknown_type')
  })
})

/* -------------------------------------------------------------------------- */
/* Legacy ID upgrade                                                          */
/* -------------------------------------------------------------------------- */

describe('ensureWidgetIdsInConfig', () => {
  test('assigns durable ids to widgets without ids', () => {
    const config = makeConfig({
      labels: [{ text: 'No ID' }],
    })

    const result = ensureWidgetIdsInConfig(config)

    expect(result.labels[0].id).toMatch(/^widget-\d+$/)
  })

  test('preserves existing identities regardless of their naming', () => {
    const config = makeConfig({
      labels: [{ id: 'label-0', text: 'Legacy' }],
    })

    const result = ensureWidgetIdsInConfig(config)

    expect(result).toBe(config)
  })

  test('preserves existing durable ids', () => {
    const config = makeConfig({
      labels: [{ id: 'widget-42', text: 'Durable' }],
    })

    const result = ensureWidgetIdsInConfig(config)

    expect(result.labels[0].id).toBe('widget-42')
  })

  test('rejects colliding identities at ingress', () => {
    const config = makeConfig({
      labels: [
        { id: 'widget-1', text: 'First' },
        { id: 'widget-1', text: 'Collision' },
      ],
    })

    expect(() => ensureWidgetIdsInConfig(config)).toThrow('Duplicate widget ID: widget-1')
  })

  test.each([null, 42, '', '   '])('rejects malformed present IDs: %s', (id) => {
    expect(() => ensureWidgetIdsInConfig(makeConfig({ labels: [{ id }] }))).toThrow('Invalid widget ID')
  })

  test('defaults absent collections and rejects malformed present collections', () => {
    expect(ensureWidgetIdsInConfig({ scene: {} })).toMatchObject({ backdrops: [], rasters: [], labels: [], values: [], plots: [] })
    expect(() => ensureWidgetIdsInConfig({ labels: null })).toThrow('Invalid widget collection: labels')
  })

  test('handles null config', () => {
    expect(ensureWidgetIdsInConfig(null)).toBeNull()
  })
})
