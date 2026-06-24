/**
 * Video import - background media selection and preview management.
 */

import { clearPreviewVideo, importPreviewVideo } from '@/api/backend'
import { openSinglePath } from '@/lib/file-dialog'
import useStore from '@/store/useStore'

const DEBUG_IMAGE_EXTENSIONS = new Set(['png', 'jpg', 'jpeg', 'webp'])

function pathExtension(path) {
  return typeof path === 'string' ? path.split('.').pop()?.toLowerCase() || '' : ''
}

export default function useVideoImport({ debugModeEnabled = false, onSetBackgroundMode }) {
  const importedVideoPath = useStore((state) => state.importedVideoPath)
  const importedBackgroundImagePath = useStore((state) => state.importedBackgroundImagePath)
  const setImportedVideo = useStore((state) => state.setImportedVideo)
  const setImportedBackgroundImage = useStore((state) => state.setImportedBackgroundImage)
  const clearImportedVideo = useStore((state) => state.clearImportedVideo)
  const setImportingVideo = useStore((state) => state.setImportingVideo)
  const config = useStore((state) => state.config)
  const setConfig = useStore((state) => state.setConfig)

  const importedVideoFilename = importedVideoPath ? importedVideoPath.split(/[/\\]/).pop() : null
  const importedBackgroundImageFilename = importedBackgroundImagePath ? importedBackgroundImagePath.split(/[/\\]/).pop() : null
  const importedMediaFilename = importedBackgroundImageFilename || importedVideoFilename

  const handleImportVideo = async () => {
    try {
      const selected = await openSinglePath(
        [
          {
            name: debugModeEnabled ? 'Video or Image' : 'Video',
            extensions: debugModeEnabled ? ['mp4', 'mov', 'mkv', 'png', 'jpg', 'jpeg', 'webp'] : ['mp4', 'mov', 'mkv'],
          },
        ],
        { lastDirectoryKey: 'last-video-import-dir' },
      )
      if (!selected) {
        return
      }

      setImportingVideo(true)

      if (debugModeEnabled && DEBUG_IMAGE_EXTENSIONS.has(pathExtension(selected))) {
        if (importedVideoPath) {
          await clearPreviewVideo()
        }
        setImportedBackgroundImage(selected)
        onSetBackgroundMode?.('image')
        return
      }

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
      onSetBackgroundMode?.('video')
    } catch (err) {
      console.error('Failed to import background media:', err)
    } finally {
      setImportingVideo(false)
    }
  }

  const handleClearImportedVideo = async () => {
    try {
      if (importedVideoPath) {
        await clearPreviewVideo()
      }
    } catch (err) {
      console.error('Failed to clear preview video:', err)
    } finally {
      clearImportedVideo()
      onSetBackgroundMode?.('checker')
    }
  }

  return {
    debugModeEnabled,
    importedBackgroundImageFilename,
    importedMediaFilename,
    importedVideoFilename,
    handleImportVideo,
    clearImportedVideo: handleClearImportedVideo,
  }
}
