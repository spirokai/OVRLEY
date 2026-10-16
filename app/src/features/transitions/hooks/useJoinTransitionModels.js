/**
 * Transition controls of every join of a program.
 */

import { useMemo } from 'react'
import { useTranslation } from 'react-i18next'
import { useShallow } from 'zustand/react/shallow'
import { findInvalidTransitions } from '@/lib/video-program/program'
import useStore from '@/store/useStore'
import { buildJoinTransitionModel, joinOverride } from '../utils/joinTransitionModel'

/**
 * Returns the control model of each join, keyed by join key. A join whose
 * transition leaves a joined piece without an untouched frame is flagged.
 * @param {{clips: Array<object>, program: {pieces: Array<object>, joins: Array<object>}}} timeline Clips and their program.
 * @returns {Map<string, object>} Control models keyed by `join.key`.
 */
export function useJoinTransitionModels({ clips, program }) {
  const { t } = useTranslation()
  const { transitionDefaults, setJoinTransition } = useStore(
    useShallow((state) => ({ transitionDefaults: state.transitionDefaults, setJoinTransition: state.setJoinTransition })),
  )
  return useMemo(() => {
    const invalidKeys = new Set(findInvalidTransitions(program).flatMap((issue) => issue.joinKeys))
    return new Map(
      program.joins.map((join) => [
        join.key,
        buildJoinTransitionModel({
          join,
          override: joinOverride(join, clips),
          transitionDefaults,
          isInvalid: invalidKeys.has(join.key),
          setJoinTransition,
          t,
        }),
      ]),
    )
  }, [clips, program, setJoinTransition, t, transitionDefaults])
}
