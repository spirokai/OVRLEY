import { open } from '@tauri-apps/plugin-dialog'
import { clearPreviewVideo, importPreviewVideo } from '../api/backend'
import useStore from '../store/useStore'

export default function useVideoImport() {
  const importedVideoPath = useStore((state) => state.importedVideoPath)
  const setImportedVideo = useStore((state) => state.setImportedVideo)
  const clearImportedVideo = useStore((state) => state.clearImportedVideo)
  const config = useStore((state) => state.config)
  const setConfig = useStore((state) => state.setConfig)

  const importedVideoFilename = importedVideoPath ? importedVideoPath.split(/[/\\]/).pop() : null

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
