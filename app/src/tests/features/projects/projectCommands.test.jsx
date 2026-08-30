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
        status="Unsaved"
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

  test('disables Save Project when the document is Saved, in the File menu', () => {
    const onSaveProject = vi.fn()
    render(
      <ActivitySection
        appVersion="v1.0.0"
        onImportActivity={vi.fn()}
        onImportVideo={vi.fn()}
        onNewProject={vi.fn()}
        onLoadProject={vi.fn()}
        onSaveProject={onSaveProject}
        onSaveProjectAs={vi.fn()}
        status="Saved"
      />,
    )

    fireEvent.click(screen.getByRole('button', { name: 'File menu' }))
    expect(screen.getByRole('button', { name: 'Save Project' })).toBeDisabled()
    expect(screen.getByRole('button', { name: 'Save Project As' })).toBeEnabled()
  })

  test('disables Save Project when the document is Saved, in the Projects drawer', () => {
    const onSave = vi.fn()
    render(
      <ProjectsDrawerContent
        projectName={null}
        projectPath={null}
        status="Saved"
        busy={false}
        onNew={vi.fn()}
        onOpen={vi.fn()}
        onSave={onSave}
        onSaveAs={vi.fn()}
      />,
    )

    expect(screen.getByRole('button', { name: 'Save Project' })).toBeDisabled()
    expect(screen.getByRole('button', { name: 'Save Project As' })).toBeEnabled()
  })

  test('offers loading without the missing source', () => {
    const onLoadAnyway = vi.fn()
    render(<MissingSourceDialog sourceRole="activity" open onLocate={vi.fn()} onLoadAnyway={onLoadAnyway} onCancel={vi.fn()} />)

    fireEvent.click(screen.getByRole('button', { name: 'Load Anyway' }))

    expect(onLoadAnyway).toHaveBeenCalledOnce()
  })
})
