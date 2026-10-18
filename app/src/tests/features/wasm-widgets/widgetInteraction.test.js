import { afterEach, beforeEach, expect, test, vi } from 'vite-plus/test'
import { createWidgetInteraction } from '@/features/wasm-widgets/utils/widgetInteraction'
import { resizeAnchor } from '@/features/wasm-widgets/utils/widgetGestureGeometry'
import { resolveActiveWidgetData, applyWidgetDrafts } from '@/lib/widget/widget-resolver'
import { createWidgetDefaults } from '@/lib/widget/widget-defaults'
import { WIDGET_TYPE_DEFINITIONS } from '@/lib/widget/widget-catalog'
import { updateWidgetInConfig } from '@/lib/widget/widget-config'
import { DEFAULT_CONFIG } from '@/store/store-utils'

beforeEach(() => vi.useFakeTimers({ toFake: ['requestAnimationFrame', 'cancelAnimationFrame'] }))
afterEach(() => vi.useRealTimers())

function editor(type = 'speed') {
  const definition = WIDGET_TYPE_DEFINITIONS[type]
  const data = { ...createWidgetDefaults({ type, ...definition.resetSelection }), x: 120.25, y: 60.5, font_size: 48, rotation: 0 }
  const original = {
    id: 'widget',
    type,
    category: definition.collection,
    data: resolveActiveWidgetData(data),
  }
  let snapshot = { widgets: [original], scene: { scale: 1.5 }, second: 0 }
  let frame = null
  const presentations = []
  const resize = vi.fn(({ widget, factor }) => ({ x: 87.125, y: 30.25, font_size: widget.font_size * factor }))
  const render = () => {
    frame = null
    presentations.push(controller.resolve(snapshot))
  }
  const controller = createWidgetInteraction({
    schedule() {
      if (frame === null) frame = requestAnimationFrame(render)
    },
    flush() {
      if (frame !== null) cancelAnimationFrame(frame)
      render()
    },
    getSnapshot: () => snapshot,
    getGeometry: () => ({ content: { left: 100, top: 65, width: 160, height: 40 }, rotation_origin: [80, 20] }),
    resize,
  })
  return {
    interaction: controller.forWidget('widget'),
    render,
    presentations,
    resize,
    original,
    setSnapshot(next) {
      snapshot = next
    },
  }
}

test.each(['speed', 'time', 'gradient', 'lean_angle', 'backdrop', 'raster', 'course', 'elevation', 'lap_timer', 'heading'])(
  '%s drag coalesces from the captured origin and flushes the final movement without rounding',
  (type) => {
    const { interaction, presentations, original } = editor(type)
    const draft = vi.fn()
    interaction.begin(draft)
    interaction.update({ type: 'drag', translate: [10, 20] })
    interaction.update({ type: 'drag', translate: [30, 40] })
    expect(draft).not.toHaveBeenCalled()
    vi.advanceTimersToNextFrame()
    expect(draft).toHaveBeenCalledTimes(1)
    expect(presentations.at(-1)[0].data).toEqual({ ...original.data, x: 150.25, y: 100.5 })
    interaction.update({ type: 'drag', translate: [50.125, 60.25] })
    const patch = interaction.finish()
    expect(patch).toEqual({ x: 170.375, y: 120.75 })
    expect(presentations.at(-1)[0].data).toEqual({ ...original.data, ...patch })
    expect(vi.getTimerCount()).toBe(0)
    expect(original.data.x).toBe(120.25)
  },
)

test('group resize shares one frame, preserves each origin, and flushes all final patches together', () => {
  const { interaction, original, resize, setSnapshot } = editor()
  const second = { ...original, id: 'other', type: 'course', data: { ...original.data, x: 300, font_size: 24 } }
  const scene = { scale: 1.5 }
  setSnapshot({ widgets: [original, second], scene, second: 0 })
  const group = interaction.group(['widget', 'other'])
  const draft = vi.fn()
  group.begin(draft)
  group.update({ widget: { type: 'resize', geometry: { factor: 1.25 } }, other: { type: 'resize', geometry: { factor: 1.5 } } })
  expect(resize).not.toHaveBeenCalled()
  vi.advanceTimersToNextFrame()
  expect(draft).toHaveBeenCalledExactlyOnceWith({
    widget: { x: 87.125, y: 30.25, font_size: 60 },
    other: { x: 87.125, y: 30.25, font_size: 36 },
  })
  setSnapshot({ widgets: applyWidgetDrafts([original, second], draft.mock.lastCall[0]), scene, second: 2.25 })
  group.update({ widget: { type: 'resize', geometry: { factor: 2 } }, other: { type: 'resize', geometry: { factor: 3 } } })
  expect(group.finish()).toEqual({
    widget: { x: 87.125, y: 30.25, font_size: 96 },
    other: { x: 87.125, y: 30.25, font_size: 72 },
  })
  expect(resize.mock.calls[2][0]).toMatchObject({ widget: original.data, second: 2.25 })
  expect(resize.mock.calls[3][0]).toMatchObject({ widget: second.data, second: 2.25, commit: true })
  expect(vi.getTimerCount()).toBe(0)
  group.begin(draft)
  group.update({ widget: { type: 'resize', geometry: { factor: 4 } }, other: { type: 'resize', geometry: { factor: 4 } } })
  group.cancel()
  vi.advanceTimersToNextFrame()
  expect(resize).toHaveBeenCalledTimes(4)
  expect(group.finish()).toBeNull()
})

