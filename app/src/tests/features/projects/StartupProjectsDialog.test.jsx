import { render } from '@testing-library/react'
import { describe, expect, test, vi } from 'vitest'
import StartupProjectsDialog from '@/features/projects/components/StartupProjectsDialog'

describe('StartupProjectsDialog', () => {
  test('displays an archived project thumbnail inside its card', () => {
    const thumbnailDataUrl = 'data:image/png;base64,dGh1bWJuYWls'
    render(
      <StartupProjectsDialog
        open
        projects={[{ name: 'Race', path: 'C:\\Projects\\Race.oly', thumbnailDataUrl }]}
        openingPath={null}
        onDismiss={vi.fn()}
        onNewProject={vi.fn()}
        onOpenProject={vi.fn()}
      />,
    )

    const thumbnail = document.querySelector(`img[src="${thumbnailDataUrl}"]`)
    expect(thumbnail).toHaveClass('object-cover')
  })
})
