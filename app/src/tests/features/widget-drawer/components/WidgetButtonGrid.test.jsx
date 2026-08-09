/**
 * Tests for WidgetButtonGrid — verifies the widget button grid renders and handles clicks.
 */

import { describe, test, expect, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { WidgetButtonGrid } from '@/features/widget-drawer/components/WidgetButtonGrid'

describe('WidgetButtonGrid', () => {
  test('renders a button for each widget type including Wave 1 standard metrics', () => {
    render(<WidgetButtonGrid onAddWidget={() => {}} />)

    const buttons = screen.getAllByRole('button')
    expect(buttons.length).toBeGreaterThan(0)
    expect(screen.getByText('Time')).toBeInTheDocument()
    expect(screen.getByText('Grad.')).toBeInTheDocument()
  })

  test('each button displays an icon', () => {
    const { container } = render(<WidgetButtonGrid onAddWidget={() => {}} />)

    const buttons = container.querySelectorAll('button')
    buttons.forEach((button) => {
      expect(button.querySelector('svg')).toBeInTheDocument()
    })
  })

  test('each button displays the widget drawer label for the current drawer catalog', () => {
    render(<WidgetButtonGrid onAddWidget={() => {}} />)

    expect(screen.getByText('HR')).toBeInTheDocument()
    expect(screen.getByText('Map')).toBeInTheDocument()
    expect(screen.getByText('Time')).toBeInTheDocument()
    expect(screen.getByText('Grad.')).toBeInTheDocument()
    expect(screen.getByText('Backdrop')).toBeInTheDocument()
    expect(screen.getByText('Temp.')).toBeInTheDocument()
    expect(screen.getByText('G-Force')).toBeInTheDocument()
    expect(screen.getByText('Air Press.')).toBeInTheDocument()
    expect(screen.getByText('GCT')).toBeInTheDocument()
    expect(screen.getByText('L/R Bal.')).toBeInTheDocument()
    expect(screen.getByText('Stride')).toBeInTheDocument()
    expect(screen.getByText('S/R')).toBeInTheDocument()
    expect(screen.getByText('V. Speed')).toBeInTheDocument()
    expect(screen.getByText('Gear')).toBeInTheDocument()
    expect(screen.getByText('V. Osc.')).toBeInTheDocument()
    expect(screen.getByText('Core T.')).toBeInTheDocument()
  })

  test('clicking a metric display type calls onAddWidget with the correct type and display label', async () => {
    const onAddWidget = vi.fn()
    const user = userEvent.setup()
    render(<WidgetButtonGrid onAddWidget={onAddWidget} />)

    await user.click(screen.getByText('Speed').closest('button'))
    const textOptions = screen.getAllByRole('button', { name: 'Text' })
    const textDisplayOption = textOptions[textOptions.length - 1]
    expect(textDisplayOption).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Linear Bar' })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Arc Gauge' })).toBeInTheDocument()
    await user.click(textDisplayOption)

    expect(onAddWidget).toHaveBeenCalledWith({ type: 'speed', displayType: 'text' })
  })

  test('lap timer presents all four canonical readouts', async () => {
    const onAddWidget = vi.fn()
    const user = userEvent.setup()
    render(<WidgetButtonGrid onAddWidget={onAddWidget} />)

    await user.click(screen.getByText('Lap Timer').closest('button'))
    expect(screen.getByRole('button', { name: 'Current Lap' })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Best Lap' })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Delta' })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Lap Times' })).toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: 'Lap Times' }))

    expect(onAddWidget).toHaveBeenCalledWith({ type: 'lap_timer', lapTimerMode: 'lap_log' })
  })

  test('clicking a backdrop display type uses backdrop labels and calls onAddWidget', async () => {
    const onAddWidget = vi.fn()
    const user = userEvent.setup()
    render(<WidgetButtonGrid onAddWidget={onAddWidget} />)

    await user.click(screen.getByText('Backdrop').closest('button'))
    expect(screen.getByRole('button', { name: 'Rectangle' })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Circle' })).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'rectangle' })).not.toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: 'Rectangle' }))

    expect(onAddWidget).toHaveBeenCalledWith({ type: 'backdrop', displayType: 'rectangle' })
  })

  test('clicking a button does not auto-close the drawer', async () => {
    const onAddWidget = vi.fn()
    const user = userEvent.setup()
    render(<WidgetButtonGrid onAddWidget={onAddWidget} />)

    await user.click(screen.getByText('HR').closest('button'))
    const textOptions = screen.getAllByRole('button', { name: 'Text' })
    await user.click(textOptions[textOptions.length - 1])

    // onAddWidget is called but nothing else — drawer state is managed externally
    expect(onAddWidget).toHaveBeenCalledTimes(1)
  })
})
