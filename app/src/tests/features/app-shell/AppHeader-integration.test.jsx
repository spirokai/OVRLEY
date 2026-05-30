/**
 * Integration test for AppHeader with grouped prop objects.
 *
 * Pins the current prop contract before the grouped-props pattern is unwound.
 * Verifies each child section receives correct props from AppHeader.
 */

import { render } from '@testing-library/react'
import { describe, expect, test, vi } from 'vitest'

vi.mock('@tauri-apps/api/core', () => ({ convertFileSrc: (path) => path }))
vi.mock('@/features/app-shell/components/ActivitySection', () => ({
  default: (props) => <div data-testid="activity-section" data-props={JSON.stringify(Object.keys(props))} />,
}))
vi.mock('@/features/app-shell/components/EditorToolbar', () => ({
  default: (props) => <div data-testid="editor-toolbar" data-props={JSON.stringify(Object.keys(props))} />,
}))
vi.mock('@/features/app-shell/components/ActionButtons', () => ({
  default: (props) => <div data-testid="action-buttons" data-props={JSON.stringify(Object.keys(props))} />,
}))

import AppHeader from '@/features/app-shell/components/AppHeader'

const defaultProps = {
  activityControls: { activityLabel: 'Test Activity', onOpenActivityFile: vi.fn() },
  backendStatus: 'connected',
  editorControls: {
    backgroundMode: 'checker',
    gridVisible: true,
    onResetZoom: vi.fn(),
    onSetBackgroundMode: vi.fn(),
    onSetGridVisible: vi.fn(),
    onSetSnapToGrid: vi.fn(),
    onZoomIn: vi.fn(),
    onZoomOut: vi.fn(),
    snapToGrid: false,
    zoomLevel: 1,
  },
  onOpenDownloads: vi.fn(),
  renderControls: {
    onOpenRenderDialog: vi.fn(),
    onRenderPreviewFrame: undefined,
    renderPreviewFrameDisabled: undefined,
    renderDisabled: false,
    renderTooltipContent: null,
    renderingVideo: false,
  },
  templateControls: {
    config: {},
    handleCreateNewTemplate: vi.fn(),
    handleImportTemplate: vi.fn(),
    handleSaveTemplate: vi.fn(),
    handleTemplateChange: vi.fn(),
    loadedTemplateFilename: null,
    loadedTemplateSource: null,
    showTemplateStatus: false,
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
  test('renders all three child sections', () => {
    const { getByTestId } = render(<AppHeader {...defaultProps} />)

    expect(getByTestId('activity-section')).toBeTruthy()
    expect(getByTestId('editor-toolbar')).toBeTruthy()
    expect(getByTestId('action-buttons')).toBeTruthy()
  })

  test('ActivitySection receives activityLabel and template CRUD props', () => {
    const { getByTestId } = render(<AppHeader {...defaultProps} />)
    const props = JSON.parse(getByTestId('activity-section').dataset.props)

    expect(props).toContain('activityLabel')
    expect(props).toContain('onOpenActivityFile')
    expect(props).toContain('config')
    expect(props).toContain('handleSaveTemplate')
  })

  test('EditorToolbar receives background/zoom/grid props', () => {
    const { getByTestId } = render(<AppHeader {...defaultProps} />)
    const props = JSON.parse(getByTestId('editor-toolbar').dataset.props)

    expect(props).toContain('backgroundMode')
    expect(props).toContain('zoomLevel')
    expect(props).toContain('gridVisible')
    expect(props).toContain('snapToGrid')
  })

  test('ActionButtons receives render and backend props', () => {
    const { getByTestId } = render(<AppHeader {...defaultProps} />)
    const props = JSON.parse(getByTestId('action-buttons').dataset.props)

    expect(props).toContain('renderDisabled')
    expect(props).toContain('renderingVideo')
    expect(props).toContain('backendStatus')
  })
})
