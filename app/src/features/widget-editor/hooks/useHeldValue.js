import { useState } from 'react'

/**
 * Keeps returning the last value seen before `hold` became true until `hold` is released.
 *
 * @template T
 * @param {T} value - Live value.
 * @param {boolean} hold - Whether to freeze the returned value.
 * @returns {T} The live value, or the frozen one while held.
 */
export default function useHeldValue(value, hold) {
  const [held, setHeld] = useState(value)
  if (!hold && held !== value) setHeld(value)
  return hold ? held : value
}
