/**
 * API contract test for UnitsControlRow.
 *
 * Tests the settled explicit checked + onCheckedChange contract.
 * The old widget + updateWidgetData convenience path has been removed.
 */

import { render, fireEvent } from '@testing-library/react'
import { describe, expect, test, vi } from 'vitest'
import { UnitsControlRow } from '@/features/widget-editor/components/widgetEditorSections'

vi.mock('@/features/widget-editor/components/widgetFormControls', () => ({
  ToggleField: ({ checked, onCheckedChange, label }) => (
    <button data-testid="toggle-field" data-checked={checked} data-label={label} onClick={() => onCheckedChange(!checked)} />
  ),
  ColorField: ({ value, label }) => <div data-testid="color-field" data-value={value} data-label={label} />,
  SelectField: ({ value, options, label }) => (
    <div data-testid="select-field" data-value={value} data-label={label} data-options-count={options?.length} />
  ),
  SectionHeading: ({ title }) => <div data-testid="section-heading">{title}</div>,
  NumberField: () => null,
  SliderField: () => null,
  TextField: () => null,
}))

describe('UnitsControlRow settled API', () => {
  test('renders toggle with explicit checked state', () => {
    const onCheckedChange = vi.fn()

    const { getByTestId } = render(<UnitsControlRow checked={false} onCheckedChange={onCheckedChange} title="Unit" />)

    const toggle = getByTestId('toggle-field')
    expect(toggle.dataset.checked).toBe('false')

    fireEvent.click(toggle)
    expect(onCheckedChange).toHaveBeenCalledWith(true)
  })

  test('renders toggle with checked=true', () => {
    const onCheckedChange = vi.fn()

    const { getByTestId } = render(<UnitsControlRow checked={true} onCheckedChange={onCheckedChange} title="Explicit Unit" />)

    expect(getByTestId('toggle-field').dataset.checked).toBe('true')

    fireEvent.click(getByTestId('toggle-field'))
    expect(onCheckedChange).toHaveBeenCalledWith(false)
  })

  test('renders color and select fields when both are provided', () => {
    const { getByTestId } = render(
      <UnitsControlRow
        checked={true}
        onCheckedChange={vi.fn()}
        title="Units"
        colorValue="#ff0000"
        onColorChange={vi.fn()}
        value="mps"
        onValueChange={vi.fn()}
        options={[{ value: 'kph', label: 'km/h' }]}
      />,
    )

    expect(getByTestId('color-field')).toBeTruthy()
    expect(getByTestId('select-field')).toBeTruthy()
  })

  test('renders without toggle when showToggle is false', () => {
    const { queryByTestId } = render(<UnitsControlRow showToggle={false} title="No Toggle" checked={true} onCheckedChange={vi.fn()} />)

    expect(queryByTestId('toggle-field')).toBeNull()
  })

  test('renders section heading with correct title', () => {
    const { getByText } = render(<UnitsControlRow checked={true} onCheckedChange={vi.fn()} title="Custom Unit" />)

    expect(getByText('Custom Unit')).toBeTruthy()
  })
})