test('course slider and handle rotation share the same patch and preserve position when switching controls', () => {
  const { interaction, original, setSnapshot } = editor('course')
  const slider = interaction.rotate(-45.25)
  interaction.begin(vi.fn())
  interaction.update({ type: 'rotate', rotation: -45.25, translate: [0, 0] })
  expect(interaction.finish()).toEqual(slider)
  expect(slider.rotation).toBe(315)
  setSnapshot({ widgets: [{ ...original, data: { ...original.data, ...slider } }], scene: { scale: 1.5 }, second: 0 })
  const restored = interaction.rotate(0)
  expect(restored.x).toBeCloseTo(original.data.x, 10)
  expect(restored.y).toBeCloseTo(original.data.y, 10)
})

test.each(['course', 'elevation', 'diving_profile'])(
  '%s resize defers geometry preparation to the final flush and preserves fractional committed dimensions',
  (type) => {
    const { interaction, resize, setSnapshot, presentations } = editor(type)
    const original = {
      id: 'widget',
      type,
      category: 'plots',
      data: { id: 'widget', value: type, x: 120.25, y: 60.5, width: 400.25, height: 210.5 },
    }
    setSnapshot({ widgets: [original], scene: { scale: 1.5 }, second: 0.75 })
    resize.mockImplementation(({ widget, factor, size, commit }) => ({
      x: commit ? 81.125 : 80.25,
      y: 30.5,
      width: size ? size[0] : widget.width * factor,
      height: size ? size[1] : widget.height * factor,
    }))
    const origin = interaction.begin(vi.fn())
    const anchor = resizeAnchor(origin.bounds, [-1, -1])
    interaction.update({ type: 'resize', geometry: { ...(type !== 'course' ? { size: [300.1875, 157.875] } : { factor: 0.75 }), ...anchor } })
    vi.advanceTimersToNextFrame()
    expect(resize.mock.lastCall[0]).toMatchObject({ widget: original.data, second: 0.75, commit: false })
    interaction.update({ type: 'resize', geometry: { ...(type !== 'course' ? { size: [550.34375, 289.4375] } : { factor: 1.375 }), ...anchor } })
    const patch = interaction.finish() // Mouseup before RAF still resolves the final size as a commit.
    expect(resize).toHaveBeenCalledTimes(2)
    expect(resize.mock.lastCall[0]).toMatchObject({ widget: original.data, second: 0.75, commit: true })
    expect(patch).toEqual({ x: 81.125, y: 30.5, width: 550.34375, height: 289.4375 })
    expect(presentations.at(-1)[0].data).toEqual({ ...original.data, ...patch })
    expect(updateWidgetInConfig({ ...DEFAULT_CONFIG, plots: [original.data] }, 'widget', patch).plots[0]).toMatchObject(patch)
    expect(vi.getTimerCount()).toBe(0)
  },
)

test('resize uses original configuration and a fixed content corner across live drafts/playback; cancellation restores the document', () => {
  const { interaction, presentations, resize, original, setSnapshot } = editor()
  const draft = vi.fn()
  const origin = interaction.begin(draft)
  const anchor = resizeAnchor(origin.bounds, [-1, -1])
  interaction.update({ type: 'resize', geometry: { factor: 1.25, ...anchor } })
  interaction.update({ type: 'resize', geometry: { factor: 1.5, ...anchor } })
  vi.advanceTimersToNextFrame()
  expect(resize).toHaveBeenCalledTimes(1)
  expect(resize.mock.calls[0][0]).toEqual({
    type: 'speed',
    widget: original.data,
    scene: { scale: 1.5 },
    second: 0,
    factor: 1.5,
    fixed: [1, 1],
    point: [260, 105],
  })
  const patch = draft.mock.calls[0][0]
  setSnapshot({ widgets: [{ ...original, data: { ...original.data, ...patch } }], scene: { scale: 1.5 }, second: 1 })
  interaction.update({ type: 'resize', geometry: { factor: 2, ...anchor } })
  vi.advanceTimersToNextFrame()
  expect(resize.mock.calls[1][0].widget).toEqual(original.data)
  expect(resize.mock.calls[1][0].second).toBe(1)
  expect(presentations.at(-1)[0].data.font_size).toBe(96)
  interaction.update({ type: 'resize', geometry: { factor: 3, ...anchor } })
  interaction.cancel()
  setSnapshot({ widgets: [original], scene: { scale: 1.5 }, second: 1 })
  vi.advanceTimersToNextFrame()
  expect(resize).toHaveBeenCalledTimes(2)
  expect(presentations.at(-1)).toEqual([original])
  expect(interaction.finish()).toBeNull()
})

