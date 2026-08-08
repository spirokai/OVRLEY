import { beforeAll, describe, expect, test, vi } from 'vitest'
import { render, screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import GForceWidgetEditor from '@/features/widget-editor/components/GForceWidgetEditor'
import { createMetricValueDefaults } from '@/features/widget-editor/utils/widgetUtils'

vi.mock('@/features/scene-settings/hooks/useAvailableFonts', () => ({
  default: () => ({ recommendedFonts: [], systemFonts: [] }),
}))

beforeAll(() => {
  globalThis.ResizeObserver = class ResizeObserver {
    observe() {}
    unobserve() {}
    disconnect() {}
  }
})

function makeWidget(variant = {}) {
  const data = createMetricValueDefaults('g_force', { font_values: 'Arial.ttf' }, { displayType: 'g_force' })
  return {
    id: 'g-force-1',
    type: 'g_force',
    data: {
      ...data,
      display_variants: {
        ...data.display_variants,
        g_force: { ...data.display_variants.g_force, ...variant },
      },
    },
  }
}

describe('GForceWidgetEditor axis controls', () => {
  test('swaps axes when a direction selects the axis used by the other direction', async () => {
    const user = userEvent.setup()
    const updateWidgetData = vi.fn()
    const { rerender } = render(<GForceWidgetEditor widget={makeWidget()} updateWidgetData={updateWidgetData} />)

    const horizontalY = within(screen.getByTestId('horizontal-axis-row')).getByRole('tab', { name: 'Y' })
    await user.click(horizontalY)
    expect(updateWidgetData).toHaveBeenCalledWith(
      'g-force-1',
      expect.objectContaining({
        display_variants: expect.objectContaining({
          g_force: expect.objectContaining({ axis_horizontal: 'y', axis_vertical: 'x' }),
        }),
      }),
    )

    updateWidgetData.mockClear()
    rerender(<GForceWidgetEditor widget={makeWidget({ axis_horizontal: 'y', axis_vertical: 'x' })} updateWidgetData={updateWidgetData} />)
    await user.click(within(screen.getByTestId('vertical-axis-row')).getByRole('tab', { name: 'Y' }))
    expect(updateWidgetData).toHaveBeenCalledWith(
      'g-force-1',
      expect.objectContaining({
        display_variants: expect.objectContaining({
          g_force: expect.objectContaining({ axis_horizontal: 'x', axis_vertical: 'y' }),
        }),
      }),
    )
  })

  test('persists horizontal invert changes through the display variant updater', async () => {
    const user = userEvent.setup()
    const updateWidgetData = vi.fn()
    render(<GForceWidgetEditor widget={makeWidget()} updateWidgetData={updateWidgetData} />)

    await user.click(within(screen.getByTestId('horizontal-axis-row')).getByRole('switch', { name: 'Invert sign' }))

    expect(updateWidgetData).toHaveBeenCalledWith(
      'g-force-1',
      expect.objectContaining({
        display_variants: expect.objectContaining({
          g_force: expect.objectContaining({ invert_horizontal: true }),
        }),
      }),
    )
  })
})
