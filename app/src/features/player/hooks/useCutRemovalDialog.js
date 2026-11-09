/**
 * State and commands of the dialog that asks how a removed cut merges its parts.
 */

import { useShallow } from 'zustand/react/shallow'
import useStore from '@/store/useStore'

/**
 * Returns the props of `CutRemovalDialog`: open while a removed cut separates an
 * included part from an excluded one (see `requestCutRemoval`).
 * @returns {{open: boolean, onKeepCut: function(): void, onMerge: function(boolean): void}} Dialog props;
 *   `onMerge` takes whether the merged part is excluded from export.
 */
export default function useCutRemovalDialog() {
  const { open, resolveCutRemoval, dismissCutRemoval } = useStore(
    useShallow((state) => ({
      open: state.cutRemovalRequest !== null,
      resolveCutRemoval: state.resolveCutRemoval,
      dismissCutRemoval: state.dismissCutRemoval,
    })),
  )
  return { open, onKeepCut: dismissCutRemoval, onMerge: resolveCutRemoval }
}