test.each(['label', 'time', 'gradient', 'lean_angle', 'lap_timer', 'heading'])('%s resize follows the playhead and flushes final intent', (type) => {
  const { interaction, resize, original, setSnapshot, render } = editor(type)
  const draft = vi.fn()
  const origin = interaction.begin(draft)
  const anchor = resizeAnchor(origin.bounds, [1, 1])
  interaction.update({ type: 'resize', geometry: { factor: 1.25, ...anchor } })
  vi.advanceTimersToNextFrame()
  for (const second of [10, 2, 30]) {
    setSnapshot({ widgets: [original], scene: { scale: 1.5 }, second })
    render()
  }
  expect(resize).toHaveBeenCalledTimes(type === 'label' ? 1 : 4)
  expect(draft).toHaveBeenCalledTimes(1) // Identical resolved patches need no new inspector draft.
  interaction.update({ type: 'resize', geometry: { factor: 1.375, ...anchor } })
  expect(interaction.finish().font_size).toBe(66)
  expect(resize).toHaveBeenCalledTimes(type === 'label' ? 2 : 5)
  expect(resize.mock.lastCall[0]).toMatchObject({ type, widget: original.data, second: 30 })
  expect(vi.getTimerCount()).toBe(0)
})

test.each(['rectangle', 'circle'])(
  'backdrop %s resize resolves variant drafts and commits the same fractional geometry without playhead work',
  (displayType) => {
    const { interaction, resize, setSnapshot, presentations, render } = editor('backdrop')
    const stored = {
      id: 'widget',
      x: 120.25,
      y: 60.5,
      display_type: displayType,
      display_variants: { rectangle: { width: 100, height: 60, corner_radius: 10 }, circle: { diameter: 100 } },
    }
    const original = { id: 'widget', type: 'backdrop', category: 'backdrops', data: resolveActiveWidgetData(stored) }
    setSnapshot({ widgets: [original], scene: { scale: 1.5 }, second: 0 })
    const patch = {
      x: 110.125,
      y: 50.25,
      display_variants: { [displayType]: displayType === 'circle' ? { diameter: 125.25 } : { width: 125.25, height: 80.5 } },
    }
    resize.mockReturnValue(patch)
    const draft = vi.fn()
    const origin = interaction.begin(draft)
    interaction.update({ type: 'resize', geometry: { size: [187.875, 120.75], ...resizeAnchor(origin.bounds, [-1, -1]) } })
    vi.advanceTimersToNextFrame()
    const live = presentations.at(-1)[0].data
    expect(live.width).toBe(125.25)
    expect(live.height).toBe(displayType === 'circle' ? 125.25 : 80.5)
    expect(live.display_variants.rectangle.corner_radius).toBe(10)
    const drafts = { widget: patch }
    expect(applyWidgetDrafts([original], drafts)[0].data).toEqual(live)
    for (const second of [1, 10, 2]) {
      setSnapshot({ widgets: [original], scene: { scale: 1.5 }, second })
      render()
    }
    expect(resize).toHaveBeenCalledTimes(1)
    expect(draft).toHaveBeenCalledTimes(1)
    const committed = updateWidgetInConfig({ backdrops: [stored] }, 'widget', interaction.finish())
    expect(resolveActiveWidgetData(committed.backdrops[0])).toEqual(live)
    expect(committed.backdrops[0].width).toBeUndefined()
  },
)

