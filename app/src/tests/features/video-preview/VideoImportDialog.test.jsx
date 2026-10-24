import { fireEvent, render, screen, within } from '@testing-library/react'
import { describe, expect, test, vi } from 'vitest'
import { VideoImportDialog } from '@/features/video-preview'

function row(overrides = {}) {
  return {
    id: 'ride',
    filename: 'ride.mp4',
    path: '/clips/ride.mp4',
    isNew: true,
    isIncluded: true,
    hasOtherResolution: false,
    isFar: false,
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
    onIncludedChange: vi.fn(),
    ...overrides,
  }
}

function model(overrides = {}) {
  return {
    open: true,
    phase: 'reviewing',
    fileCount: 3,
    windowKind: 'activity',
    resolutionLabel: '1920×1080',
    rows: [
      row(),
      row({ id: 'early', filename: 'early.mp4', path: '/clips/early.mp4', isIncluded: false, isFar: true, offsetSeconds: -4500 }),
      row({
        id: 'extra',
        filename: 'extra.mp4',
        path: '/clips/extra.mp4',
        isIncluded: false,
        isManual: true,
        isPositioned: true,
      }),
      row({ id: 'old', filename: 'old.mp4', path: '/clips/old.mp4', isNew: false, isFar: true, isManual: true }),
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
    farConfirmation: null,
    error: null,
    onCancel: vi.fn(),
    onConfirm: vi.fn(),
    onBack: vi.fn(),
    ...overrides,
  }
}

describe('VideoImportDialog', () => {
  test('lists every video with its switch, sync, status, and the reasons one file is not possible', () => {
    const props = model()
    render(<VideoImportDialog {...props} />)

    const ride = screen.getByRole('listitem', { name: 'ride.mp4' })
    expect(within(ride).getByText('New')).toBeInTheDocument()
    expect(within(ride).getByText('Will be imported')).toBeInTheDocument()
    expect(within(ride).getByRole('textbox', { name: 'Sync of ride.mp4' })).toHaveValue('05:30')
    expect(within(ride).getByText('330 s')).toBeInTheDocument()
    fireEvent.click(within(ride).getByRole('switch', { name: 'Import ride.mp4' }))
    expect(props.rows[0].onIncludedChange).toHaveBeenCalledExactlyOnceWith(false)

    const early = screen.getByRole('listitem', { name: 'early.mp4' })
    expect(early).toHaveTextContent('Not imported')
    expect(early).toHaveTextContent('Starts more than an hour before or after the activity')
    expect(screen.getByRole('listitem', { name: 'notes.txt' })).toHaveTextContent('Not imported: Not a supported video file (MP4, MOV, MKV).')
    expect(screen.getByRole('alert')).toHaveTextContent('Videos overlap (ride.mp4, old.mp4).')

    // A switched-off row cannot be edited.
    const extra = screen.getByRole('listitem', { name: 'extra.mp4' })
    expect(within(extra).getByRole('textbox', { name: 'Sync of extra.mp4' })).toBeDisabled()
    for (const name of ['Place extra.mp4 automatically again', 'Move extra.mp4 up', 'Move extra.mp4 down']) {
      expect(within(extra).getByRole('button', { name })).toBeDisabled()
    }
    fireEvent.click(within(extra).getByRole('switch', { name: 'Import extra.mp4' }))
    expect(props.rows[2].onIncludedChange).toHaveBeenCalledExactlyOnceWith(true)

    const old = screen.getByRole('listitem', { name: 'old.mp4' })
    expect(old).toHaveTextContent('Already imported')
    fireEvent.click(within(old).getByRole('switch', { name: 'Keep old.mp4 in the project' }))
    expect(props.rows[3].onIncludedChange).toHaveBeenCalledExactlyOnceWith(false)
    fireEvent.click(within(old).getByRole('button', { name: 'Place old.mp4 automatically again' }))
    expect(props.rows[3].onOffsetReset).toHaveBeenCalledOnce()

    fireEvent.click(screen.getByRole('button', { name: 'Import 1 video' }))
    expect(props.onConfirm).toHaveBeenCalledOnce()
  })

  test('a video with another resolution cannot be switched on', () => {
    render(
      <VideoImportDialog
        {...model({ rows: [row({ isIncluded: false, hasOtherResolution: true, formatLabel: '3840×2160 · 25 fps', onIncludedChange: null })] })}
      />,
    )
    const ride = screen.getByRole('listitem', { name: 'ride.mp4' })
    expect(ride).toHaveTextContent('Cannot be imported: the overlay is laid out for 1920×1080')
    expect(within(ride).getByRole('switch', { name: 'Import ride.mp4' })).toBeDisabled()
    expect(within(ride).getByRole('textbox', { name: 'Sync of ride.mp4' })).toBeDisabled()
  })

  test('an existing video switched off is removed from the project', () => {
    render(<VideoImportDialog {...model({ rows: [row({ isNew: false, isIncluded: false })] })} />)
    expect(screen.getByRole('listitem', { name: 'ride.mp4' })).toHaveTextContent('Will be removed from the project')
  })

  test('asks before importing videos far from the activity', () => {
    const props = model({ phase: 'confirming', farConfirmation: { names: ['early.mp4', 'late.mp4'], from: '-01:15:00', to: '15:20:00' } })
    render(<VideoImportDialog {...props} />)

    expect(screen.getAllByRole('alert')[1]).toHaveTextContent(
      'These videos start far from the activity or the other videos: early.mp4, late.mp4. The timeline will run from -01:15:00 to 15:20:00. Import them anyway?',
    )
    fireEvent.click(screen.getByRole('button', { name: 'Back' }))
    expect(props.onBack).toHaveBeenCalledOnce()
    fireEvent.click(screen.getByRole('button', { name: 'Import anyway' }))
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
