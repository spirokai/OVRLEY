/**
 * Orchestrates template lifecycle: create, save, import, switch, and dirty tracking.
 * Container hook — composes sub-hooks and exposes template actions.
 */

import { useCallback, useEffect, useState } from 'react'
import * as backend from '@/api/backend'
import { hasTauriRuntime } from '@/features/app-shell'
import { useTemplateStore } from '@/hooks/useAppStoreSelectors'
import useTemplateFetching from './useTemplateFetching'
import {
  createTemplateFilePayload,
  downloadTemplateFile,
  normalizeTemplateFilePayload,
  sanitizeTemplateFilename,
  stringifyTemplateFile,
} from '../utils/templateSnapshot'
import { useTemplateSaveStatus } from './useTemplateSaveStatus'
import { selectBrowserTemplateFile, getFilenameFromPath, getFilenameFromTemplateId } from '../utils/templateFileUtils'

function getErrorMessage(error, fallbackMessage) {
  if (error instanceof Error && error.message) {
    return error.message
  }

  if (typeof error === 'string' && error.trim()) {
    return error
  }

  if (error && typeof error === 'object' && typeof error.message === 'string' && error.message.trim()) {
    return error.message
  }

  return fallbackMessage
}

/**
 * Provides template management state and actions.
 *
 * @param {object} options - Structured options for the helper.
 * @param {*} options.onTemplateCreated - Callback invoked to template created.
 * @returns {object} Result produced by the helper.
 */
