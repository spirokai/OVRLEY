import { getPreference, setPreference } from '@/lib/preferences-store'
import { requireUnitSystem } from '@/lib/widget/unit-system'
import useStore from '@/store/useStore'

const UNIT_SYSTEM_PREFERENCE_KEY = 'unitSystem'

/** Hydrates the optional persisted unit system before React mounts. */
export async function hydrateUnitSystemPreference() {
  const system = await getPreference(UNIT_SYSTEM_PREFERENCE_KEY)
  if (system === null || system === undefined) return
  useStore.getState().initializeUnitSystem(requireUnitSystem(system))
}

/**
 * Converts the open overlay to the selected unit system and persists the choice.
 * @param {string} system Unit system selected by the user.
 * @returns {Promise<void>}
 */
export async function changeUnitSystemPreference(system) {
  useStore.getState().setUnitSystem(requireUnitSystem(system))
  await setPreference(UNIT_SYSTEM_PREFERENCE_KEY, system)
}
