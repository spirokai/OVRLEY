/**
 * Orders camera chapters by filename, with numeric chapter numbers in natural order.
 * @param {string[]} paths Native file paths.
 * @returns {string[]} Validated, ordered MP4 paths.
 */
export function orderVideoParts(paths) {
  if (!Array.isArray(paths) || paths.length < 2) throw new Error('Select at least two MP4 parts.')
  if (paths.some((path) => typeof path !== 'string' || !/\.mp4$/i.test(path))) {
    throw new Error('Multiple-file import requires native MP4 paths. Select the files using Browse.')
  }
  if (new Set(paths.map((path) => path.toLowerCase())).size !== paths.length) throw new Error('The same video part was selected twice.')
  const collator = new Intl.Collator('en', { numeric: true, sensitivity: 'base' })
  return [...paths].sort((left, right) => collator.compare(left.split(/[/\\]/).pop(), right.split(/[/\\]/).pop()) || collator.compare(left, right))
}

/** @param {object[]} parts Native sequence metadata. @param {number} duration Group duration. @returns {object[]} Validated parts. */
export function validateVideoParts(parts, duration) {
  if (!Array.isArray(parts) || parts.length < 2) throw new Error('Video group must contain at least two parts.')
  let end = 0
  for (const part of parts) {
    if (typeof part.path !== 'string' || !part.path || !Number.isFinite(part.duration) || part.duration <= 0 || part.startSeconds !== end) {
      throw new Error('Invalid video group part or non-contiguous timing.')
    }
    end += part.duration
  }
  if (Math.abs(end - duration) > 0.000001) throw new Error('Video group duration does not match its parts.')
  return parts
}

/** @param {object[]} parts Validated parts. @param {number} second Group-local time. @returns {object} Active part, clamped outside the group. */
export function videoPartAtTime(parts, second) {
  return parts.find((part) => second < part.startSeconds + part.duration) ?? parts[parts.length - 1]
}
