import { render, screen } from '@testing-library/react'
import { describe, expect, test, vi } from 'vitest'

vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({
    minimize: vi.fn(),
    toggleMaximize: vi.fn(),
    close: vi.fn(),
  }),
}))

let WindowControls
beforeAll(async () => {
  const mod = await import('@/features/app-shell/components/WindowControls')
  WindowControls = mod.default
})

describe('WindowControls', () => {
  test('renders minimize, maximize, and close buttons', () => {
    render(<WindowControls />)

    expect(screen.getByLabelText('Minimize')).toBeDefined()
    expect(screen.getByLabelText('Maximize')).toBeDefined()
    expect(screen.getByLabelText('Close')).toBeDefined()
  })
})
