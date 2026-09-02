import { useEffect, useRef, useState } from 'react'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { hasTauriRuntime } from '@/api/backend'

function isPositionInsideElement(element, x, y) {
  if (!element) return false
  const bounds = element.getBoundingClientRect()
  return x >= bounds.left && x <= bounds.right && y >= bounds.top && y <= bounds.bottom
}

/**
 * Owns browser and native desktop drag/drop state for a generic file drop target.
 *
 * @param {(selections: Array<File|string>) => void} onDropFiles - Receives dropped browser files or native paths.
 * @returns {{dragPosition: {x: number, y: number}|null, dropZoneRef: React.RefObject, isDraggingFile: boolean, isOverDropZone: boolean, dropZoneProps: object}} Drop-zone presentation state and event props.
 */
export function useFileDropZone(onDropFiles) {
  const dropZoneRef = useRef(null)
  const [dragPosition, setDragPosition] = useState(null)
  const [isDraggingFile, setIsDraggingFile] = useState(false)
  const [isOverDropZone, setIsOverDropZone] = useState(false)

  useEffect(() => {
    const handleWindowDragOver = (event) => {
      if (!event.dataTransfer.types.includes('Files')) return
      event.preventDefault()
      event.dataTransfer.dropEffect = 'copy'
      setDragPosition({ x: event.clientX, y: event.clientY })
      setIsDraggingFile(true)
      setIsOverDropZone(isPositionInsideElement(dropZoneRef.current, event.clientX, event.clientY))
    }

    const handleWindowDragLeave = (event) => {
      if (event.relatedTarget !== null) return
      setDragPosition(null)
      setIsDraggingFile(false)
      setIsOverDropZone(false)
    }

    const handleWindowDrop = (event) => {
      if (!event.dataTransfer.types.includes('Files')) return
      event.preventDefault()
      setDragPosition(null)
      setIsDraggingFile(false)
      setIsOverDropZone(false)
    }

    window.addEventListener('dragover', handleWindowDragOver)
    window.addEventListener('dragleave', handleWindowDragLeave)
    window.addEventListener('drop', handleWindowDrop)
    return () => {
      window.removeEventListener('dragover', handleWindowDragOver)
      window.removeEventListener('dragleave', handleWindowDragLeave)
      window.removeEventListener('drop', handleWindowDrop)
    }
  }, [])

  useEffect(() => {
    if (!hasTauriRuntime()) return undefined

    let disposed = false
    let unlisten = null
    getCurrentWindow()
      .onDragDropEvent((event) => {
        const { payload } = event
        if (payload.type === 'leave') {
          setDragPosition(null)
          setIsDraggingFile(false)
          setIsOverDropZone(false)
          return
        }

        const position = { x: payload.position.x / window.devicePixelRatio, y: payload.position.y / window.devicePixelRatio }
        const overDropZone = isPositionInsideElement(dropZoneRef.current, position.x, position.y)
        if (payload.type === 'drop') {
          setDragPosition(null)
          setIsDraggingFile(false)
          setIsOverDropZone(false)
          if (overDropZone) onDropFiles(payload.paths)
          return
        }

        setDragPosition(position)
        setIsDraggingFile(true)
        setIsOverDropZone(overDropZone)
      })
      .then((stopListening) => {
        if (disposed) stopListening()
        else unlisten = stopListening
      })
      .catch((error) => {
        console.error('Failed to register native file drop zone:', error)
      })

    return () => {
      disposed = true
      if (unlisten) unlisten()
    }
  }, [onDropFiles])

  const handleDragOver = (event) => {
    if (!event.dataTransfer.types.includes('Files')) return
    event.preventDefault()
    event.dataTransfer.dropEffect = 'copy'
    setDragPosition({ x: event.clientX, y: event.clientY })
    setIsDraggingFile(true)
    setIsOverDropZone(true)
  }

  const handleDragLeave = (event) => {
    if (!event.currentTarget.contains(event.relatedTarget)) {
      setIsOverDropZone(false)
    }
  }

  const handleDrop = (event) => {
    event.preventDefault()
    setDragPosition(null)
    setIsDraggingFile(false)
    setIsOverDropZone(false)
    onDropFiles(Array.from(event.dataTransfer.files))
  }

  return {
    dragPosition,
    dropZoneRef,
    isDraggingFile,
    isOverDropZone,
    dropZoneProps: {
      onDragEnter: handleDragOver,
      onDragLeave: handleDragLeave,
      onDragOver: handleDragOver,
      onDrop: handleDrop,
    },
  }
}
