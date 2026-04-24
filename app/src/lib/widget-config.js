export function cloneConfig(config) {
  return JSON.parse(JSON.stringify(config))
}

export function buildConfigWidgets(config) {
  if (!config) return []

  const widgets = []

  ;(config.labels || []).forEach((item, index) => {
    widgets.push({
      id: `label-${index}`,
      type: 'label',
      category: 'labels',
      index,
      name: item.text || 'Text',
      data: item,
    })
  })
  ;(config.values || []).forEach((item, index) => {
    widgets.push({
      id: `value-${index}`,
      type: item.value,
      category: 'values',
      index,
      name: item.value,
      data: item,
    })
  })
  ;(config.plots || []).forEach((item, index) => {
    widgets.push({
      id: `plot-${index}`,
      type: item.value,
      category: 'plots',
      index,
      name: item.value,
      data: item,
    })
  })

  return widgets
}

export function groupWidgetsForSidebar(widgets, typeLabels) {
  const grouped = widgets.reduce((accumulator, widget) => {
    const typeName = typeLabels[widget.type] || widget.type
    if (!accumulator[typeName]) accumulator[typeName] = []
    accumulator[typeName].push({
      ...widget,
      name: widget.type === 'label' ? widget.name : typeName,
    })
    return accumulator
  }, {})

  return Object.keys(grouped)
    .sort()
    .flatMap((typeName, groupIndex) =>
      grouped[typeName].map((widget, widgetIndex) => ({
        ...widget,
        showSeparator: groupIndex > 0 && widgetIndex === 0,
      })),
    )
}

export function findWidgetById(config, widgetId) {
  return (
    buildConfigWidgets(config).find((widget) => widget.id === widgetId) || null
  )
}

export function updateWidgetInConfig(config, widgetId, updates) {
  if (!config) return config

  const widget = findWidgetById(config, widgetId)
  if (!widget) return config

  const nextConfig = cloneConfig(config)
  const currentWidget = nextConfig[widget.category]?.[widget.index]
  if (!currentWidget) return config

  nextConfig[widget.category][widget.index] = {
    ...currentWidget,
    ...updates,
  }

  return nextConfig
}

export function updateWidgetsInConfig(config, updatesById) {
  if (!config || !updatesById || typeof updatesById !== 'object') {
    return config
  }

  const widgetUpdates = Object.entries(updatesById).filter(
    ([, updates]) => updates && typeof updates === 'object',
  )
  if (!widgetUpdates.length) {
    return config
  }

  const widgetsById = Object.fromEntries(
    buildConfigWidgets(config).map((widget) => [widget.id, widget]),
  )
  const nextConfig = cloneConfig(config)
  let hasChanges = false

  widgetUpdates.forEach(([widgetId, updates]) => {
    const widget = widgetsById[widgetId]
    const currentWidget = widget
      ? nextConfig[widget.category]?.[widget.index]
      : null

    if (!widget || !currentWidget) {
      return
    }

    nextConfig[widget.category][widget.index] = {
      ...currentWidget,
      ...updates,
    }
    hasChanges = true
  })

  return hasChanges ? nextConfig : config
}

export function replaceWidgetInConfig(config, widgetId, nextWidgetData) {
  if (!config) return config

  const widget = findWidgetById(config, widgetId)
  if (!widget) return config

  const nextConfig = cloneConfig(config)
  if (!nextConfig[widget.category]?.[widget.index]) return config

  nextConfig[widget.category][widget.index] = nextWidgetData
  return nextConfig
}

export function deleteWidgetInConfig(config, widgetId) {
  if (!config) return config

  const widget = findWidgetById(config, widgetId)
  if (!widget) return config

  const nextConfig = cloneConfig(config)
  nextConfig[widget.category] = (nextConfig[widget.category] || []).filter(
    (_, index) => index !== widget.index,
  )

  return nextConfig
}

export function deleteWidgetsInConfig(config, widgetIds) {
  if (!config || !Array.isArray(widgetIds) || !widgetIds.length) {
    return config
  }

  const idsToDelete = new Set(widgetIds)
  const indexesByCategory = buildConfigWidgets(config).reduce(
    (accumulator, widget) => {
      if (!idsToDelete.has(widget.id)) {
        return accumulator
      }

      if (!accumulator[widget.category]) {
        accumulator[widget.category] = new Set()
      }

      accumulator[widget.category].add(widget.index)
      return accumulator
    },
    {},
  )

  const categories = Object.keys(indexesByCategory)
  if (!categories.length) {
    return config
  }

  const nextConfig = cloneConfig(config)
  categories.forEach((category) => {
    const indexes = indexesByCategory[category]
    nextConfig[category] = (nextConfig[category] || []).filter(
      (_, index) => !indexes.has(index),
    )
  })

  return nextConfig
}
