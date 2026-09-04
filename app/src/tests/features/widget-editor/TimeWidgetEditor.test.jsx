import { beforeAll, describe, expect, test, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import TimeWidgetEditor from '@/features/widget-editor/components/TimeWidgetEditor'

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

describe('TimeWidgetEditor', () => {
  test('renders the shared alignment control and commits a canonical selection', async () => {
    const user = userEvent.setup()
    const updateWidgetData = vi.fn()
    const widget = {
      id: 'time-0',
      type: 'time',
      data: { content_alignment: 'left', font: 'Arial.ttf', font_size: 72, format: 'time-24' },
    }

    render(<TimeWidgetEditor widget={widget} updateWidgetData={updateWidgetData} setNumericField={vi.fn()} />)

    await user.click(screen.getByRole('radio', { name: 'Align right' }))
    expect(updateWidgetData).toHaveBeenCalledWith('time-0', { content_alignment: 'right' })
  })
})
