import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vite-plus/test'
import BatchRenderQueueRow from '@/features/render-video/components/BatchRenderQueueRow'

function renderFailedRow(error) {
  render(
    <BatchRenderQueueRow
      item={{ id: 'ride', filename: 'ride.mp4', path: 'C:\\ride.mp4', status: 'failed', error }}
      isActive={false}
      batchRunning={false}
      showBatchProgress
      currentItemProgress={null}
      setBatchItemSkipOverlay={() => {}}
    />,
  )
}

describe('BatchRenderQueueRow', () => {
  it('shows a failure in full, in its own case, for selecting and copying', () => {
    const error = 'Encoding error: Failed writing composite overlay frame.\nFFmpeg stderr:\nUnknown filter overlay_cuda in C:\\Rides\\ride.mp4'
    renderFailedRow(error)

    const message = screen.getByText(/Unknown filter overlay_cuda/)
    expect(message.textContent).toBe(error)
    expect(message).toHaveClass('select-text', 'normal-case', 'whitespace-pre-wrap')
    expect(message).not.toHaveClass('truncate')
  })
})
