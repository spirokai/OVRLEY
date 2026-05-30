/**
 * Behavior tests for the widget identity seam.
 *
 * These specs document the durable-id contract the rest of the editor relies
 * on: widget ids live in widget data, legacy templates upgrade on contact, and
 * config-to-widget mapping no longer depends on array position.
 */

import { describe, expect, test } from 'vitest'
import { buildConfigWidgets, deleteWidgetInConfig, findWidgetInConfig, replaceWidgetInConfig, updateWidgetInConfig } from '@/lib/widget-config'

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
