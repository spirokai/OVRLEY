import { probeVideo } from '../api/backend'

/**
 * Extracts metadata from a video file path.
 *
 * @param {string} filePath Absolute path to the video
 * @returns {Promise<object>} Extracted metadata (duration, fps, resolution, creationTime)
 */
export async function extractVideoMetadata(filePath) {
  try {
    const metadata = await probeVideo(filePath)
    return metadata
  } catch (error) {
    console.error('Failed to probe video metadata:', error)
    throw error
  }
}
