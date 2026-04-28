export function normalizeHexColor(value, fallback = '#000000') {
  if (typeof value !== 'string') {
    return fallback
  }

  const trimmed = value.trim().replace(/^#/, '')

  if (/^[0-9a-fA-F]{3}$/.test(trimmed)) {
    return `#${trimmed
      .split('')
      .map((part) => part.repeat(2))
      .join('')
      .toLowerCase()}`
  }

  if (/^[0-9a-fA-F]{4}$/.test(trimmed)) {
    return `#${trimmed
      .slice(0, 3)
      .split('')
      .map((part) => part.repeat(2))
      .join('')
      .toLowerCase()}`
  }

  if (/^[0-9a-fA-F]{6}$/.test(trimmed)) {
    return `#${trimmed.toLowerCase()}`
  }

  if (/^[0-9a-fA-F]{8}$/.test(trimmed)) {
    return `#${trimmed.slice(0, 6).toLowerCase()}`
  }

  return fallback
}

export function isColorFieldKey(key) {
  return key === 'color' || key.endsWith('_color') || key.startsWith('color_')
}

export function normalizeColorFields(record) {
  if (!record || typeof record !== 'object') {
    return record
  }

  return Object.fromEntries(
    Object.entries(record).map(([key, value]) => [
      key,
      isColorFieldKey(key) ? normalizeHexColor(value) : value,
    ]),
  )
}
