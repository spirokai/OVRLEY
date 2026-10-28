import { describe, expect, test, vi } from 'vite-plus/test'
import { fireEvent, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import BackdropWidgetEditor from '@/features/widget-editor/components/BackdropWidgetEditor'

vi.mock('@/features/widget-editor/components/widgetFormControls', () => ({
  TIME_FORMATS: [],
  ColorField: ({ label, value, onChange }) => (
    <button type="button" aria-label={label} data-value={value} onClick={() => onChange('#123456')}>
      {label}
    </button>
  ),
  NumberField: ({ label, value, onChange }) => <input aria-label={label} value={value ?? ''} onChange={(event) => onChange(event.target.value)} />,
  SelectField: ({ label, value, onValueChange, options }) => (
    <label>
      {label}
      <select aria-label={label} value={value} onChange={(event) => onValueChange(event.target.value)}>
        {options.map((option) => (
          <option key={option.value} value={option.value}>
            {option.label}
          </option>
        ))}
      </select>
    </label>
  ),
  SliderField: ({ label, value, step = 1, onSliderChange }) => (
    <button type="button" aria-label={label} data-value={value} onClick={() => onSliderChange(value + step)}>
      {label}
    </button>
  ),
  TextField: () => null,
  ToggleField: ({ checked, onCheckedChange }) => (
    <button type="button" role="switch" aria-checked={checked} onClick={() => onCheckedChange(!checked)} />
  ),
}))

function makeBackdropWidget(overrides = {}) {
  return {
    id: 'backdrop-1',
    type: 'backdrop',
    category: 'backdrops',
    data: {
      id: 'backdrop-1',
      x: 10,
      y: 20,
      opacity: 0.75,
      display_type: 'rectangle',
      fill_color: '#ffffff',
      fill_opacity: 0.5,
      border_thickness: 2,
      border_color: '#000000',
      border_opacity: 0.25,
      display_variants: {
        rectangle: {
          width: 240,
          height: 120,
          corner_radius: 8,
          round_top_left: false,
          round_top_right: false,
          round_bottom_left: false,
          round_bottom_right: false,
          edge_strips: [],
          edge_strip_thickness: 6,
          edge_strip_color: '#FF5722',
        },
      },
      ...overrides,
    },
  }
}

describe('BackdropWidgetEditor', () => {
  test('updates shared rectangle styling fields without touching general opacity', async () => {
    const user = userEvent.setup()
    const updateWidgetData = vi.fn()
    const updateWidgetSize = vi.fn()
    const commitWidgetSize = vi.fn()

    render(
      <BackdropWidgetEditor
        widget={makeBackdropWidget()}
        updateWidgetData={updateWidgetData}
        updateWidgetSize={updateWidgetSize}
        commitWidgetSize={commitWidgetSize}
      />,
    )

    await user.click(screen.getByRole('button', { name: 'Fill Color' }))
    await user.click(screen.getByRole('button', { name: 'Fill Opacity' }))
    await user.click(screen.getByRole('button', { name: 'Border Color' }))
    await user.click(screen.getByRole('button', { name: 'Border Opacity' }))
    await user.click(screen.getByRole('button', { name: 'Border Thickness' }))

    expect(updateWidgetData).toHaveBeenCalledWith('backdrop-1', { fill_color: '#123456' })
    expect(updateWidgetData).toHaveBeenCalledWith('backdrop-1', { fill_opacity: 0.55 })
    expect(updateWidgetData).toHaveBeenCalledWith('backdrop-1', { border_color: '#123456' })
    expect(updateWidgetData).toHaveBeenCalledWith('backdrop-1', { border_opacity: 0.3 })
    expect(updateWidgetSize).toHaveBeenCalledWith('backdrop-1', { border_thickness: 3 })
    expect(updateWidgetData).not.toHaveBeenCalledWith('backdrop-1', expect.objectContaining({ opacity: expect.any(Number) }))
  })

  test('updates rectangle dimensions and corner radius inside the active variant', async () => {
    const user = userEvent.setup()
    const updateWidgetData = vi.fn()
    const updateWidgetSize = vi.fn()
    const commitWidgetSize = vi.fn()

    render(
      <BackdropWidgetEditor
        widget={makeBackdropWidget()}
        updateWidgetData={updateWidgetData}
        updateWidgetSize={updateWidgetSize}
        commitWidgetSize={commitWidgetSize}
      />,
    )

    fireEvent.change(screen.getByLabelText('Width'), { target: { value: '300' } })
    fireEvent.change(screen.getByLabelText('Height'), { target: { value: '160' } })
    await user.click(screen.getByRole('button', { name: 'Corner Radius' }))

    expect(updateWidgetData).toHaveBeenCalledWith(
      'backdrop-1',
      expect.objectContaining({
        display_variants: expect.objectContaining({
          rectangle: expect.objectContaining({ width: 300, corner_radius: 8 }),
        }),
      }),
    )
    expect(updateWidgetData).toHaveBeenCalledWith(
      'backdrop-1',
      expect.objectContaining({
        display_variants: expect.objectContaining({
          rectangle: expect.objectContaining({ height: 160, corner_radius: 8 }),
        }),
      }),
    )
    expect(updateWidgetSize).toHaveBeenCalledWith(
      'backdrop-1',
      expect.objectContaining({
        display_variants: expect.objectContaining({
          rectangle: expect.objectContaining({ corner_radius: 9 }),
        }),
      }),
    )
  })

  test('corner grid exposes and toggles spatial per-corner controls', async () => {
    const user = userEvent.setup()
    const updateWidgetData = vi.fn()

    render(<BackdropWidgetEditor widget={makeBackdropWidget()} updateWidgetData={updateWidgetData} />)

    expect(screen.getByRole('button', { name: 'Round top left corner' })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Round top right corner' })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Round bottom left corner' })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Round bottom right corner' })).toBeInTheDocument()
    expect(screen.getByTestId('corner-grid')).toHaveClass('h-32')
    expect(screen.getByTestId('corner-control-rows')).toHaveClass('h-32', 'grid-rows-2')

    await user.click(screen.getByRole('button', { name: 'Round bottom right corner' }))

    expect(updateWidgetData).toHaveBeenCalledWith(
      'backdrop-1',
      expect.objectContaining({
        display_variants: expect.objectContaining({
          rectangle: expect.objectContaining({ round_bottom_right: true }),
        }),
      }),
    )
  })

  test('display type changes seed the requested backdrop variant non-destructively', () => {
    const updateWidgetData = vi.fn()

    render(<BackdropWidgetEditor widget={makeBackdropWidget()} updateWidgetData={updateWidgetData} />)

    fireEvent.change(screen.getByLabelText('Display Type'), { target: { value: 'circle' } })

    expect(updateWidgetData).toHaveBeenCalledWith(
      'backdrop-1',
      expect.objectContaining({
        display_type: 'circle',
        display_variants: expect.objectContaining({
          rectangle: expect.objectContaining({ width: 240, height: 120 }),
          circle: { diameter: 200 },
        }),
      }),
    )
  })

  test('display type changes restore an existing rectangle variant without reinitializing it', () => {
    const updateWidgetData = vi.fn()
    const rectangleVariant = {
      width: 320,
      height: 180,
      corner_radius: 12,
      round_top_left: true,
      round_top_right: false,
      round_bottom_left: true,
      round_bottom_right: false,
      edge_strips: [],
      edge_strip_thickness: 6,
      edge_strip_color: '#FF5722',
    }

    render(
      <BackdropWidgetEditor
        widget={makeBackdropWidget({
          display_type: 'circle',
          display_variants: {
            rectangle: rectangleVariant,
            circle: { diameter: 260 },
          },
        })}
        updateWidgetData={updateWidgetData}
      />,
    )

    fireEvent.change(screen.getByLabelText('Display Type'), { target: { value: 'rectangle' } })

    expect(updateWidgetData).toHaveBeenCalledWith(
      'backdrop-1',
      expect.objectContaining({
        display_type: 'rectangle',
        display_variants: {
          rectangle: rectangleVariant,
          circle: { diameter: 260 },
        },
      }),
    )
  })

  test('updates circle diameter inside the active circle variant', () => {
    const updateWidgetData = vi.fn()

    render(
      <BackdropWidgetEditor
        widget={makeBackdropWidget({
          display_type: 'circle',
          display_variants: {
            rectangle: {
              width: 240,
              height: 120,
              corner_radius: 8,
              round_top_left: false,
              round_top_right: false,
              round_bottom_left: false,
              round_bottom_right: false,
              edge_strips: [],
              edge_strip_thickness: 6,
              edge_strip_color: '#FF5722',
            },
            circle: { diameter: 260 },
          },
        })}
        updateWidgetData={updateWidgetData}
      />,
    )

    fireEvent.change(screen.getByLabelText('Diameter'), { target: { value: '300' } })

    expect(updateWidgetData).toHaveBeenCalledWith(
      'backdrop-1',
      expect.objectContaining({
        display_variants: expect.objectContaining({
          circle: { diameter: 300 },
        }),
      }),
    )
  })
})
