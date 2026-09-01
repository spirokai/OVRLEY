/**
 * Converts English copy to the canonical camelCase translation-key suffix.
 * @param {string} content User-facing English copy.
 * @returns {string|null} Translation-key suffix, or null when the copy contains no keyable characters.
 */
export function createTranslationKeySuffix(content) {
  const words = content
    .replace(/[^\w\s\d]/g, '')
    .trim()
    .split(/\s+/)

  if (words.length === 0 || words[0] === '') {
    return null
  }

  return words.map((word, index) => (index === 0 ? word.toLowerCase() : word.charAt(0).toUpperCase() + word.slice(1).toLowerCase())).join('')
}

/**
 * Gets the canonical translation-key prefix from a source path.
 * Features are grouped by feature name; other files are grouped by their
 * top-level source folder. Root source files use their filename.
 * @param {string} filePath Source file path.
 * @returns {string} Source owner name.
 */
export function getTranslationPrefix(filePath) {
  const pathSegments = filePath.replaceAll('\\', '/').split('/')
  const srcIndex = pathSegments.lastIndexOf('src')
  const featuresIndex = pathSegments.indexOf('features')

  if (featuresIndex !== -1 && pathSegments[featuresIndex + 1]) {
    return pathSegments[featuresIndex + 1]
  }

  const sourceOwner = pathSegments[srcIndex + 1]

  if (srcIndex === -1 || !sourceOwner) {
    throw new Error(`Translation source is not inside src: ${filePath}`)
  }

  return sourceOwner.replace(/\.[^.]+$/, '').toLowerCase()
}

/**
 * Creates a canonical source-prefixed translation key.
 * @param {string} filePath Source file path.
 * @param {string} content User-facing English copy.
 * @returns {string} Dotted translation key.
 */
export function createTranslationKey(filePath, content) {
  const suffix = createTranslationKeySuffix(content)

  if (!suffix) {
    throw new Error(`Cannot generate a translation key from: ${content}`)
  }

  return `${getTranslationPrefix(filePath)}.${suffix}`
}

export const sourceKeyInstrumentationPlugin = {
  name: 'source-key-instrumentation',
  instrumentOnResult: (filePath, candidates) =>
    candidates.map((candidate) => {
      const suffix = createTranslationKeySuffix(candidate.content)

      if (!suffix) {
        return { ...candidate, skipReason: 'Copy contains no keyable characters' }
      }

      return {
        ...candidate,
        key: `${getTranslationPrefix(filePath)}.${suffix}`,
      }
    }),
}
