/**
 * Integration test for AppHeader with canonical owner objects.
 */

import { render } from '@testing-library/react'
import { describe, expect, test, vi } from 'vitest'

vi.mock('@tauri-apps/api/core', () => ({ convertFileSrc: (path) => path }))
vi.mock('@/features/app-shell/components/ActivitySection', () => ({
  default: (props) => <div data-testid="activity-section" data-props={JSON.stringify(Object.keys(props))} />,
}))
vi.mock('@/features/app-shell/components/TemplateSection', () => ({
  default: (props) => <div data-testid="template-section" data-props={JSON.stringify(Object.keys(props))} />,
}))
vi.mock('@/features/app-shell/components/ActionButtons', () => ({
  default: (props) => <div data-testid="action-buttons" data-props={JSON.stringify(Object.keys(props))} />,
}))

import AppHeader from '@/features/app-shell/components/AppHeader'

const defaultProps = {
  activityImport: { activityFilename: 'Test Activity', handleActivityFileOpen: vi.fn() },
  appShell: { config: {} },
  backendState: { backendStatus: 'connected' },
  editorShell: { debugModeEnabled: false, openKeyboardShortcuts: vi.fn() },
  onOpenDownloads: vi.fn(),
  projectLifecycle: {
    handleNewProject: vi.fn(),
    handleOpenProject: vi.fn(),
    handleSaveProject: vi.fn(),
    handleSaveProjectAs: vi.fn(),
  },
  renderWorkflow: {
    openRenderDialog: vi.fn(),
    handleRenderPreviewFrame: undefined,
    renderPreviewFrameDisabled: undefined,
    renderDisabled: false,
    renderTooltipContent: null,
    renderingVideo: false,
  },
  templateManagement: {
    handleCreateNewTemplate: vi.fn(),
    handleImportTemplate: vi.fn(),
    handleSaveTemplate: vi.fn(),
    handleTemplateChange: vi.fn(),
    loadedTemplateFilename: null,
    loadedTemplateSource: null,
    showTemplateStatus: false,
    templateSelectorOpen: false,
    setTemplateSelectorOpen: vi.fn(),
    templates: [],
  },
  videoControls: {
    debugModeEnabled: false,
    importedBackgroundImageFilename: null,
    importedMediaFilename: null,
    importedVideoFilename: null,
    handleImportVideo: vi.fn(),
    clearImportedVideo: vi.fn(),
  },
}

describe('AppHeader grouped-props contract', () => {
  test('renders activity, template, and action sections', () => {
    const { getByTestId } = render(<AppHeader {...defaultProps} />)

    expect(getByTestId('activity-section')).toBeTruthy()
    expect(getByTestId('template-section')).toBeTruthy()
    expect(getByTestId('action-buttons')).toBeTruthy()
  })

  test('ActivitySection receives project and media import props', () => {
    const { getByTestId } = render(<AppHeader {...defaultProps} />)
    const props = JSON.parse(getByTestId('activity-section').dataset.props)

    expect(props).toContain('onImportActivity')
    expect(props).toContain('onImportVideo')
    expect(props).toContain('onNewProject')
    expect(props).toContain('onLoadProject')
    expect(props).toContain('onSaveProject')
    expect(props).toContain('onSaveProjectAs')
    expect(props).toContain('appVersion')
  })

  test('TemplateSection receives template selector and CRUD props', () => {
    const { getByTestId } = render(<AppHeader {...defaultProps} />)
    const props = JSON.parse(getByTestId('template-section').dataset.props)

    expect(props).toContain('handleTemplateChange')
    expect(props).toContain('handleCreateNewTemplate')
    expect(props).toContain('handleSaveTemplate')
    expect(props).toContain('handleImportTemplate')
  })

  test('ActionButtons receives render and backend props', () => {
    const { getByTestId } = render(<AppHeader {...defaultProps} />)
    const props = JSON.parse(getByTestId('action-buttons').dataset.props)

    expect(props).toContain('renderDisabled')
    expect(props).toContain('renderingVideo')
    expect(props).toContain('backendStatus')
    expect(props).toContain('onOpenKeyboardShortcuts')
  })
})
