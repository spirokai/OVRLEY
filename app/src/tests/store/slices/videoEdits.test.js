import { beforeEach, describe, expect, test } from 'vite-plus/test'
import useStore from '@/store/useStore'
import { createTestVideo, videoStoreState } from '@/tests/helpers/videoFixtures'

const edits = () => useStore.getState().videoEdits.ride

beforeEach(() => {
  useStore.setState(useStore.getInitialState(), true)
  useStore.setState(videoStoreState([createTestVideo({ id: 'ride', durationSeconds: 10 })]))
  useStore.temporal.getState().clear()
})

describe('video edits in the store', () => {
  test('a cut, an exclusion, and a transition are one undoable step each', () => {
    const cutId = useStore.getState().cutVideo('ride', 150)
    expect(edits().cuts).toEqual([{ id: cutId, frame: 150 }])
    useStore.getState().setVideoSegmentExcluded('ride', cutId, true)
    expect(edits().segments[cutId].excluded).toBe(true)
    useStore.getState().setJoinTransition({ videoId: 'ride', segmentKey: cutId }, { type: 'fade', durationSeconds: 1 })
    expect(edits().segments[cutId].transitionIn).toEqual({ type: 'fade', durationSeconds: 1 })

    useStore.temporal.getState().undo()
    expect(edits().segments[cutId].transitionIn).toBeNull()
    useStore.temporal.getState().undo()
    expect(edits().segments[cutId].excluded).toBe(false)
    useStore.temporal.getState().undo()
    expect(edits().cuts).toEqual([])
    expect(edits().segments[cutId]).toBeUndefined()
  })

  test('removing a cut between segments in the same state merges them without asking', () => {
    const cutId = useStore.getState().cutVideo('ride', 150)
    useStore.getState().requestCutRemoval('ride', cutId)
    expect(useStore.getState().cutRemovalRequest).toBeNull()
    expect(edits().cuts).toEqual([])
    expect(edits().segments).toEqual({ start: { excluded: false, transitionIn: null } })
  })

  test.each([
    [true, true],
    [false, false],
  ])('removing a cut between different states asks first and applies the choice (excluded: %s)', (choice, expected) => {
    const cutId = useStore.getState().cutVideo('ride', 150)
    useStore.getState().setVideoSegmentExcluded('ride', cutId, true)

    useStore.getState().requestCutRemoval('ride', cutId)
    expect(useStore.getState().cutRemovalRequest).toEqual({ videoId: 'ride', cutId })
    expect(edits().cuts).toHaveLength(1)

    useStore.getState().resolveCutRemoval(choice)
    expect(useStore.getState().cutRemovalRequest).toBeNull()
    expect(edits().cuts).toEqual([])
    expect(edits().segments.start.excluded).toBe(expected)
  })

  test('a trim cuts the video and excludes the outer part in one undoable step, and never passes a cut', () => {
    useStore.getState().setJoinTransition({ videoId: 'ride', segmentKey: 'start' }, { type: 'fade', durationSeconds: 1 })
    const cutId = useStore.getState().cutVideo('ride', 150)

    useStore.getState().trimVideo('ride', 'start', 30)
    const [trim] = edits().cuts
    expect(trim.frame).toBe(30)
    // The video's incoming transition moves to the part that now starts it.
    expect(edits().segments.start).toEqual({ excluded: true, transitionIn: null })
    expect(edits().segments[trim.id]).toEqual({ excluded: false, transitionIn: { type: 'fade', durationSeconds: 1 } })
    useStore.getState().trimVideo('ride', 'end', 270)
    expect(edits().cuts.map((cut) => cut.frame)).toEqual([30, 150, 270])
    expect(edits().segments[edits().cuts[2].id].excluded).toBe(true)

    expect(() => useStore.getState().trimVideo('ride', 'start', 20)).toThrow('already excluded')
    useStore.temporal.getState().undo()
    useStore.temporal.getState().undo()
    expect(edits().cuts).toEqual([{ id: cutId, frame: 150 }])
    expect(edits().segments.start).toEqual({ excluded: false, transitionIn: { type: 'fade', durationSeconds: 1 } })
    expect(() => useStore.getState().trimVideo('ride', 'end', 100)).toThrow('cannot pass a cut')
  })

  test('a start trim moves a selected first segment to the part it now starts', () => {
    useStore.getState().selectClip({ laneId: 'video:ride', segmentKey: 'start' })
    useStore.getState().trimVideo('ride', 'start', 30)
    expect(useStore.getState().selectedClip).toEqual({ laneId: 'video:ride', segmentKey: edits().cuts[0].id })
  })

  test('keeping the cut changes nothing, and removing the video ends the question', () => {
    const cutId = useStore.getState().cutVideo('ride', 150)
    useStore.getState().setVideoSegmentExcluded('ride', cutId, true)
    useStore.getState().requestCutRemoval('ride', cutId)
    useStore.getState().dismissCutRemoval()
    expect(useStore.getState().cutRemovalRequest).toBeNull()
    expect(edits().cuts).toHaveLength(1)

    useStore.getState().requestCutRemoval('ride', cutId)
    useStore.getState().removeImportedVideo('ride')
    expect(useStore.getState().cutRemovalRequest).toBeNull()
  })
})
