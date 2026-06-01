/**
 * Behavior tests for the widget identity seam.
 *
 * These specs document the durable-id contract the rest of the editor relies
 * on: widget ids live in widget data, legacy templates upgrade on contact, and
 * config-to-widget mapping no longer depends on array position.
 */

import { describe, expect, test } from 'vitest'
import { buildConfigWidgets, groupWidgetsForSidebar } from '@/lib/widget-presentation'
import {
  deleteWidgetInConfig,
  deleteWidgetsInConfig,
  duplicateWidgetsInConfig,
  ensureWidgetIdsInConfig,
  findWidgetInConfig,
  replaceWidgetInConfig,
  updateWidgetInConfig,
  updateWidgetsInConfig,
} from '@/lib/widget-config'

function makeConfig({ labels = [], values = [], plots = [] } = {}) {
  return {
    scene: { width: 1920, height: 1080, fps: 30 },
    labels,
    values,
    plots,
  }
}

describe('widget-config stable identity', () => {
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
        index: 0,
        name: 'Alpha',
      }),
      expect.objectContaining({
        id: 'widget-2',
        category: 'labels',
        index: 1,
        name: 'Beta',
      }),
    ])
  })

  test('widget CRUD upgrades legacy templates to durable ids before applying updates', () => {
    const legacyConfig = makeConfig({
      labels: [
        { text: 'Legacy A', x: 0, y: 0 },
        { text: 'Legacy B', x: 10, y: 10 },
      ],
    })

    const [firstWidget, secondWidget] = buildConfigWidgets(legacyConfig)
    const updatedConfig = updateWidgetInConfig(legacyConfig, secondWidget.id, { text: 'Updated B' })
    const nextWidgets = buildConfigWidgets(updatedConfig)
    const survivingConfig = deleteWidgetInConfig(updatedConfig, firstWidget.id)

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
    expect(findWidgetInConfig(updatedConfig, secondWidget.id)).toMatchObject({
      category: 'labels',
      id: secondWidget.id,
      index: 1,
      data: expect.objectContaining({
        id: secondWidget.id,
        text: 'Updated B',
      }),
    })
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
      nonexistent: { text: 'nope' },
    })

    expect(result.labels[0]).toMatchObject({ id: 'widget-1', x: 5, y: 5, text: 'A' })
    expect(result.labels[1]).toMatchObject({ id: 'widget-2', text: 'B' })
    expect(result.labels[2]).toMatchObject({ id: 'widget-3', text: 'C-updated' })
  })

  test('updateWidgetsInConfig handles null / empty inputs', () => {
    expect(updateWidgetsInConfig(null, {})).toBeNull()
    expect(updateWidgetsInConfig(makeConfig(), null)).toEqual(makeConfig())
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

  test('deleteWidgetsInConfig handles null / empty inputs', () => {
    expect(deleteWidgetsInConfig(null, [])).toBeNull()
    expect(deleteWidgetsInConfig(makeConfig(), [])).toEqual(makeConfig())
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
    const typeLabels = { label: 'Text', speed: 'Speed', heart_rate: 'Heart Rate', cadence: 'Cadence' }
    const widgets = [
      { id: 'w1', type: 'label', name: 'My Label', category: 'labels' },
      { id: 'w2', type: 'heart_rate', name: 'HR', category: 'values' },
      { id: 'w3', type: 'speed', name: 'Speed', category: 'values' },
      { id: 'w4', type: 'cadence', name: 'Cad', category: 'values' },
    ]

    const result = groupWidgetsForSidebar(widgets, typeLabels)

    const types = [...new Set(result.map((w) => w.type))]
    const groupLabels = result.map((w) => w.groupLabel).filter(Boolean)

    expect(types).toEqual(['cadence', 'heart_rate', 'speed', 'label'])
    expect(groupLabels.length).toBe(4)
  })

  test('only first widget per group gets a groupLabel', () => {
    const typeLabels = { label: 'Text' }
    const widgets = [
      { id: 'w1', type: 'label', name: 'A' },
      { id: 'w2', type: 'label', name: 'B' },
    ]

    const result = groupWidgetsForSidebar(widgets, typeLabels)

    expect(result[0].groupLabel).toBe('Text')
    expect(result[1].groupLabel).toBeNull()
  })

  test('falls back to widget type when typeLabel is missing', () => {
    const widgets = [{ id: 'w1', type: 'unknown_type', name: 'X' }]

    const result = groupWidgetsForSidebar(widgets, {})

    expect(result[0].groupLabel).toBe('unknown_type')
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

  test('replaces legacy index-derived ids', () => {
    const config = makeConfig({
      labels: [{ id: 'label-0', text: 'Legacy' }],
    })

    const result = ensureWidgetIdsInConfig(config)

    expect(result.labels[0].id).toMatch(/^widget-\d+$/)
    expect(result.labels[0].id).not.toBe('label-0')
  })

  test('preserves existing durable ids', () => {
    const config = makeConfig({
      labels: [{ id: 'widget-42', text: 'Durable' }],
    })

    const result = ensureWidgetIdsInConfig(config)

    expect(result.labels[0].id).toBe('widget-42')
  })

  test('resolves id collisions by assigning a new generated id', () => {
    const config = makeConfig({
      labels: [
        { id: 'widget-1', text: 'First' },
        { id: 'widget-1', text: 'Collision' },
      ],
    })

    const result = ensureWidgetIdsInConfig(config)

    const ids = result.labels.map((l) => l.id)

    expect(new Set(ids).size).toBe(2)
    expect(ids[0]).toBe('widget-1')
    expect(ids[1]).toMatch(/^widget-\d+$/)
    expect(ids[1]).not.toBe('widget-1')
  })

  test('handles null config', () => {
    expect(ensureWidgetIdsInConfig(null)).toBeNull()
  })
})
