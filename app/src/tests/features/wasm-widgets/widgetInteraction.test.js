import { afterEach, beforeEach, expect, test, vi } from 'vitest'
import { createWidgetInteraction } from '@/features/wasm-widgets/utils/widgetInteraction'
import { resizeAnchor } from '@/features/wasm-widgets/utils/widgetGestureGeometry'

beforeEach(() => vi.useFakeTimers({ toFake: ['requestAnimationFrame', 'cancelAnimationFrame'] }))
afterEach(() => vi.useRealTimers())

function editor(type = 'speed') {
  const original = { id: 'widget', type, data: { x: 120.25, y: 60.5, font_size: 48 } }
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
    getGeometry: () => ({ content: { left: 100, top: 65, width: 160, height: 40 } }),
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

test('drag previews coalesce from the captured origin; release flushes the last unpainted movement without rounding', () => {
  const { interaction, presentations, original } = editor()
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
})

test('resize uses original configuration and a fixed content corner across live drafts/playback; cancellation restores the document', () => {
  const { interaction, presentations, resize, original, setSnapshot } = editor()
  const draft = vi.fn()
  const origin = interaction.begin(draft)
  const anchor = resizeAnchor(origin.bounds, [-1, -1])
  interaction.update({ type: 'resize', geometry: { factor: 1.25, ...anchor } })
  interaction.update({ type: 'resize', geometry: { factor: 1.5, ...anchor } })
  vi.advanceTimersToNextFrame()
  expect(resize).toHaveBeenCalledTimes(1)
  expect(resize.mock.calls[0][0]).toEqual({ type: 'speed', widget: original.data, scene: { scale: 1.5 }, second: 0, factor: 1.5, fixed: [1, 1], point: [260, 105] })
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

test('label resize ignores playhead changes but flushes the final pointer intent', () => {
  const { interaction, resize, original, setSnapshot, render } = editor('label')
  const draft = vi.fn()
  const origin = interaction.begin(draft)
  const anchor = resizeAnchor(origin.bounds, [1, 1])
  interaction.update({ type: 'resize', geometry: { factor: 1.25, ...anchor } })
  vi.advanceTimersToNextFrame()
  for (const second of [10, 2, 30]) {
    setSnapshot({ widgets: [original], scene: { scale: 1.5 }, second })
    render()
  }
  expect(resize).toHaveBeenCalledTimes(1)
  expect(draft).toHaveBeenCalledTimes(1)
  interaction.update({ type: 'resize', geometry: { factor: 1.375, ...anchor } })
  expect(interaction.finish().font_size).toBe(66)
  expect(resize).toHaveBeenCalledTimes(2)
  expect(resize.mock.lastCall[0]).toMatchObject({ type: 'label', widget: original.data, second: 30 })
  expect(vi.getTimerCount()).toBe(0)
})
