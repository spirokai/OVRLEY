import { describe, test, expect, vi, beforeAll } from 'vitest'
import { render, screen } from '@testing-library/react'
import MetricWidgetEditor from '@/features/widget-editor/components/MetricWidgetEditor'

beforeAll(() => {
  globalThis.ResizeObserver = class ResizeObserver {
    observe() {}
    unobserve() {}
    disconnect() {}
  }
})

function makeWidget(type, data = {}) {
  return {
    id: 'value-0',
    type,
    data,
  }
}

describe('MetricWidgetEditor decimal control', () => {
  test('shows decimal toggle for g_force', () => {
    render(<MetricWidgetEditor widget={makeWidget('g_force', { display_unit: 'g' })} updateWidgetData={vi.fn()} setNumericField={vi.fn()} />)
    expect(screen.getByText('Decimals')).toBeInTheDocument()
  })

  test('shows decimal toggle for stride_length', () => {
    render(<MetricWidgetEditor widget={makeWidget('stride_length', { display_unit: 'm' })} updateWidgetData={vi.fn()} setNumericField={vi.fn()} />)
    expect(screen.getByText('Decimals')).toBeInTheDocument()
  })

  test('shows decimal toggle for torque', () => {
    render(<MetricWidgetEditor widget={makeWidget('torque', { display_unit: 'nm' })} updateWidgetData={vi.fn()} setNumericField={vi.fn()} />)
    expect(screen.getByText('Decimals')).toBeInTheDocument()
  })

  test('shows decimal toggle for vertical_speed', () => {
    render(<MetricWidgetEditor widget={makeWidget('vertical_speed', { display_unit: 'mps' })} updateWidgetData={vi.fn()} setNumericField={vi.fn()} />)
    expect(screen.getByText('Decimals')).toBeInTheDocument()
  })

  test('hides decimal toggle for pace', () => {
    render(<MetricWidgetEditor widget={makeWidget('pace', { display_unit: 'min_per_km' })} updateWidgetData={vi.fn()} setNumericField={vi.fn()} />)
    expect(screen.queryByText('Decimals')).not.toBeInTheDocument()
  })

  test('hides decimal toggle for air_pressure', () => {
    render(<MetricWidgetEditor widget={makeWidget('air_pressure', { display_unit: 'hpa' })} updateWidgetData={vi.fn()} setNumericField={vi.fn()} />)
    expect(screen.queryByText('Decimals')).not.toBeInTheDocument()
  })

  test('hides decimal toggle for ground_contact_time', () => {
    render(
      <MetricWidgetEditor widget={makeWidget('ground_contact_time', { display_unit: 'ms' })} updateWidgetData={vi.fn()} setNumericField={vi.fn()} />,
    )
    expect(screen.queryByText('Decimals')).not.toBeInTheDocument()
  })

  test('hides decimal toggle for stroke_rate', () => {
    render(<MetricWidgetEditor widget={makeWidget('stroke_rate', { display_unit: 'spm' })} updateWidgetData={vi.fn()} setNumericField={vi.fn()} />)
    expect(screen.queryByText('Decimals')).not.toBeInTheDocument()
  })

  test('shows decimal toggle for vertical_oscillation', () => {
    render(
      <MetricWidgetEditor widget={makeWidget('vertical_oscillation', { display_unit: 'mm' })} updateWidgetData={vi.fn()} setNumericField={vi.fn()} />,
    )
    expect(screen.getByText('Decimals')).toBeInTheDocument()
  })

  test('hides decimal toggle for left_right_balance', () => {
    render(
      <MetricWidgetEditor
        widget={makeWidget('left_right_balance', { display_unit: 'percent' })}
        updateWidgetData={vi.fn()}
        setNumericField={vi.fn()}
      />,
    )
    expect(screen.queryByText('Decimals')).not.toBeInTheDocument()
  })
})

describe('MetricWidgetEditor balance format', () => {
  test('shows balance format label for left_right_balance', () => {
    render(
      <MetricWidgetEditor
        widget={makeWidget('left_right_balance', { display_unit: 'percent' })}
        updateWidgetData={vi.fn()}
        setNumericField={vi.fn()}
      />,
    )
    expect(screen.getByText('Balance Format')).toBeInTheDocument()
  })

  test('defaults to 52%/48% for left_right_balance', () => {
    render(
      <MetricWidgetEditor
        widget={makeWidget('left_right_balance', { display_unit: 'percent', balance_format: 'percent_label' })}
        updateWidgetData={vi.fn()}
        setNumericField={vi.fn()}
      />,
    )
    expect(screen.getByText('52%/48%')).toBeInTheDocument()
  })

  test('does not show balance format for non-balance widgets', () => {
    render(<MetricWidgetEditor widget={makeWidget('g_force', { display_unit: 'g' })} updateWidgetData={vi.fn()} setNumericField={vi.fn()} />)
    expect(screen.queryByText('Balance Format')).not.toBeInTheDocument()
  })
})
