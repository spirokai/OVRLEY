import { fireEvent, render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import { SyncDoctorDrawerContent } from '@/features/toolbar/components/SyncDoctorDrawerContent'
import { describeVisualSyncCandidate } from '@/features/toolbar/utils/visualSyncUtils'

vi.mock('react-i18next', () => ({ useTranslation: () => ({ t: (key) => key }) }))

describe('visual sync candidate presentation', () => {
  it('shows coverage separately from agreement and only applies accepted offsets', () => {
    const candidate = {
      offset_seconds: 17239,
      accepted: true,
      correlation: 0.7505,
      nomination_correlation: 0.7401,
      nomination_margin: 0.3096,
      observed_seconds: 437.7534,
      observed_fraction: 0.47277,
      retained_video_observation_fraction: 1,
      sections: [{ video_start_seconds: 616, video_end_seconds: 924, held_out: true, correlation: 0.7715, agrees: true }],
      rejection_reasons: [],
    }
    const t = (key, values = {}) => `${key} ${Object.values(values).join(' ')}`.trim()
    const accepted = describeVisualSyncCandidate(candidate, false, t)
    const rejected = describeVisualSyncCandidate(
      { ...candidate, offset_seconds: 20, accepted: false, rejection_reasons: ['weak_turning_agreement'] },
      false,
      t,
    )
    const apply = vi.fn()
    render(
      <SyncDoctorDrawerContent sync={{ ready: true, busy: false, status: 'matched', candidates: [accepted, rejected], start: vi.fn(), apply }} />,
    )
    expect(screen.getByRole('button', { name: 'syncDoctor.diagnostic 20.00' })).toBeDisabled()
    expect(screen.getAllByText('syncDoctor.evidence 0.750 0.740 0.310')).toHaveLength(2)
    expect(screen.getAllByText('syncDoctor.support 437.8 47.3 100.0')).toHaveLength(2)
    expect(screen.getAllByText(/syncDoctor.heldOut/)).toHaveLength(2)
    fireEvent.click(screen.getByRole('button', { name: 'syncDoctor.apply 17239.00' }))
    expect(apply).toHaveBeenCalledWith(17239)
    expect(screen.queryByText(/confidence|uncertainty|legacy|prototype/i)).not.toBeInTheDocument()
  })
})
