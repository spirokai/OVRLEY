/**
 * Renders the error alert portion of the application interface.
 */

import React from 'react'
import { createPortal } from 'react-dom'
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { AlertCircle, Check, Copy, X } from 'lucide-react'
import { Button } from '@/components/ui/button'
import useCopyToClipboard from '../hooks/useCopyToClipboard'
import { translateBackendError } from '@/i18n'
import useStore from '@/store/useStore'
import { useShallow } from 'zustand/react/shallow'
import { useTranslation } from 'react-i18next'

/**
 * Renders the error alert component.
 * @returns {JSX.Element} Rendered component output.
 */
function ErrorAlert() {
  const { t } = useTranslation()
  const { clearError, errorMessage } = useStore(
    useShallow((state) => ({
      clearError: state.clearError,
      errorMessage: state.errorMessage,
    })),
  )

  const { copied, copy } = useCopyToClipboard()

  if (!errorMessage) {
    return null
  }

  const message = translateBackendError(errorMessage)

  // Portaled above every app layer: dialog backdrops (z-120..140) and Moveable's body-level control box (z-index 3000).
  return createPortal(
    <div className="fixed top-4 right-4 z-4000 max-w-md animate-in fade-in slide-in-from-top-4 duration-300">
      <Alert variant="destructive" className="relative gap-y-2 pl-5 pr-12 shadow-lg">
        <AlertTitle>
          <div className="flex items-center gap-2">
            <AlertCircle className="h-4 w-4 shrink-0" />
            <span className="truncate">{t('app-shell.errorRenderingVideo', 'Error Rendering Video')}</span>
            <Button
              variant="ghost"
              size="icon"
              className="h-6 w-6 shrink-0 hover:bg-destructive-foreground/10"
              onClick={() => copy(message)}
              aria-label={t('app-shell.copyError', 'Copy error')}
              title={t('app-shell.copyError', 'Copy error')}
            >
              {copied ? <Check className="h-3.5 w-3.5" /> : <Copy className="h-3.5 w-3.5" />}
            </Button>
          </div>
        </AlertTitle>
        <AlertDescription className="-mr-12 -mb-3 max-h-[60vh] min-w-0 overflow-y-auto pr-12 pb-3 text-sm opacity-90 select-text whitespace-pre-wrap wrap-anywhere thin-scrollbar">
          {message}
        </AlertDescription>
        <Button variant="ghost" size="icon" className="absolute top-2 right-2 h-8 w-8 hover:bg-destructive-foreground/10" onClick={clearError}>
          <X className="h-4 w-4" />
        </Button>
      </Alert>
    </div>,
    document.body,
  )
}

export default ErrorAlert
