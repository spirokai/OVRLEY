/**
 * Orchestrates template lifecycle: create, save, import, switch, and dirty tracking.
 * Container hook — composes sub-hooks and exposes template actions.
 */

import { useCallback, useRef, useState } from 'react'
import * as backend from '@/api/backend'
import { hasTauriRuntime, useUnsavedChangesConfirm } from '@/features/app-shell'
import { fileFromSelectedPath, openSinglePath } from '@/lib/file-dialog'
import { deletePreference, getPreference, setPreference } from '@/lib/preferences-store'
import { useTemplateStore } from '@/hooks/useAppStoreSelectors'
import { replaceEditorDocument } from '@/features/undo-redo/undoHistory'
import useStore from '@/store/useStore'
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
import i18next from 'i18next'

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
    config,
    createNewTemplate,
    globalDefaults,
    hydrateTemplateState,
    lastSavedTemplateState,
    loadedTemplateSource,
    setErrorMessage,
    setProcessing,
    setLastSavedTemplateState,
    setLoadedTemplateSource,
    templates,
  } = useTemplateStore()

  const { fetchTemplates } = useTemplateFetching()
  const { answerConfirm, isOpen: isNewTemplateConfirmOpen, requestConfirm } = useUnsavedChangesConfirm()

  const [templateSelectorOpen, setTemplateSelectorOpen] = useState(false)

  const openTemplateSelector = useCallback(() => {
    setTemplateSelectorOpen(true)
  }, [])

  // Derived state — template save status computed from current editor state vs last saved snapshot
  const { currentTemplateState, status, showTemplateStatus } = useTemplateSaveStatus({
    config,
    globalDefaults,
    lastSavedTemplateState,
  })

  const loadTemplateState = useCallback(
    (templateState, source) => {
      replaceEditorDocument(useStore, () => {
        hydrateTemplateState(templateState, { source })
        setLastSavedTemplateState(templateState)
      })
    },
    [hydrateTemplateState, setLastSavedTemplateState],
  )

  // Template change handler — loads a template from the backend by filename
  const handleTemplateChange = useCallback(
    async (filename) => {
      if (!filename) return

      try {
        setProcessing(true)
        let descriptor = useStore.getState().templates.find((template) => template.id === filename)
        if (!descriptor) {
          await fetchTemplates()
          descriptor = useStore.getState().templates.find((template) => template.id === filename)
        }
        if (!descriptor) throw new Error(`Unknown template: ${filename}`)
        const data = await backend.getTemplate(filename)
        const normalizedTemplate = normalizeTemplateFilePayload(data, {
          globalDefaults,
        })
        const { name: _templateName, ...templateState } = normalizedTemplate

        loadTemplateState(
          templateState,
          descriptor.type === 'built-in' ? { kind: 'bundled', templateId: filename } : { kind: 'file', path: descriptor.path },
        )
        await setPreference('last-template', { source: 'backend', filename })
        return true
      } catch (error) {
        console.error('Failed to load template:', error)
        setErrorMessage(`Failed to load template: ${getErrorMessage(error, 'Unknown error')}`)
        return false
      } finally {
        setProcessing(false)
      }
    },
    [fetchTemplates, globalDefaults, loadTemplateState, setErrorMessage, setProcessing],
  )

  // Save template handler — serializes current state and triggers save dialog or download
  const handleSaveTemplate = useCallback(async () => {
    const sourceName = loadedTemplateSource?.kind === 'bundled' ? loadedTemplateSource.templateId : loadedTemplateSource?.path
    const suggestedFilename = sanitizeTemplateFilename(getFilenameFromTemplateId(getFilenameFromPath(sourceName)) || 'my_template')

    try {
      const payload = createTemplateFilePayload(
        {
          config,
          globalDefaults,
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
          title: i18next.t('template-manager.saveTemplate', 'Save Template'),
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

        await fetchTemplates()
        setLoadedTemplateSource({ kind: 'file', path: String(selectedPath) })
        setLastSavedTemplateState(currentTemplateState)
        await setPreference('last-template', { source: 'file', path: String(selectedPath) })
        return { kind: 'file', path: String(selectedPath) }
      }

      downloadTemplateFile(payload, suggestedFilename)
      setLoadedTemplateSource(null)
      setLastSavedTemplateState(currentTemplateState)
      return null
    } catch (error) {
      console.error('Failed to save template:', error)
      setErrorMessage(`Failed to save template: ${getErrorMessage(error, 'Unknown error')}`)
    }
  }, [
    config,
    currentTemplateState,
    fetchTemplates,
    globalDefaults,
    loadedTemplateSource,
    setErrorMessage,
    setLastSavedTemplateState,
    setLoadedTemplateSource,
  ])

  // Import template handler — opens file picker and hydrates state from a JSON file
  const handleImportTemplate = useCallback(async () => {
    try {
      let file
      let selectedPath = null

      if (hasTauriRuntime()) {
        selectedPath = await openSinglePath([{ name: 'OVRLEY Template', extensions: ['json'] }])
        if (!selectedPath) return
        file = await fileFromSelectedPath(selectedPath)
      } else {
        file = await selectBrowserTemplateFile()
      }

      if (!file) return

      const rawText = await file.text()
      const parsedTemplate = JSON.parse(rawText)
      const normalizedTemplate = normalizeTemplateFilePayload(parsedTemplate, {
        globalDefaults,
      })
      const { name: _templateName, ...templateState } = normalizedTemplate
      loadTemplateState(templateState, selectedPath ? { kind: 'file', path: selectedPath } : null)
    } catch (error) {
      console.error('Failed to import template:', error)
      setErrorMessage(`Failed to import template: ${getErrorMessage(error, 'Unknown error')}`)
    }
  }, [globalDefaults, loadTemplateState, setErrorMessage])

  // Confirm create new — executes the new template action after the confirmation is answered
  const confirmCreateNewTemplate = useCallback(() => {
    replaceEditorDocument(useStore, createNewTemplate)
    deletePreference('last-template')
    onTemplateCreated()
  }, [createNewTemplate, onTemplateCreated])

  // Create new template — asks to save unsaved changes before discarding them
  const handleCreateNewTemplate = useCallback(async () => {
    if (status !== 'Draft' && status !== 'Modified') {
      confirmCreateNewTemplate()
      return
    }

    const action = await requestConfirm()
    if (action === 'cancel') return
    if (action === 'save') {
      const saved = await handleSaveTemplate()
      if (saved === undefined) return
    }
    confirmCreateNewTemplate()
  }, [confirmCreateNewTemplate, handleSaveTemplate, requestConfirm, status])

  const newTemplateConfirmDialog = {
    open: isNewTemplateConfirmOpen,
    title: i18next.t('template-manager.createNewTemplate', 'Create New Template'),
    description: i18next.t('template-manager.unsavedChanges', 'Your template has unsaved changes. Save them or discard them.'),
    cancelLabel: i18next.t('template-manager.cancel', 'Cancel'),
    saveLabel: i18next.t('template-manager.save', 'Save'),
    discardLabel: i18next.t('template-manager.newTemplate', 'New Template'),
    onCancel: () => answerConfirm('cancel'),
    onSave: () => answerConfirm('save'),
    onDiscard: () => answerConfirm('discard'),
  }

  // Always point to the latest handleTemplateChange so restoreLastLoadedTemplate
  // can invoke the current handler without depending on it.
  const handleTemplateChangeRef = useRef(handleTemplateChange)
  handleTemplateChangeRef.current = handleTemplateChange

  const restoreLastLoadedTemplate = useCallback(async () => {
    try {
      const saved = await getPreference('last-template')
      if (saved?.source === 'backend' && saved.filename) {
        const restored = await handleTemplateChangeRef.current(saved.filename)
        if (!restored) {
          replaceEditorDocument(useStore, createNewTemplate)
          await deletePreference('last-template')
        }
      } else if (saved?.source === 'file' && saved.path) {
        const file = await fileFromSelectedPath(saved.path)
        const normalizedTemplate = normalizeTemplateFilePayload(JSON.parse(await file.text()))
        const { name: _templateName, ...templateState } = normalizedTemplate
        loadTemplateState(templateState, { kind: 'file', path: saved.path })
      }
    } catch (error) {
      console.error('Failed to restore last template:', error)
      setErrorMessage(`Failed to load template: ${getErrorMessage(error, 'Unknown error')}`)
      replaceEditorDocument(useStore, createNewTemplate)
      await deletePreference('last-template')
    }
  }, [createNewTemplate, loadTemplateState, setErrorMessage])

  return {
    handleCreateNewTemplate,
    handleImportTemplate,
    handleSaveTemplate,
    handleTemplateChange,
    loadedTemplateSource,
    loadTemplateState,
    newTemplateConfirmDialog,
    openTemplateSelector,
    restoreLastLoadedTemplate,
    setTemplateSelectorOpen,
    showTemplateStatus,
    status,
    templateSelectorOpen,
    templates,
  }
}
