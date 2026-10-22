import { fireEvent, render, screen, within } from '@testing-library/react'
import { describe, expect, test, vi } from 'vitest'
import { VideoImportDialog } from '@/features/video-preview'

function row(overrides = {}) {
  return {
    id: 'ride',
    filename: 'ride.mp4',
    path: '/clips/ride.mp4',
    isNew: true,
    verdict: 'import',
    offsetSeconds: 330,
    fps: { num: 30, den: 1 },
    formatLabel: '1920×1080 · 30 fps',
    isManual: false,
    isPositioned: false,
    isIncompatible: false,
    overlaps: [],
    onOffsetCommit: vi.fn(() => null),
    onOffsetReset: vi.fn(),
    onMoveUp: null,
    onMoveDown: null,
    onRemove: null,
    ...overrides,
  }
}

function model(overrides = {}) {
  return {
    open: true,
    phase: 'reviewing',
    fileCount: 3,
    windowKind: 'activity',
    rows: [
      row(),
      row({ id: 'early', filename: 'early.mp4', path: '/clips/early.mp4', verdict: 'excluded', offsetSeconds: -4500 }),
      row({ id: 'old', filename: 'old.mp4', path: '/clips/old.mp4', isNew: false, verdict: 'outside', isManual: true, onRemove: vi.fn() }),
    ],
    failures: [{ key: '/clips/notes.txt', name: 'notes.txt', message: 'Not a supported video file (MP4, MOV, MKV).' }],
    importCount: 1,
    issues: ['Videos overlap (ride.mp4, old.mp4). Move a video or delete the overlapping part.'],
    sync: {
      activitySummary: { durationSeconds: 120 },
      canResetCreationTime: false,
      filenameCreationTimeAvailable: false,
      sharedOffsetSeconds: -4500,
      sharedOffsetFps: { num: 30, den: 1 },
      timezone: null,
      timezoneDisabled: false,
      videoSyncTimezoneMode: null,
      videoSyncWarning: null,
      openManualVideoSync: null,
      computeVideoSync: vi.fn(),
      commitSharedOffset: vi.fn(),
      resetVideoCreationTimes: vi.fn(),
      setVideoCreationTimesFromFilenames: vi.fn(),
      setVideoSyncTimezoneMode: vi.fn(),
    },
    transition: {
      type: 'none',
      typeLabel: 'No transition',
      previewType: null,
      durationSeconds: 2,
      durationFps: { num: 30, den: 1 },
      onTypeChange: vi.fn(),
      onDurationCommit: vi.fn(),
    },
    error: null,
    onCancel: vi.fn(),
    onConfirm: vi.fn(),
    ...overrides,
  }
}

describe('VideoImportDialog', () => {
  test('lists every video with its sync, verdict, and the reasons one file is not possible', () => {
    const props = model()
    render(<VideoImportDialog {...props} />)

    const ride = screen.getByRole('listitem', { name: 'ride.mp4' })
    expect(within(ride).getByText('New')).toBeInTheDocument()
    expect(within(ride).getByText('Will be imported')).toBeInTheDocument()
    expect(within(ride).getByRole('textbox', { name: 'Sync of ride.mp4' })).toHaveValue('05:30')
    expect(within(ride).getByText('330 s')).toBeInTheDocument()
    expect(screen.getByRole('listitem', { name: 'early.mp4' })).toHaveTextContent(
      'Not imported: starts more than an hour before or after the activity',
    )
    expect(screen.getByRole('listitem', { name: 'notes.txt' })).toHaveTextContent('Not imported: Not a supported video file (MP4, MOV, MKV).')
    expect(screen.getByRole('alert')).toHaveTextContent('Videos overlap (ride.mp4, old.mp4).')

    const old = screen.getByRole('listitem', { name: 'old.mp4' })
    fireEvent.click(within(old).getByRole('button', { name: 'Remove old.mp4 from the project' }))
    expect(props.rows[2].onRemove).toHaveBeenCalledOnce()
    fireEvent.click(within(old).getByRole('button', { name: 'Place old.mp4 automatically again' }))
    expect(props.rows[2].onOffsetReset).toHaveBeenCalledOnce()

    fireEvent.click(screen.getByRole('button', { name: 'Import 1 video' }))
    expect(props.onConfirm).toHaveBeenCalledOnce()
  })

  test('reads the files first and cannot import while nothing joins', () => {
    const { rerender } = render(<VideoImportDialog open phase="probing" fileCount={4} onCancel={vi.fn()} />)
    expect(screen.getByText('Reading 4 files…')).toBeInTheDocument()

    rerender(<VideoImportDialog {...model({ importCount: 0, error: 'preview server unavailable' })} />)
    expect(screen.getByRole('button', { name: 'Import 0 videos' })).toBeDisabled()
    expect(screen.getByText('preview server unavailable')).toBeInTheDocument()
  })
})