export default function useTemplateManagement({ onTemplateCreated }) {
  // Store selectors — template config, state, and actions from the Zustand template slice
  const {
    aspectRatio,
    config,
    createNewTemplate,
    exportCodec,
    exportRange,
    globalDefaults,
    hydrateTemplateState,
    lastSavedTemplateState,
    loadedTemplateFilename,
    loadedTemplateSource,
    setErrorMessage,
    setProcessing,
    setLastSavedTemplateState,
    setLoadedTemplate,
    templates,
    updateRate,
  } = useTemplateStore()

  const { fetchTemplates } = useTemplateFetching()

  // Local UI state — manages the new-template confirmation dialog visibility
  const [showNewTemplateConfirm, setShowNewTemplateConfirm] = useState(false)

  // Derived state — template save status computed from current editor state vs last saved snapshot
  const { currentTemplateState, status, showTemplateStatus } = useTemplateSaveStatus({
    config,
    globalDefaults,
    updateRate,
    exportRange,
    exportCodec,
    aspectRatio,
    lastSavedTemplateState,
  })

  // Side effects — closes the new-template confirmation dialog on Escape key press
  useEffect(() => {
    if (!showNewTemplateConfirm || typeof window === 'undefined') {
      return undefined
    }

    const handleKeyDown = (event) => {
      if (event.key === 'Escape') {
        setShowNewTemplateConfirm(false)
      }
    }

    window.addEventListener('keydown', handleKeyDown)
    return () => {
      window.removeEventListener('keydown', handleKeyDown)
    }
  }, [showNewTemplateConfirm])

  // Template change handler — loads a template from the backend by filename
  const handleTemplateChange = useCallback(
    async (filename) => {
      if (!filename) return

      try {
        setProcessing(true)
        const data = await backend.getTemplate(filename)
        const normalizedTemplate = normalizeTemplateFilePayload(data, {
          globalDefaults,
          updateRate,
          exportRange,
          exportCodec,
          aspectRatio,
        })
        const { name: _templateName, ...templateState } = normalizedTemplate

        hydrateTemplateState(templateState, {
          filename,
          source: 'backend',
        })
        setLastSavedTemplateState(templateState)
      } catch (error) {
        console.error('Failed to load template:', error)
        setErrorMessage(`Failed to load template: ${getErrorMessage(error, 'Unknown error')}`)
      } finally {
        setProcessing(false)
      }
    },
    [
      aspectRatio,
      exportCodec,
      exportRange,
      globalDefaults,
      hydrateTemplateState,
      setErrorMessage,
      setProcessing,
      setLastSavedTemplateState,
      updateRate,
    ],
  )

  // Save template handler — serializes current state and triggers save dialog or download
  const handleSaveTemplate = useCallback(async () => {
    const suggestedFilename = sanitizeTemplateFilename(getFilenameFromTemplateId(loadedTemplateFilename) || 'my_template')

    try {
      const payload = createTemplateFilePayload(
        {
          config,
          globalDefaults,
          updateRate,
          exportRange,
          exportCodec,
          aspectRatio,
        },
        {
          name: suggestedFilename.replace(/\.json$/i, ''),
        },
      )
      const templateContents = stringifyTemplateFile(payload)

      if (hasTauriRuntime()) {
        const { save } = await import('@tauri-apps/plugin-dialog')
        const defaultPath = await backend.getDefaultTemplateSavePath(suggestedFilename)
        const selectedPath = await save({
          title: 'Save Template',
          defaultPath,
          filters: [
            {
              name: 'OVRLEY Template',
              extensions: ['json'],
            },
          ],
        })

        if (!selectedPath) return

        await backend.writeTemplateFile(selectedPath, templateContents)

        const savedFilename = getFilenameFromPath(selectedPath)
        const defaultTemplateDir = defaultPath.replace(/[\\/][^\\/]*$/, '')
        const selectedTemplateDir = String(selectedPath).replace(/[\\/][^\\/]*$/, '')
        const savedInDefaultTemplateDir = defaultTemplateDir.toLowerCase() === selectedTemplateDir.toLowerCase()

        await fetchTemplates()
        setLoadedTemplate(savedInDefaultTemplateDir ? `user:${savedFilename}` : savedFilename, savedInDefaultTemplateDir ? 'backend' : 'file')
        setLastSavedTemplateState(currentTemplateState)
        return
      }

      downloadTemplateFile(payload, suggestedFilename)
      setLoadedTemplate(suggestedFilename, 'file')
      setLastSavedTemplateState(currentTemplateState)
    } catch (error) {
      console.error('Failed to save template:', error)
      setErrorMessage(`Failed to save template: ${getErrorMessage(error, 'Unknown error')}`)
    }
  }, [
    aspectRatio,
    config,
    currentTemplateState,
    exportCodec,
    exportRange,
    fetchTemplates,
    globalDefaults,
    loadedTemplateFilename,
    setErrorMessage,
    setLastSavedTemplateState,
    setLoadedTemplate,
    updateRate,
  ])

  // Import template handler — opens browser file picker and hydrates state from a JSON file
  const handleImportTemplate = useCallback(async () => {
    try {
      if (hasTauriRuntime()) {
        await backend.openTemplates()
        return
      }

      const file = await selectBrowserTemplateFile()
      if (!file) return

      const rawText = await file.text()
      const parsedTemplate = JSON.parse(rawText)
      const normalizedTemplate = normalizeTemplateFilePayload(parsedTemplate, {
        globalDefaults,
        updateRate,
        exportRange,
        exportCodec,
        aspectRatio,
      })
      const { name: _templateName, ...templateState } = normalizedTemplate
      const importedFilename = sanitizeTemplateFilename(normalizedTemplate.name || file.name)

      hydrateTemplateState(templateState, {
        filename: importedFilename,
        source: 'file',
      })
      setLastSavedTemplateState(templateState)
    } catch (error) {
      console.error('Failed to import template:', error)
      setErrorMessage(`Failed to import template: ${getErrorMessage(error, 'Unknown error')}`)
    }
  }, [aspectRatio, exportCodec, exportRange, globalDefaults, hydrateTemplateState, setErrorMessage, setLastSavedTemplateState, updateRate])

  // Confirm create new — executes the new template action and closes confirmation
  const confirmCreateNewTemplate = useCallback(() => {
    createNewTemplate()
    onTemplateCreated()
    setShowNewTemplateConfirm(false)
  }, [createNewTemplate, onTemplateCreated])

  // Create new template — shows confirmation dialog if there are unsaved changes
  const handleCreateNewTemplate = useCallback(() => {
    const hasUnsavedChanges = status === 'Draft' || status === 'Modified'
    if (hasUnsavedChanges) {
      setShowNewTemplateConfirm(true)
      return
    }

    confirmCreateNewTemplate()
  }, [confirmCreateNewTemplate, status])

  return {
    confirmCreateNewTemplate,
    handleCreateNewTemplate,
    handleImportTemplate,
    handleSaveTemplate,
    handleTemplateChange,
    loadedTemplateFilename,
    loadedTemplateSource,
    setShowNewTemplateConfirm,
    showNewTemplateConfirm,
    showTemplateStatus,
    status,
    templates,
  }
}
