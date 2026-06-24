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
          fps: 30,
          updateRate: 1,
          exportMode: 'composite',
          exportCodec: 'libx264',
          exportAcceleration: 'cpu',
          exportBitrate: 20,
          exportRange: { ...DEFAULT_EXPORT_RANGE },
        }}
      />,
    )

    expect(screen.getByText('Composite Video Export Settings')).toBeInTheDocument()

    await user.click(screen.getByRole('switch', { name: /transparent export/i }))

    expect(screen.getByText('Transparent Export Settings')).toBeInTheDocument()
    expect(screen.getByText('Custom Export Range')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /use video range/i })).toBeInTheDocument()
    expect(screen.getByDisplayValue('00:00:05')).toBeInTheDocument()
    expect(screen.getByDisplayValue('00:00:17')).toBeInTheDocument()
  })
})
