/**
 * Template grouping and resolution utilities for the app shell.
 * These helpers sort and group templates by resolution for the template selector UI.
 */

/**
 * Extracts the resolution from a template object.
 * @param {object} template - Template object with width/height or scene dimensions.
 * @returns {{ width: number, height: number, label: string, pixels: number }|null}
 *   Resolution info, or null if dimensions are invalid.
 */
export function getTemplateResolution(template) {
  const width = Number(template.width ?? template.scene?.width)
  const height = Number(template.height ?? template.scene?.height)

  if (!Number.isFinite(width) || !Number.isFinite(height) || width <= 0 || height <= 0) {
    return null
  }

  return {
    width,
    height,
    label: `${width} x ${height}`,
    pixels: width * height,
  }
}

/**
 * Groups templates by their resolution label and sorts groups by pixel count descending.
 * Templates within each group are sorted alphabetically by name.
 * @param {object[]} templates - Array of template objects.
 * @returns {object[]} Grouped and sorted template groups, each with a `templates` array.
 */
export function getTemplateGroups(templates) {
  const groupsByKey = new Map()

  templates.forEach((template) => {
    const resolution = getTemplateResolution(template)
    const key = resolution ? resolution.label : 'Unknown Resolution'
    const group = groupsByKey.get(key) || {
      key,
      label: key,
      pixels: resolution?.pixels ?? -1,
      width: resolution?.width ?? -1,
      height: resolution?.height ?? -1,
      templates: [],
    }

    group.templates.push(template)
    groupsByKey.set(key, group)
  })

  return [...groupsByKey.values()]
    .map((group) => ({
      ...group,
      templates: group.templates.sort((left, right) =>
        left.name.localeCompare(right.name, undefined, {
          sensitivity: 'base',
        }),
      ),
    }))
    .sort((left, right) => {
      if (right.pixels !== left.pixels) return right.pixels - left.pixels
      if (right.width !== left.width) return right.width - left.width
      if (right.height !== left.height) return right.height - left.height
      return left.label.localeCompare(right.label, undefined, {
        sensitivity: 'base',
      })
    })
}
