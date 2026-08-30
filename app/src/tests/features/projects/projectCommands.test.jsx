import { fireEvent, render, screen } from '@testing-library/react'
import { describe, expect, test, vi } from 'vitest'
import ActivitySection from '@/features/app-shell/components/ActivitySection'
import { MissingSourceDialog } from '@/features/projects'
import { ProjectsDrawerContent } from '@/features/toolbar'

describe('New Project command surfaces', () => {
  test('runs from the File menu', () => {
    const onNewProject = vi.fn()
    render(
      <ActivitySection
        appVersion="v1.0.0"
        onImportActivity={vi.fn()}
        onImportVideo={vi.fn()}
        onNewProject={onNewProject}
        onLoadProject={vi.fn()}
        onSaveProject={vi.fn()}
        onSaveProjectAs={vi.fn()}
      />,
    )

    fireEvent.click(screen.getByRole('button', { name: 'File menu' }))
    fireEvent.click(screen.getByRole('button', { name: 'New Project' }))

    expect(onNewProject).toHaveBeenCalledOnce()
  })

  test('runs from the Projects drawer', () => {
    const onNew = vi.fn()
    render(
      <ProjectsDrawerContent
        projectName={null}
        projectPath={null}
        status="Unsaved"
        busy={false}
        onNew={onNew}
        onOpen={vi.fn()}
        onSave={vi.fn()}
        onSaveAs={vi.fn()}
      />,
    )

    fireEvent.click(screen.getByRole('button', { name: 'New Project' }))

    expect(onNew).toHaveBeenCalledOnce()
  })

  test('offers loading without the missing source', () => {
    const onLoadAnyway = vi.fn()
    render(<MissingSourceDialog sourceRole="activity" open onLocate={vi.fn()} onLoadAnyway={onLoadAnyway} onCancel={vi.fn()} />)

    fireEvent.click(screen.getByRole('button', { name: 'Load Anyway' }))

    expect(onLoadAnyway).toHaveBeenCalledOnce()
  })
})
