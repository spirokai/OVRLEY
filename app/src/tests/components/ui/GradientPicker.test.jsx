/**
 * Behavior tests for the reusable linear gradient picker.
 */

import { fireEvent, render, screen } from '@testing-library/react'
import { beforeAll, describe, expect, test, vi } from 'vite-plus/test'
import GradientPicker from '@/components/ui/gradient-picker'

const stops = [
  { offset: 0, color: '#000000' },
  { offset: 1, color: '#ffffff' },
]

/**
 * Renders the picker with a fixed 200px-wide track.
 *
 * @param {{ offset: number, color: string }[]} value - Gradient stops.
 * @returns {{ onChange: Function, track: HTMLElement }} Change spy and track element.
 */
function renderPicker(value) {
  const onChange = vi.fn()
  render(<GradientPicker stops={value} onChange={onChange} />)
  const track = screen.getByRole('group', { name: 'Gradient stops' })
  track.getBoundingClientRect = () => ({ left: 0, width: 200, top: 0, height: 28, right: 200, bottom: 28 })
  return { onChange, track }
}

beforeAll(() => {
  globalThis.ResizeObserver = class ResizeObserver {
    observe() {}
    unobserve() {}
    disconnect() {}
  }
})

describe('GradientPicker', () => {
  test('clicking the track adds an interpolated stop', () => {
    const { onChange, track } = renderPicker(stops)

    fireEvent.pointerDown(track, { button: 0, clientX: 50 })

    expect(onChange).toHaveBeenCalledWith([...stops, { offset: 0.25, color: '#404040' }])
  })

  test('does not add more than five stops', () => {
    const full = [0, 0.25, 0.5, 0.75, 1].map((offset) => ({ offset, color: '#000000' }))
    const { onChange, track } = renderPicker(full)

    fireEvent.pointerDown(track, { button: 0, clientX: 50 })

    expect(onChange).not.toHaveBeenCalled()
  })

  test('dragging a stop moves it horizontally without adding a stop', () => {
    const { onChange } = renderPicker(stops)
    const node = screen.getByRole('button', { name: 'Color stop at 0%' })
    node.setPointerCapture = vi.fn()
    node.releasePointerCapture = vi.fn()

    fireEvent.pointerDown(node, { button: 0, clientX: 0, pointerId: 1 })
    fireEvent.pointerMove(node, { clientX: 120, pointerId: 1 })
    fireEvent.pointerUp(node, { clientX: 120, pointerId: 1 })

    expect(onChange).toHaveBeenCalledTimes(1)
    expect(onChange).toHaveBeenCalledWith([{ offset: 0.6, color: '#000000' }, stops[1]])
  })

  test('Delete removes a stop only while more than two remain', () => {
    const three = [...stops, { offset: 0.5, color: '#808080' }]
    const { onChange } = renderPicker(three)

    fireEvent.keyDown(screen.getByRole('button', { name: 'Color stop at 50%' }), { key: 'Delete' })

    expect(onChange).toHaveBeenCalledWith(stops)
  })

  test('keeps at least two stops', () => {
    const { onChange } = renderPicker(stops)

    fireEvent.keyDown(screen.getByRole('button', { name: 'Color stop at 0%' }), { key: 'Delete' })

    expect(onChange).not.toHaveBeenCalled()
  })

  test('clicking a stop opens the color picker', () => {
    renderPicker(stops)

    fireEvent.click(screen.getByRole('button', { name: 'Color stop at 0%' }))

    expect(screen.getByRole('button', { name: 'Remove stop' })).toBeDisabled()
  })
})
