import { beforeAll, describe, expect, test, vi } from 'vite-plus/test'
import { fireEvent, render, screen } from '@testing-library/react'
import i18next from '@/i18n'
import RollingChartWidgetEditor from '@/features/widget-editor/components/RollingChartWidgetEditor'
import {
  CHART_LIMITS,
  PLOTTABLE_METRICS,
  addChartSeries,
  formatWindowSeconds,
  moveChartSeries,
  parseWindowSeconds,
  removeChartSeries,
  replaceChartSeriesMetric,
} from '@/features/widget-editor/utils/rollingChart'
import { WIDGET_TYPE_DEFINITIONS } from '@/lib/widget/widget-catalog'
import { createWidgetDefaults } from '@/lib/widget/widget-defaults'
import { DEFAULT_GLOBAL_DEFAULTS } from '@/lib/template/template-constants'
import { createDurableEditorState, createEditorEffectiveConfig, getEffectiveWidgetData } from '@/lib/template/template-state'

beforeAll(() => {
  globalThis.ResizeObserver = class ResizeObserver {
    observe() {}
    unobserve() {}
    disconnect() {}
  }
})

const SERIES = [
  { metric: 'speed', color: '#ffffff' },
  { metric: 'heartrate', color: '#e0103a' },
]

describe('rolling chart series and window', () => {
  test('plots numeric metrics only, never specially formatted ones', () => {
    expect(PLOTTABLE_METRICS).toEqual(expect.arrayContaining(['speed', 'heartrate', 'throttle_position', 'brake_position', 'gradient']))
    for (const metric of ['left_right_balance', 'gas_mix', 'gear_position', 'gps_coordinates', 'lap_timer', 'time', 'heading']) {
      expect(PLOTTABLE_METRICS).not.toContain(metric)
    }
  })

  test('accepts the window as seconds or minutes and seconds within the limits', () => {
    expect(parseWindowSeconds('45')).toBe(45)
    expect(parseWindowSeconds(' 1:30 ')).toBe(90)
    expect(parseWindowSeconds('5:00')).toBe(300)
    expect(parseWindowSeconds('0:05')).toBe(CHART_LIMITS.minWindowSeconds)
    for (const text of ['4', '5:01', '1:5', '1:75', '', 'abc', '-10', '1.5']) expect(parseWindowSeconds(text)).toBeNull()
    expect(formatWindowSeconds(30)).toBe('0:30')
    expect(formatWindowSeconds(125)).toBe('2:05')
  })

  test('adds the next unplotted metric in an unused palette color', () => {
    const next = addChartSeries(SERIES)
    expect(next).toHaveLength(3)
    expect(next[2].metric).not.toMatch(/^(speed|heartrate)$/)
    expect(Object.keys(next[2]).sort()).toEqual(['color', 'metric'])
    expect(SERIES.map((item) => item.color)).not.toContain(next[2].color)
  })

  test('replacing a metric keeps its color; moving swaps neighbours; removing drops the row', () => {
    expect(replaceChartSeriesMetric(SERIES, 1, 'cadence')[1]).toEqual({ metric: 'cadence', color: '#e0103a' })
    expect(moveChartSeries(SERIES, 1, -1).map((item) => item.metric)).toEqual(['heartrate', 'speed'])
    expect(removeChartSeries(SERIES, 0)).toEqual([SERIES[1]])
    expect(SERIES).toHaveLength(2)
  })

  test('templates store no legend labels; rendering names them capitalized in the requested language', () => {
    const data = createWidgetDefaults({ type: 'rolling_chart' }, DEFAULT_GLOBAL_DEFAULTS)
    expect(data.legend_font).toBe(DEFAULT_GLOBAL_DEFAULTS.font_values)
    const config = { scene: {}, rasters: [], plots: [data] }
    const durable = createDurableEditorState({ config, globalDefaults: DEFAULT_GLOBAL_DEFAULTS })
    for (const item of durable.config.plots[0].series) expect(item).not.toHaveProperty('label')
    const labels = (language) =>
      createEditorEffectiveConfig({ config: durable.config, globalDefaults: durable.globalDefaults, language }).plots[0].series.map(
        (item) => item.label,
      )
    expect(labels('en')).toEqual(['SPEED', 'HR'])
    expect(labels('de')).toEqual(data.series.map((item) => i18next.getFixedT('de')(WIDGET_TYPE_DEFINITIONS[item.metric].shortNameKey).toUpperCase()))
    // Locale-aware capitalization: Turkish dotted i.
    const turkish = getEffectiveWidgetData({ category: 'plots', data: { ...data, series: [{ metric: 'cadence', color: '#ffffff' }] } }, {}, 'tr')
    expect(turkish.series[0].label).toBe(i18next.getFixedT('tr')('widgets.types.cadence.shortName').toLocaleUpperCase('tr'))
    expect(data.series).toEqual(WIDGET_TYPE_DEFINITIONS.rolling_chart.defaults.series)
  })
})

