import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { useState } from 'react'
import { beforeEach, describe, expect, test, vi } from 'vitest'
import RenderVideoDialog from '@/features/render-video/components/RenderVideoDialog'
import { DEFAULT_EXPORT_RANGE } from '@/features/template-manager'
import useStore from '@/store/useStore'
import { DEFAULT_CONFIG, DEFAULT_RENDER_PROGRESS } from '@/store/store-utils'

globalThis.ResizeObserver ??= class ResizeObserver {
  observe() {}
  unobserve() {}
  disconnect() {}
}

function RenderVideoDialogHarness({ initialSettings }) {
  const [settings, setSettings] = useState(initialSettings)

  return (
    <RenderVideoDialog
      phase="confirm"
      settings={settings}
      onSettingsChange={(updates) => setSettings((current) => ({ ...current, ...updates }))}
      onClose={vi.fn()}
      onConfirm={vi.fn()}
    />
  )
}

describe('RenderVideoDialog', () => {
  beforeEach(() => {
    useStore.setState(useStore.getInitialState(), true)
    useStore.setState({
      config: {
        ...DEFAULT_CONFIG,
        scene: {
          ...DEFAULT_CONFIG.scene,
        },
      },
      platformOs: 'windows',
      availableCodecs: {
        proresKs: true,
        libx264: true,
      },
      renderProgress: { ...DEFAULT_RENDER_PROGRESS },
    })
  })

  test('shows composite export title and lets imported-video users switch to transparent export', async () => {
    useStore.setState({
      importedVideoPath: 'C:\\video.mp4',
      importedVideoFps: 30,
      importedVideoDuration: 12,
      importedVideoResolution: { width: 1920, height: 1080 },
      videoSyncOffsetSeconds: 5,
    })

    const user = userEvent.setup()

    render(
      <RenderVideoDialogHarness
        initialSettings={{
          renderTarget: 'current',
          fps: 30,
          updateRate: 1,
          exportMode: 'composite',
          exportCodec: 'libx264',
          exportAcceleration: 'cpu',
          qualityType: 'quality',
          qualityValue: 18,
          exportRange: { ...DEFAULT_EXPORT_RANGE },
        }}
      />,
    )

    expect(screen.getByText('Export Settings')).toBeInTheDocument()

    const slider = screen.getByRole('slider', { name: 'Quality' })
    expect(slider).toHaveAttribute('aria-valuenow', '27')
    expect(slider).toHaveAttribute('aria-valuemin', '10')
    expect(slider).toHaveAttribute('aria-valuemax', '35')
    await user.click(screen.getByRole('tab', { name: 'Bitrate' }))
    expect(screen.getByText('20 Mbps')).toBeInTheDocument()
    await user.click(screen.getByRole('tab', { name: 'Quality' }))
    await user.tab()
    await user.keyboard('{ArrowRight}')
    expect(screen.getByText('CRF 20')).toBeInTheDocument()

    await user.click(screen.getByRole('tab', { name: 'Transparent' }))

    expect(screen.getByText('Export Settings')).toBeInTheDocument()
    expect(screen.getByText('Custom Export Range')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /use video range/i })).toBeInTheDocument()
    expect(screen.getByDisplayValue('00:00:05')).toBeInTheDocument()
    expect(screen.getByDisplayValue('00:00:17')).toBeInTheDocument()
  })

  test('preserves export markers when switching an imported-video dialog to transparent export', async () => {
    useStore.setState({
      importedVideoPath: 'C:\\video.mp4',
      importedVideoFps: 30,
      importedVideoDuration: 12,
      importedVideoResolution: { width: 1920, height: 1080 },
      videoSyncOffsetSeconds: 5,
    })

    const user = userEvent.setup()

    render(
      <RenderVideoDialogHarness
        initialSettings={{
          renderTarget: 'current',
          fps: 30,
          updateRate: 1,
          exportMode: 'composite',
          exportCodec: 'libx264',
          exportAcceleration: 'cpu',
          qualityType: 'quality',
          qualityValue: 18,
          exportRange: {
            ...DEFAULT_EXPORT_RANGE,
            type: 'custom',
            from: 2,
            to: 8,
          },
        }}
      />,
    )

    await user.click(screen.getByRole('tab', { name: 'Transparent' }))

    expect(screen.getByDisplayValue('00:00:02')).toBeInTheDocument()
    expect(screen.getByDisplayValue('00:00:08')).toBeInTheDocument()
    expect(screen.queryByDisplayValue('00:00:05')).not.toBeInTheDocument()
    expect(screen.queryByDisplayValue('00:00:17')).not.toBeInTheDocument()
  })

  test('commits the edited output path instead of the previous draft value', async () => {
    const user = userEvent.setup()

    render(
      <RenderVideoDialogHarness
        initialSettings={{
          renderTarget: 'current',
          fps: 30,
          updateRate: 1,
          exportMode: 'transparent',
          exportCodec: 'prores_ks',
          exportAcceleration: 'cpu',
          exportRange: { ...DEFAULT_EXPORT_RANGE },
          outputPath: 'C:\\renders\\previous.mov',
        }}
      />,
    )

    const outputPathInput = screen.getByRole('textbox', { name: 'Output path' })
    await user.clear(outputPathInput)
    await user.type(outputPathInput, 'C:\\missing\\nested\\output.mov')
    await user.tab()

    expect(outputPathInput).toHaveValue('C:\\missing\\nested\\output.mov')
  })

  test('switches to batch mode with folder pickers in place of the single output file', async () => {
    const user = userEvent.setup()

    render(<RenderVideoDialogHarness initialSettings={transparentSettings()} />)

    expect(screen.queryByRole('tab', { name: 'Full Video' })).not.toBeInTheDocument()
    await user.click(screen.getByRole('tab', { name: 'Batch' }))

    expect(screen.getByText('Video folder')).toBeInTheDocument()
    expect(screen.getByText('Output folder')).toBeInTheDocument()
    expect(screen.queryByRole('textbox', { name: 'Output path' })).not.toBeInTheDocument()
    expect(screen.queryByText('Custom Export Range')).not.toBeInTheDocument()
    expect(screen.getByRole('button', { name: /start batch render/i })).toBeDisabled()

    await user.click(screen.getByRole('tab', { name: 'Full Video' }))

    expect(screen.getByText("Locked to each video's FPS")).toBeInTheDocument()
  })

  test('does not block batch mode on the current video resolution mismatch', async () => {
    useStore.setState({
      importedVideoPath: 'C:\\video.mp4',
      importedVideoFps: 30,
      importedVideoDuration: 12,
      importedVideoResolution: { width: 640, height: 360 },
    })
    const user = userEvent.setup()

    render(<RenderVideoDialogHarness initialSettings={transparentSettings()} />)

    expect(screen.getByText(/must match imported video/i)).toBeInTheDocument()
    await user.click(screen.getByRole('tab', { name: 'Batch' }))

    expect(screen.queryByText(/must match imported video/i)).not.toBeInTheDocument()
    expect(screen.getByText('Video folder')).toBeInTheDocument()
  })

  test('enables batch start once videos are queued and an output folder is chosen', () => {
    useStore.getState().setBatchQueueFromPaths(['C:\\videos\\ride.mp4'])
    useStore.getState().setBatchOutputFolder('C:\\renders')

    render(<RenderVideoDialogHarness initialSettings={{ ...transparentSettings(), renderTarget: 'batch' }} />)

    expect(screen.getByText('ride.mp4')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /start batch render/i })).toBeEnabled()
  })

  test('falls back to transparent export when returning to the current video without an import', async () => {
    const user = userEvent.setup()

    render(<RenderVideoDialogHarness initialSettings={{ ...transparentSettings(), renderTarget: 'batch' }} />)

    await user.click(screen.getByRole('tab', { name: 'Full Video' }))
    await user.click(screen.getByRole('tab', { name: 'Current video' }))

    expect(screen.queryByRole('tab', { name: 'Full Video' })).not.toBeInTheDocument()
    expect(screen.getByText('Custom Export Range')).toBeInTheDocument()
    expect(screen.getByRole('textbox', { name: 'Output path' })).toBeInTheDocument()
  })
})

function transparentSettings() {
  return {
    renderTarget: 'current',
    fps: 30,
    updateRate: 1,
    exportMode: 'transparent',
    exportCodec: 'prores_ks',
    exportAcceleration: 'cpu',
    exportRange: { ...DEFAULT_EXPORT_RANGE },
    outputPath: 'C:\\renders\\overlay.mov',
  }
}
