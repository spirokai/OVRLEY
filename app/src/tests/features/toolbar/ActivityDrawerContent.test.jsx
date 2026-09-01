import { fireEvent, render, screen } from '@testing-library/react'
import { describe, expect, test, vi } from 'vitest'

vi.mock('@/features/toolbar/hooks/useFileDropZone', () => ({
  useFileDropZone: () => ({
    dragPosition: null,
    dropZoneRef: { current: null },
    isDraggingFile: false,
    isOverDropZone: false,
    dropZoneProps: {},
  }),
}))

import { ActivityDrawerContent } from '@/features/toolbar'

const activitySummary = {
  availableMetrics: [
    { attribute: 'cadence', source: 'direct' },
    { attribute: 'gps_coordinates', source: 'direct' },
    { attribute: 'gradient', source: 'derived' },
    { attribute: 'speed', source: 'mixed' },
    { attribute: 'barometric_altitude', source: 'direct' },
  ],
  durationSeconds: 3723,
  fileFormat: 'fit',
  fileName: 'morning-ride.fit',
  originalSampleCount: 3599,
  sport: 'cycling',
  syncTime: '2026-07-18T07:20:03.000Z',
  timezone: 'Europe/Zurich',
  totalDistanceMeters: 42195,
}

describe('ActivityDrawerContent', () => {
  test('offers browse and drop import controls', () => {
    const onBrowseActivity = vi.fn()
    render(
      <ActivityDrawerContent
        activitySummary={null}
        filename={null}
        onBrowseActivity={onBrowseActivity}
        onDeleteActivity={vi.fn()}
        onDropActivityFiles={vi.fn()}
      />,
    )

    fireEvent.click(screen.getByRole('button', { name: 'Load activity' }))

    expect(onBrowseActivity).toHaveBeenCalledOnce()
    expect(screen.getByText('Drop activity file')).toBeInTheDocument()
    expect(screen.queryByText('Activity details')).not.toBeInTheDocument()
  })

  test('renders localized activity metadata and metric provenance grids', () => {
    const onDeleteActivity = vi.fn()
    render(
      <ActivityDrawerContent
        activitySummary={activitySummary}
        filename="morning-ride.fit"
        onBrowseActivity={vi.fn()}
        onDeleteActivity={onDeleteActivity}
        onDropActivityFiles={vi.fn()}
      />,
    )

    expect(screen.getByText('morning-ride.fit')).toBeInTheDocument()
    expect(screen.getByText('FIT')).toBeInTheDocument()
    expect(screen.getByText('Europe/Zurich')).toBeInTheDocument()
    expect(screen.getByText('42.2 km')).toBeInTheDocument()
    expect(screen.getByText('1:02:03')).toBeInTheDocument()
    expect(screen.getByText('cycling')).toBeInTheDocument()
    expect(screen.getByRole('heading', { name: 'Extracted' })).toBeInTheDocument()
    expect(screen.getByRole('heading', { name: 'Calculated' })).toBeInTheDocument()
    expect(screen.getByText('Cadence')).toBeInTheDocument()
    expect(screen.getByText('Barometric Altitude')).toBeInTheDocument()
    expect(screen.getByText('GPS Coordinates')).toBeInTheDocument()
    expect(screen.queryByText('G Force X')).not.toBeInTheDocument()
    expect(screen.getByText('Gradient')).toBeInTheDocument()
    expect(screen.getByText('Speed')).toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: 'Delete activity' }))
    expect(onDeleteActivity).toHaveBeenCalledOnce()
  })
})
