import { fireEvent, render, screen } from '@testing-library/react'
import { beforeEach, describe, expect, test } from 'vite-plus/test'
import { CutRemovalDialog } from '@/features/player/components/CutRemovalDialog'
import useStore from '@/store/useStore'
import { createTestVideo, videoStoreState } from '@/tests/helpers/videoFixtures'

let cutId

beforeEach(() => {
  useStore.setState(useStore.getInitialState(), true)
  useStore.setState(videoStoreState([createTestVideo({ id: 'ride', durationSeconds: 10 })]))
  cutId = useStore.getState().cutVideo('ride', 150)
  useStore.getState().setVideoSegmentExcluded('ride', cutId, true)
})

describe('CutRemovalDialog', () => {
  test('stays closed until a cut between different states is removed', () => {
    render(<CutRemovalDialog />)
    expect(screen.queryByRole('dialog')).toBeNull()
  })

  test.each([
    ['Include both', false],
    ['Exclude both', true],
  ])('%s merges the parts in that state', (name, excluded) => {
    useStore.getState().requestCutRemoval('ride', cutId)
    render(<CutRemovalDialog />)
    fireEvent.click(screen.getByRole('button', { name }))
    const edits = useStore.getState().videoEdits.ride
    expect(edits.cuts).toEqual([])
    expect(edits.segments.start.excluded).toBe(excluded)
    expect(useStore.getState().cutRemovalRequest).toBeNull()
  })

  test('keeping the cut changes nothing', () => {
    useStore.getState().requestCutRemoval('ride', cutId)
    render(<CutRemovalDialog />)
    fireEvent.click(screen.getByRole('button', { name: 'Keep the cut' }))
    expect(useStore.getState().videoEdits.ride.cuts).toHaveLength(1)
    expect(useStore.getState().cutRemovalRequest).toBeNull()
  })
})
