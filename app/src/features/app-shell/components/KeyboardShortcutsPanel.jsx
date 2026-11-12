import { useMemo } from 'react'
import { Kbd } from '@/components/ui/kbd'
import { getKeyboardShortcutGroups } from '../utils/keyboardShortcutGroups'
import { useTranslation } from 'react-i18next'

/**
 * Renders the keyboard shortcut reference.
 *
 * @returns {JSX.Element} Rendered shortcuts panel.
 */
export default function KeyboardShortcutsPanel() {
  const { t } = useTranslation()
  const shortcutGroups = useMemo(() => getKeyboardShortcutGroups(t), [t])

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <p className="shrink-0 px-6 pt-4 pb-3 normal-case font-light text-[0.9rem]">
        {t('app-shell.keyboardShortcutsDescription', 'You can use the following keyboard shortcuts to improve your workflow within OVRLEY:')}
      </p>
      <div className="min-h-0 flex-1 space-y-8 overflow-y-auto border-t border-border/60 px-6 py-4 thin-scrollbar">
        {shortcutGroups.map((category) => (
          <div key={category.name} className="space-y-3">
            <h3 className="text-[1rem] font-extrabold uppercase text-primary">{category.name}</h3>
            <div className="space-y-2">
              {category.shortcuts.map((shortcut) => (
                <div key={`${category.name}-${shortcut.description}`} className="grid grid-cols-[16rem_1fr] items-center gap-8">
                  <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
                    {shortcut.options.map((option) => (
                      <span
                        key={`${shortcut.description}-${option.modifiers.join('|')}-${option.keys.join('|')}`}
                        className="inline-flex items-center gap-1"
                      >
                        {option.modifiers.map((modifier, modifierIndex) => (
                          <span key={modifier} className="inline-flex items-center gap-1">
                            <Kbd>{modifier}</Kbd>
                            {modifierIndex < option.modifiers.length - 1 ? <span className="text-[0.9rem] text-muted-foreground">+</span> : null}
                          </span>
                        ))}
                        {option.modifiers.length && option.keys.length ? <span className="text-[0.9rem] text-muted-foreground">+</span> : null}
                        {option.keys.map((key, keyIndex) => (
                          <span key={`${key}-${keyIndex}`} className="contents">
                            <Kbd>{key}</Kbd>
                          </span>
                        ))}
                      </span>
                    ))}
                  </div>
                  <span className="text-xs text-foreground">{shortcut.description}</span>
                </div>
              ))}
            </div>
          </div>
        ))}
      </div>
    </div>
  )
}