test('heading frame drafts preserve inactive variants and match the committed fractional dimensions', () => {
  const { interaction, resize, setSnapshot, presentations } = editor('heading')
  const stored = {
    id: 'widget',
    value: 'heading',
    x: 120.25,
    y: 60.5,
    display_type: 'heading_tape',
    display_variants: { heading_tape: { width: 400, height: 80, pixels_per_degree: 9 }, text: { icon_size: 24 } },
  }
  const original = { id: 'widget', type: 'heading', category: 'values', data: resolveActiveWidgetData(stored) }
  setSnapshot({ widgets: [original], scene: { scale: 1.5 }, second: 0 })
  const patch = { x: 110.125, y: 50.25, display_variants: { heading_tape: { width: 495.125, height: 70.375 } } }
  resize.mockReturnValue(patch)
  const origin = interaction.begin(vi.fn())
  interaction.update({ type: 'resize', geometry: { size: [742.6875, 105.5625], ...resizeAnchor(origin.bounds, [-1, -1]) } })
  const committed = updateWidgetInConfig({ ...DEFAULT_CONFIG, values: [stored] }, 'widget', interaction.finish())
  const live = presentations.at(-1)[0].data
  expect(resolveActiveWidgetData(committed.values[0])).toEqual(live)
  expect(live).toMatchObject({ x: 110.125, y: 50.25, width: 495.125, height: 70.375, pixels_per_degree: 9 })
  expect(committed.values[0].display_variants.text).toEqual(stored.display_variants.text)
  expect(committed.values[0]).not.toHaveProperty('width')

  setSnapshot({ widgets: [{ ...original, data: live }], scene: { scale: 1.5 }, second: 2.25 })
  const offsetPatch = { display_variants: { heading_tape: { height: 76.625, label_offset: 9.125 } } }
  resize.mockReturnValue(offsetPatch)
  expect(interaction.resize({ label_offset: 9.125 })).toEqual(offsetPatch)
  expect(resize.mock.lastCall[0]).toEqual({
    type: 'heading',
    widget: live,
    scene: { scale: 1.5 },
    second: 2.25,
    fixed: [0, 0],
    point: [live.x, live.y],
    label_offset: 9.125,
  })
})

test.each(['lean_angle', 'g_force'])('%s resize resolves nested intrinsic drafts and commits the same effective config', (type) => {
  const { interaction, resize, setSnapshot, presentations } = editor(type)
  const stored = {
    id: 'widget',
    value: type,
    x: 120.25,
    y: 60.5,
    font_size: 60,
    display_type: type,
    display_variants: {
      text: { icon_size: 24 },
      [type]: {
        diameter: 180,
        ...(type === 'g_force'
          ? { width: 220, height: 220, border_thickness: 2, marker_size: 12, label_font_size: 14, label_offset_x: 5, fill_color: '#222222' }
          : { track_thickness: 24, track_border_thickness: 2, value_offset_x: 5, value_offset_y: -3, track_empty_color: '#222222' }),
      },
    },
  }
  const original = { id: 'widget', type, category: 'values', data: resolveActiveWidgetData(stored) }
  setSnapshot({ widgets: [original], scene: { scale: 1.5 }, second: 0 })
  const patch = {
    x: 110.125,
    y: 50.25,
    ...(type === 'lean_angle' ? { font_size: 74.0625 } : {}),
    display_variants: {
      [type]: {
        diameter: 222.1875,
        ...(type === 'g_force'
          ? {
              width: 271.5625,
              height: 271.5625,
              border_thickness: 2.46875,
              marker_size: 14.8125,
              label_font_size: 17.28125,
              label_offset_x: 6.171875,
            }
          : { track_thickness: 29.625, track_border_thickness: 2.46875, value_offset_x: 6.171875, value_offset_y: -3.703125 }),
      },
    },
  }
  resize.mockReturnValue(patch)
  const draft = vi.fn()
  const origin = interaction.begin(draft)
  interaction.update({ type: 'resize', geometry: { factor: 1.234375, ...resizeAnchor(origin.bounds, [-1, -1]) } })
  const committed = updateWidgetInConfig({ ...DEFAULT_CONFIG, values: [stored] }, 'widget', interaction.finish())
  const live = presentations.at(-1)[0].data
  expect(resolveActiveWidgetData(committed.values[0])).toEqual(live)
  expect(live).toMatchObject({ x: 110.125, y: 50.25, diameter: 222.1875, font_size: type === 'lean_angle' ? 74.0625 : 60 })
  expect(committed.values[0].display_variants.text).toEqual(stored.display_variants.text)
  expect(committed.values[0].display_variants[type][type === 'g_force' ? 'fill_color' : 'track_empty_color']).toBe('#222222')
  expect(committed.values[0]).not.toHaveProperty('diameter')
  if (type === 'g_force') expect(live).toMatchObject({ width: 271.5625, height: 271.5625, label_font_size: 17.28125 })
  else {
    expect(live).not.toHaveProperty('width')
    expect(live).not.toHaveProperty('height')
  }
})
