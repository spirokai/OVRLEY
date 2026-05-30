import { render, screen } from '@testing-library/react'
import { describe, expect, test, vi } from 'vitest'

vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({
    minimize: vi.fn(),
    toggleMaximize: vi.fn(),
    close: vi.fn(),
  }),
}))

let TitleBar
beforeAll(async () => {
  const mod = await import('@/features/app-shell/components/TitleBar')
  TitleBar = mod.default
})

describe('TitleBar', () => {
  test('renders minimize, maximize, and close buttons', () => {
    render(<TitleBar />)

    expect(screen.getByLabelText('Minimize')).toBeDefined()
    expect(screen.getByLabelText('Maximize')).toBeDefined()
    expect(screen.getByLabelText('Close')).toBeDefined()
  })
})
