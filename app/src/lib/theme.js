const THEME_COLOR_VARS = {
  background: '--theme-color-background',
  accent: '--theme-color-accent',
  aqua: '--theme-color-aqua',
  ice: '--theme-color-ice',
  teal: '--theme-color-teal',
}

export function getThemeColor(name) {
  const variableName = THEME_COLOR_VARS[name] || name

  if (typeof window === 'undefined' || typeof document === 'undefined') {
    return ''
  }

  return getComputedStyle(document.documentElement)
    .getPropertyValue(variableName)
    .trim()
}
