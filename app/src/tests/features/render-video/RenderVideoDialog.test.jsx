import { act, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { useState } from 'react'
import { beforeEach, describe, expect, test, vi } from 'vite-plus/test'
import RenderVideoDialog from '@/features/render-video/components/RenderVideoDialog'
import { DEFAULT_EXPORT_RANGE } from '@/features/template-manager'
import useStore from '@/store/useStore'
import { DEFAULT_CONFIG, DEFAULT_RENDER_PROGRESS } from '@/store/store-utils'
import { createTestVideo, videoStoreState } from '@/tests/helpers/videoFixtures'

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
    useStore.setState(videoStoreState([createTestVideo({ id: 'clip', path: 'C:\\video.mp4', durationSeconds: 12 })], { clip: 5 }))

    const user = userEvent.setup()

    render(
      <RenderVideoDialogHarness
        initialSettings={{
          outputMode: 'combined',
          fps: 30,
          widgetUpdateRate: 1,
          exportMode: 'composite',
          codec: 'libx264',
          exportAcceleration: 'cpu',
          qualityType: 'quality',
          qualityValue: 18,
          range: { ...DEFAULT_EXPORT_RANGE },
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
    // With videos the export follows their parts: no range inputs, and no whole-activity render without an external activity.
    expect(screen.queryByText('Custom Export Range')).not.toBeInTheDocument()
    expect(screen.queryByRole('switch', { name: 'Render full activity' })).not.toBeInTheDocument()
  })

  test('offers the whole-activity render, off by default, for a transparent video export with an external activity', async () => {
    useStore.setState({
      ...videoStoreState([createTestVideo({ id: 'clip', path: 'C:\\video.mp4', durationSeconds: 12 })], { clip: 5 }),
      parsedActivity: { metadata: {} },
    })
    const user = userEvent.setup()

    render(
      <RenderVideoDialogHarness
        initialSettings={{
          outputMode: 'combined',
          fps: 30,
          widgetUpdateRate: 1,
          exportMode: 'composite',
          codec: 'libx264',
          exportAcceleration: 'cpu',
          qualityType: 'quality',
          qualityValue: 18,
          range: { ...DEFAULT_EXPORT_RANGE },
          renderFullActivity: false,
        }}
      />,
    )

    expect(screen.queryByRole('switch', { name: 'Render full activity' })).not.toBeInTheDocument()
    await user.click(screen.getByRole('tab', { name: 'Transparent' }))
    const fullActivity = screen.getByRole('switch', { name: 'Render full activity' })
    expect(fullActivity).not.toBeChecked()
    await user.click(fullActivity)
    expect(fullActivity).toBeChecked()
  })

  test('commits the edited output path instead of the previous draft value', async () => {
    const user = userEvent.setup()

    render(
      <RenderVideoDialogHarness
        initialSettings={{
          outputMode: 'combined',
          fps: 30,
          widgetUpdateRate: 1,
          exportMode: 'transparent',
          codec: 'prores_ks',
          exportAcceleration: 'cpu',
          range: { ...DEFAULT_EXPORT_RANGE },
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

  test('keeps fractional bar progress, rounds percentage labels and ETA, and preserves completion presentation', () => {
    const id = 'C:/videos/ride.mp4'
    const snapshot = {
      batchId: 'batch-1',
      revision: 1,
      phase: 'rendering',
      rendererBusy: true,
      activeItemId: id,
      plannedFrames: 300,
      processedFrames: 32,
      renderedFrames: 32,
      encodedFrames: 20,
      elapsedSeconds: 1,
      estimatedSecondsRemaining: 59.6,
      currentItemProgress: {
        plannedFrames: 300,
        currentFrames: 32,
        renderedFrames: 32,
        encodedFrames: 20,
        elapsedSeconds: 1,
        estimatedSecondsRemaining: 1.6,
        renderingFps: 30,
      },
      items: [{ id, phase: 'rendering', plannedFrames: 300, currentFrames: 32, renderedFrames: 32, encodedFrames: 20, outcome: null }],
      outputs: [],
      resultCounts: { succeeded: 0, failed: 0, cancelled: 0, unstarted: 0 },
    }
    useStore.setState({
      batchSnapshot: snapshot,
      ...videoStoreState([createTestVideo({ id: 'ride', path: id }), createTestVideo({ id: 'climb', path: 'C:/videos/climb.mp4', importOrder: 1 })], {
        climb: 120,
      }),
    })
    render(<RenderVideoDialogHarness initialSettings={{ ...transparentSettings(), outputMode: 'separate' }} />)

    expect(screen.getByText('11% Complete')).toBeInTheDocument()
    expect(screen.getByText('11%')).toBeInTheDocument()
    const indicators = document.querySelectorAll('[data-slot="progress-indicator"]')
    expect(indicators).toHaveLength(2)
    for (const indicator of indicators) {
      expect(indicator).toHaveStyle({ transform: 'translateX(-89.33333333333333%)' })
    }
    expect(screen.getByText('1:00')).toBeInTheDocument()
    expect(screen.getByText('Est. Remaining: 0:02')).toBeInTheDocument()

    act(() => {
      useStore.getState().applyBatchSnapshot({
        ...snapshot,
        revision: 2,
        phase: 'completed',
        rendererBusy: false,
        activeItemId: null,
        currentItemProgress: null,
        processedFrames: 300,
        renderedFrames: 300,
        encodedFrames: 300,
        estimatedSecondsRemaining: null,
        items: [{ ...snapshot.items[0], phase: 'finished', outcome: { status: 'succeeded', outputPath: 'C:/renders/ride_overlay.mov' } }],
        outputs: ['C:/renders/ride_overlay.mov'],
        resultCounts: { succeeded: 1, failed: 0, cancelled: 0, unstarted: 0 },
      })
    })

    expect(screen.getByRole('heading', { name: 'Export Finished' })).toBeInTheDocument()
    expect(screen.getByText('ride.mp4')).toBeInTheDocument()
    expect(screen.getByText('Done')).toBeInTheDocument()
    expect(screen.queryByText(/1 completed/)).not.toBeInTheDocument()
    expect(screen.queryByText('Est. Remaining')).not.toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Close' })).toBeEnabled()
  })
})

function transparentSettings() {
  return {
    outputMode: 'combined',
    fps: 30,
    widgetUpdateRate: 1,
    exportMode: 'transparent',
    codec: 'prores_ks',
    exportAcceleration: 'cpu',
    range: { ...DEFAULT_EXPORT_RANGE },
    outputPath: 'C:\\renders\\overlay.mov',
  }
}