describe('rolling chart editor', () => {
  function renderEditor(overrides = {}) {
    const updateWidgetData = vi.fn()
    const widget = {
      id: 'chart',
      type: 'rolling_chart',
      category: 'plots',
      data: { ...WIDGET_TYPE_DEFINITIONS.rolling_chart.defaults, ...overrides },
    }
    render(
      <RollingChartWidgetEditor
        widget={widget}
        updateWidgetData={updateWidgetData}
        updateWidgetSize={vi.fn()}
        commitWidgetSize={vi.fn()}
        setNumericField={vi.fn()}
      />,
    )
    return updateWidgetData
  }

  test('edits the series list through its row controls', () => {
    const update = renderEditor()
    fireEvent.click(screen.getByRole('button', { name: 'Add metric' }))
    expect(update.mock.calls.at(-1)[1].series).toHaveLength(3)
    fireEvent.click(screen.getAllByRole('button', { name: 'Move down' })[0])
    expect(update.mock.calls.at(-1)[1].series.map((item) => item.metric)).toEqual(['heartrate', 'speed'])
    fireEvent.click(screen.getAllByRole('button', { name: 'Remove metric' })[1])
    expect(update.mock.calls.at(-1)[1].series.map((item) => item.metric)).toEqual(['speed'])
    expect(screen.getAllByRole('button', { name: 'Move up' })[0]).toBeDisabled()
  })

  test('keeps the last metric', () => {
    renderEditor({ series: [SERIES[0]] })
    expect(screen.getByRole('button', { name: 'Remove metric' })).toBeDisabled()
  })

  test('stops adding at the series limit', () => {
    renderEditor({ series: PLOTTABLE_METRICS.slice(0, CHART_LIMITS.maxSeries).map((metric) => ({ metric, label: metric, color: '#ffffff' })) })
    expect(screen.getByRole('button', { name: 'Add metric' })).toBeDisabled()
  })

  test('commits a typed window only when it parses', () => {
    const update = renderEditor()
    const input = screen.getByRole('textbox', { name: 'Time Window' })
    expect(input).toHaveValue('0:30')
    fireEvent.change(input, { target: { value: '2' } })
    fireEvent.blur(input)
    expect(input).toHaveAttribute('aria-invalid', 'true')
    expect(update).not.toHaveBeenCalled()
    fireEvent.change(input, { target: { value: '1:15' } })
    fireEvent.blur(input)
    expect(update).toHaveBeenCalledWith('chart', { window_seconds: 75 })
  })

  test('colors markers per series and gates the gradient on the area fill', () => {
    renderEditor({ area_fill: false })
    expect(screen.getByText('Marker Type')).toBeInTheDocument()
    expect(screen.queryByText('Color')).not.toBeInTheDocument()
    expect(screen.getByRole('switch', { name: 'Gradient' })).toBeDisabled()
  })
})
