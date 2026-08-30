import { directoryFromSelectedPath } from '@/lib/utils'

function splitPath(path) {
  return String(path).replace(/\\/g, '/').split('/').filter(Boolean)
}

function isWindowsPath(path) {
  return /^[a-zA-Z]:[\\/]/.test(path)
}

/**
 * Creates the canonical locator for a native source relative to a project.
 * @param {string} sourcePath Absolute source path.
 * @param {string} projectPath Absolute `.oly` path.
 * @returns {{kind: 'project-relative'|'absolute', value: string}} Path locator.
 */
export function createPathLocator(sourcePath, projectPath) {
  if (!sourcePath || !projectPath) throw new Error('Source and project paths are required')
  const directory = directoryFromSelectedPath(projectPath)
  const sourceParts = splitPath(sourcePath)
  const directoryParts = splitPath(directory)
  const caseInsensitive = isWindowsPath(sourcePath) || isWindowsPath(projectPath)
  const matches = directoryParts.every((part, index) => {
    const sourcePart = sourceParts[index]
    return caseInsensitive ? sourcePart?.toLowerCase() === part.toLowerCase() : sourcePart === part
  })
  if (matches && sourceParts.length > directoryParts.length) {
    return { kind: 'project-relative', value: sourceParts.slice(directoryParts.length).join('/') }
  }
  return { kind: 'absolute', value: sourcePath }
}
