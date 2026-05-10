/**
 * Implements the use Template Management hook and related behavior for the app.
 */

import { useCallback, useEffect, useMemo, useState } from 'react'
import * as backend from '@/api/backend'
import { hasTauriRuntime } from '@/hooks/useBackendStatus'
import { useTemplateStore } from '@/hooks/useAppStoreSelectors'
import {
  createTemplateFilePayload,
  createTemplateState,
  downloadTemplateFile,
  normalizeTemplateFilePayload,
  sanitizeTemplateFilename,
  stringifyTemplateFile,
  templateStatesEqual,
} from '@/lib/template-snapshot'

/**
 * Handles select browser template file.
 * @returns {*} Result produced by the helper.
 */
const selectBrowserTemplateFile = () =>
  new Promise((resolve) => {
    const input = document.createElement('input')
    input.type = 'file'
    input.accept = '.json,application/json'
    input.onchange = () => resolve(input.files?.[0] ?? null)
    input.oncancel = () => resolve(null)
    input.click()
  })

/**
 * Returns filename from path.
 *
 * @param {*} path - Filesystem path for the target resource.
 * @returns {*} Requested value or structure.
 */
const getFilenameFromPath = (path) => {
  const segments = String(path || '').split(/[/\\]/)
  return segments[segments.length - 1] || 'ovrley_template.json'
}

/**
 * Provides template management state and actions.
 *
 * @param {object} options - Structured options for the helper.
 * @param {*} options.onTemplateCreated - Callback invoked to template created.
 * @returns {object} Result produced by the helper.
 */
export default function useTemplateManagement({ onTemplateCreated }) {
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
    setGeneratingImage,
    setLastSavedTemplateState,
    setLoadedTemplate,
    templates,
    updateRate,
  } = useTemplateStore()
  const [showNewTemplateConfirm, setShowNewTemplateConfirm] = useState(false)

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

  const currentTemplateState = useMemo(
    () =>
      createTemplateState({
        config,
        globalDefaults,
        updateRate,
        exportRange,
        exportCodec,
        aspectRatio,
      }),
    [config, globalDefaults, updateRate, exportRange, exportCodec, aspectRatio],
  )

  const status = useMemo(() => {
    if (!config) {
      return null
    }

    if (!lastSavedTemplateState) {
      return 'Draft'
    }

    return templateStatesEqual(currentTemplateState, lastSavedTemplateState)
      ? 'Saved'
      : 'Modified'
  }, [config, currentTemplateState, lastSavedTemplateState])

  const showTemplateStatus = status === 'Draft' || status === 'Modified'

  const handleTemplateChange = useCallback(
    async (filename) => {
      if (!filename) return

      try {
        setGeneratingImage(true)
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
        setErrorMessage(`Failed to load template: ${error.message}`)
      } finally {
        setGeneratingImage(false)
      }
    },
    [
      aspectRatio,
      exportCodec,
      exportRange,
      globalDefaults,
      hydrateTemplateState,
      setErrorMessage,
      setGeneratingImage,
      setLastSavedTemplateState,
      updateRate,
    ],
  )

  const handleSaveTemplate = useCallback(async () => {
    const suggestedFilename = sanitizeTemplateFilename(
      loadedTemplateFilename || 'my_template',
    )

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
        const defaultPath =
          await backend.getDefaultTemplateSavePath(suggestedFilename)
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

        const savedFilename = sanitizeTemplateFilename(
          getFilenameFromPath(selectedPath),
        )
        setLoadedTemplate(savedFilename, 'file')
        setLastSavedTemplateState(currentTemplateState)
        return
      }

      downloadTemplateFile(payload, suggestedFilename)
      setLoadedTemplate(suggestedFilename, 'file')
      setLastSavedTemplateState(currentTemplateState)
    } catch (error) {
      console.error('Failed to save template:', error)
      setErrorMessage(`Failed to save template: ${error.message}`)
    }
  }, [
    aspectRatio,
    config,
    currentTemplateState,
    exportCodec,
    exportRange,
    globalDefaults,
    loadedTemplateFilename,
    setErrorMessage,
    setLastSavedTemplateState,
    setLoadedTemplate,
    updateRate,
  ])

  const handleImportTemplate = useCallback(async () => {
    try {
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
      const importedFilename = sanitizeTemplateFilename(
        normalizedTemplate.name || file.name,
      )

      hydrateTemplateState(templateState, {
        filename: importedFilename,
        source: 'file',
      })
      setLastSavedTemplateState(templateState)
    } catch (error) {
      console.error('Failed to import template:', error)
      setErrorMessage(`Failed to import template: ${error.message}`)
    }
  }, [
    aspectRatio,
    exportCodec,
    exportRange,
    globalDefaults,
    hydrateTemplateState,
    setErrorMessage,
    setLastSavedTemplateState,
    updateRate,
  ])

  const confirmCreateNewTemplate = useCallback(() => {
    createNewTemplate()
    onTemplateCreated()
    setShowNewTemplateConfirm(false)
  }, [createNewTemplate, onTemplateCreated])

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
