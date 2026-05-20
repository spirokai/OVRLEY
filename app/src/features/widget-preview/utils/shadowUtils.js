/**
 * Shadow utilities — text shadow and outline shadow computation that mirrors
 * the Skia renderer's shadow behavior.
 */

/**
 * Extracts the text shadow configuration from scene data.
 *
 * Mirrors the Skia renderer's shadow parameter extraction — reads shadow_strength,
 * shadow_distance, and shadow_color from the data object.
 *
 * @param {object|null|undefined} data - Scene or style data with shadow properties.
 * @returns {{ color: string, distance: number, strength: number }|undefined} Shadow config, or undefined if no shadow.
 */
export function getTextShadowParts(data) {
  const shadowStrength = Number(data?.shadow_strength) || 0
  const shadowDistance = Number(data?.shadow_distance) || 0
  const shadowColor = data?.shadow_color

  if (!shadowColor || (!shadowStrength && !shadowDistance)) return undefined

  return {
    color: shadowColor,
    distance: shadowDistance,
    strength: shadowStrength,
  }
}

/**
 * Formats shadow configuration as a CSS text-shadow string.
 *
 * @param {object|null|undefined} data - Scene or style data with shadow properties.
 * @returns {string|undefined} CSS text-shadow value, or undefined if no shadow.
 */
export function getTextShadow(data) {
  const shadow = getTextShadowParts(data)

  if (!shadow) return undefined

  return `${shadow.distance}px ${shadow.distance}px ${shadow.strength}px ${shadow.color}`
}

/**
 * Builds a CSS text-shadow string that simulates an outline/border around text.
 *
 * Creates multiple offset shadow layers in 8 directions (cardinal + diagonal)
 * at each pixel step up to the border thickness, approximating a stroke effect.
 *
 * @param {object|null|undefined} data - Scene data with border_thickness and border_color.
 * @returns {string} CSS text-shadow value, or empty string if no border.
 */
export function getTextOutlineShadow(data) {
  const borderThickness = Number(data?.border_thickness) || 0
  const borderColor = data?.border_color

  if (!borderThickness || !borderColor) return ''

  const offsets = [
    [-1, 0],
    [1, 0],
    [0, -1],
    [0, 1],
    [-1, -1],
    [1, -1],
    [-1, 1],
    [1, 1],
  ]
  const layers = []

  for (let step = 1; step <= borderThickness; step += 1) {
    offsets.forEach(([x, y]) => {
      layers.push(`${x * step}px ${y * step}px 0 ${borderColor}`)
    })
  }

  return layers.join(', ')
}

/**
 * Combines both outline shadow and drop shadow into a single CSS text-shadow string.
 *
 * @param {object|null|undefined} data - Scene data with border and shadow properties.
 * @returns {string|undefined} Combined CSS text-shadow, or undefined if neither is present.
 */
export function getCombinedTextShadow(data) {
  const outlineShadow = getTextOutlineShadow(data)
  const dropShadow = getTextShadow(data)

  if (outlineShadow && dropShadow) {
    return `${outlineShadow}, ${dropShadow}`
  }

  return outlineShadow || dropShadow || undefined
}
