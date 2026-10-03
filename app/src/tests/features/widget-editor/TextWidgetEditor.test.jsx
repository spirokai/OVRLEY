import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { beforeAll, describe, expect, test, vi } from 'vitest'
import TextWidgetEditor from '@/features/widget-editor/components/TextWidgetEditor'

const fonts = vi.hoisted(() => [
  {
    id: 'Inter.ttf',
    name: 'Inter',
    faces: [
      {
        style: 'normal',
        weight: 400,
        axes: [{ tag: 'wght', min: 100, default: 400, max: 900, hidden: false }],
        file: 'Inter-variable.ttf',
        local_name: null,
      },
      {
        style: 'italic',
        weight: 400,
        axes: [{ tag: 'wght', min: 100, default: 400, max: 900, hidden: false }],
        file: 'Inter-italic-variable.ttf',
        local_name: null,
      },
    ],
  },
  {
    id: 'Teko.ttf',
    name: 'Teko',
    faces: [
      {
        style: 'normal',
        weight: 400,
        axes: [{ tag: 'wght', min: 300, default: 400, max: 700, hidden: false }],
        file: 'Teko-variable.ttf',
        local_name: null,
      },
    ],
  },
  {
    id: 'Static',
    name: 'Static',
    faces: [
      { style: 'normal', weight: 400, axes: [], file: null, local_name: 'Static' },
      { style: 'normal', weight: 700, axes: [], file: null, local_name: 'Static-Bold' },
    ],
  },
])

vi.mock('@/lib/font-resources', () => ({
  getPreparedFont: (id) => fonts.find((font) => font.id === id),
  getFontCatalog: async () => ({ recommendedFonts: fonts, systemFonts: [] }),
}))

vi.mock('@/api/backend', () => ({
  listAvailableFonts: async () => ({ recommendedFonts: fonts, systemFonts: [] }),
  getFontCapabilities: async (id) => fonts.find((font) => font.id === id),
}))

const widget = (font, font_weight, italic = false) => ({
  id: 'title',
  type: 'label',
  category: 'labels',
  data: { font, font_weight, italic, letter_spacing: 0, font_size: 60, text: 'Title', color: '#ffffff' },
})

beforeAll(() => {
  HTMLElement.prototype.scrollIntoView = () => {}
  globalThis.ResizeObserver = class {
    observe() {}
    unobserve() {}
    disconnect() {}
  }
})

describe('label weight controls', () => {
  test('changes fractional spacing through the live updater and commit without changing weight or italic', async () => {
    const updateWidgetSize = vi.fn()
    const commitWidgetSize = vi.fn()
    const updateWidgetData = vi.fn()
    const label = widget('Inter.ttf', 537, true)
    label.data.letter_spacing = -1.25
    render(
      <TextWidgetEditor widget={label} updateWidgetData={updateWidgetData} updateWidgetSize={updateWidgetSize} commitWidgetSize={commitWidgetSize} />,
    )
    await waitFor(() => expect(screen.getAllByRole('slider')).toHaveLength(3))
    const spacing = screen.getByRole('slider', { name: 'Letter Spacing' })
    expect(spacing).toHaveAttribute('aria-valuenow', '-1.25')
    expect(screen.getByText('-1.3%')).toBeInTheDocument()
    fireEvent.keyDown(spacing, { key: 'ArrowRight' })
    expect(updateWidgetSize).toHaveBeenCalledWith('title', { letter_spacing: -0.5 })
    expect(commitWidgetSize).toHaveBeenCalledWith('title')
    expect(updateWidgetData).not.toHaveBeenCalled()
    await waitFor(() => expect(screen.getAllByRole('slider')).toHaveLength(3))
  })
  test('keeps weight and italic controls disabled for a static upright family', async () => {
    const updateWidgetData = vi.fn()
    const updateWidgetSize = vi.fn()
    const commitWidgetSize = vi.fn()
    render(
      <TextWidgetEditor
        widget={widget('Static', 537)}
        updateWidgetData={updateWidgetData}
        updateWidgetSize={updateWidgetSize}
        commitWidgetSize={commitWidgetSize}
      />,
    )
    await waitFor(() => expect(screen.getAllByRole('slider')).toHaveLength(3))
    expect(screen.getByRole('button', { name: 'Italic' })).toBeDisabled()
    const weight = screen.getAllByRole('slider')[1]
    expect(weight).toHaveAttribute('data-disabled')
    expect(weight).toHaveAttribute('aria-valuenow', '700')
    expect(weight).not.toHaveAttribute('tabindex')
    expect(updateWidgetSize).not.toHaveBeenCalled()
    expect(commitWidgetSize).not.toHaveBeenCalled()
    expect(updateWidgetData).not.toHaveBeenCalled()
  })

  test('delegates font changes to the committed widget action', async () => {
    const updateWidgetData = vi.fn()
    render(
      <TextWidgetEditor
        widget={widget('Inter.ttf', 900, true)}
        updateWidgetData={updateWidgetData}
        updateWidgetSize={vi.fn()}
        commitWidgetSize={vi.fn()}
      />,
    )
    await waitFor(() => expect(screen.getAllByRole('slider')).toHaveLength(3))
    expect(screen.getAllByRole('slider')[1]).toHaveAttribute('aria-valuemax', '900')
    fireEvent.keyDown(screen.getByRole('combobox'), { key: 'Enter' })
    fireEvent.click(screen.getByRole('option', { name: 'Teko' }))
    await waitFor(() => expect(updateWidgetData).toHaveBeenCalledWith('title', { font: 'Teko.ttf', font_family: 'Teko' }))
    expect(updateWidgetData).toHaveBeenCalledTimes(1)
  })

  test('toggles italic through the committed updater without changing weight', async () => {
    const updateWidgetData = vi.fn()
    const label = widget('Inter.ttf', 537)
    const props = { updateWidgetData, updateWidgetSize: vi.fn(), commitWidgetSize: vi.fn() }
    const { rerender } = render(<TextWidgetEditor widget={label} {...props} />)
    const toggle = screen.getByRole('button', { name: 'Italic' })
    await waitFor(() => expect(toggle).toBeEnabled())
    expect(toggle).toHaveAttribute('aria-pressed', 'false')
    fireEvent.click(toggle)
    expect(updateWidgetData).toHaveBeenLastCalledWith('title', { italic: true })
    rerender(<TextWidgetEditor widget={{ ...label, data: { ...label.data, italic: true } }} {...props} />)
    expect(toggle).toHaveAttribute('aria-pressed', 'true')
    fireEvent.click(toggle)
    expect(updateWidgetData).toHaveBeenLastCalledWith('title', { italic: false })
  })

  test('uses the detected range and existing live-update and commit callbacks', async () => {
    const updateWidgetSize = vi.fn()
    const commitWidgetSize = vi.fn()
    render(
      <TextWidgetEditor
        widget={widget('Teko.ttf', 537)}
        updateWidgetData={vi.fn()}
        updateWidgetSize={updateWidgetSize}
        commitWidgetSize={commitWidgetSize}
      />,
    )
    await waitFor(() => expect(screen.getAllByRole('slider')).toHaveLength(3))
    const weight = screen.getAllByRole('slider')[1]
    expect(weight).toHaveAttribute('aria-valuemin', '300')
    expect(weight).toHaveAttribute('aria-valuemax', '700')
    expect(screen.getByText('537')).toBeInTheDocument()
    fireEvent.keyDown(weight, { key: 'ArrowRight' })
    expect(updateWidgetSize).toHaveBeenCalledWith('title', { font_weight: 600 })
    expect(commitWidgetSize).toHaveBeenCalledWith('title')
  })
})
