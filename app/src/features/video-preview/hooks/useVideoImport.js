/**
 * Video import — Tauri dialog-based video file selection and preview management.
 * Handles importing video files for overlay preview and clearing the imported video.
 */

import { open } from '@tauri-apps/plugin-dialog'
import { clearPreviewVideo, importPreviewVideo } from '@/api/backend'
import useStore from '@/store/useStore'

/**
 * Container hook for video import functionality.
 * Provides the filename, import handler, and clear handler for the
 * Tauri-native video import flow.
 *
 * @returns {{
 *   importedVideoFilename: string|null,
 *   handleImportVideo: Function,
 *   clearImportedVideo: Function,
 * }}
 */
export default function useVideoImport() {
  // Store selectors — individual selectors to avoid unnecessary re-renders
  const importedVideoPath = useStore((state) => state.importedVideoPath)
  const setImportedVideo = useStore((state) => state.setImportedVideo)
  const clearImportedVideo = useStore((state) => state.clearImportedVideo)
  const config = useStore((state) => state.config)
  const setConfig = useStore((state) => state.setConfig)

  // Derived state — extract filename from the full path
  const importedVideoFilename = importedVideoPath ? importedVideoPath.split(/[/\\]/).pop() : null

  // Video import handler — opens Tauri file dialog, imports preview, syncs scene FPS
  const handleImportVideo = async () => {
    try {
      const selected = await open({
        multiple: false,
        filters: [
          {
            name: 'Video',
            extensions: ['mp4', 'mov', 'mkv'],
          },
        ],
      })
      if (selected) {
        const response = await importPreviewVideo(selected)
        const metadata = {
          ...response.metadata,
          importId: response.importId,
          previewUrl: response.previewUrl,
          previewWarnings: response.warnings ?? [],
          previewError: null,
        }
        setImportedVideo(metadata)
        if (metadata.fps && config?.scene) {
          setConfig({
            ...config,
            scene: { ...config.scene, fps: Math.round(metadata.fps) },
          })
        }
        // Phase 2: computeVideoSync will be called here
      }
    } catch (err) {
      console.error('Failed to import video:', err)
    }
  }

  // Clear handler — removes the preview video from the backend and resets store state
  const handleClearImportedVideo = async () => {
    try {
      await clearPreviewVideo()
    } catch (err) {
      console.error('Failed to clear preview video:', err)
    } finally {
      clearImportedVideo()
    }
  }

  return {
    importedVideoFilename,
    handleImportVideo,
    clearImportedVideo: handleClearImportedVideo,
  }
}
