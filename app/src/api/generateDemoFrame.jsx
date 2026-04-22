import { getCurrentParsedActivity } from './activityCache'
import useStore from '../store/useStore'
import * as backend from './backend'

// Track if a request is in progress to prevent duplicate calls
let isGenerating = false

export default async function generateDemoFrame(config) {
  try {
    const {
      setImageFilename,
      setGeneratingImage,
      config: storeConfig,
      selectedSecond,
    } = useStore.getState()

    const configToSend = config ?? storeConfig

    const parsedActivity = getCurrentParsedActivity()

    // Validate we have required data
    if (!configToSend || !configToSend.scene) {
      console.error('No valid config available')
      return
    }

    if (!parsedActivity) {
      throw new Error('No parsed activity available for preview')
    }

    // If already generating, skip this request
    if (isGenerating) {
      console.log('Demo generation already in progress, skipping')
      return
    }

    isGenerating = true
    setGeneratingImage(true)

    console.log('📤 Sending demo request:', {
      second: selectedSecond,
      start: configToSend?.scene?.start,
      end: configToSend?.scene?.end,
    })

    const data = await backend.generateDemo(
      configToSend,
      parsedActivity,
      selectedSecond,
    )

    // Handle 429 (Too Many Requests / Busy) gracefully
    if (data.error_code === 'BUSY') {
      console.log(
        '⏳ Backend is busy generating another frame, skipping this request',
      )
      return
    }

    // Check if response contains an error
    if (data.error) {
      console.error('❌ Backend returned error:', data)
      throw new Error(data.error)
    }

    const demoImageFilename = data.filename
    if (demoImageFilename) {
      const imageUrl = await backend.getImageUrl(demoImageFilename)
      setImageFilename(imageUrl)
      console.log('✅ Image filename set:', demoImageFilename)

      // Clear any previous errors on success
      const { clearError } = useStore.getState()
      clearError()
    }
  } catch (error) {
    // Don't log abort errors as they're intentional
    if (error.name !== 'AbortError') {
      console.error('Error generating demo frame:', error)

      const { setErrorMessage } = useStore.getState()
      setErrorMessage(error.message || 'Failed to generate preview')

      // Don't re-throw - we've handled it by setting the error state
    }
  } finally {
    isGenerating = false
    const { setGeneratingImage } = useStore.getState()
    setGeneratingImage(false)
  }
}
