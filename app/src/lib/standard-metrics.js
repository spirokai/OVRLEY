const CURRENT_STANDARD_METRIC_WIDGET_TYPES = ['speed', 'heartrate', 'cadence', 'power', 'temperature']

const STANDARD_METRIC_DEFINITIONS = {
  speed: {
    label: 'Speed',
    defaultDisplayUnit: 'kmh',
    supportedDisplayUnits: [
      { value: 'kmh', label: 'km/h' },
      { value: 'mph', label: 'mph' },
      { value: 'kn', label: 'kn' },
      { value: 'mps', label: 'm/s' },
    ],
    showUnitsByDefault: true,
    formatter: 'speed',
    icon: {
      source: 'shared',
      assetFile: 'widget-speed.svg',
    },
  },
  heartrate: {
    label: 'Heart Rate',
    defaultDisplayUnit: 'bpm',
    supportedDisplayUnits: [{ value: 'bpm', label: 'BPM' }],
    showUnitsByDefault: false,
    formatter: 'integer',
    icon: {
      source: 'shared',
      assetFile: 'widget-heartrate.svg',
    },
  },
  cadence: {
    label: 'Cadence',
    defaultDisplayUnit: 'rpm',
    supportedDisplayUnits: [{ value: 'rpm', label: 'RPM' }],
    showUnitsByDefault: false,
    formatter: 'integer',
    icon: {
      source: 'shared',
      assetFile: 'widget-cadence.svg',
    },
  },
  power: {
    label: 'Power',
    defaultDisplayUnit: 'w',
    supportedDisplayUnits: [{ value: 'w', label: 'W' }],
    showUnitsByDefault: false,
    formatter: 'integer',
    icon: {
      source: 'shared',
      assetFile: 'widget-power.svg',
    },
  },
  temperature: {
    label: 'Temperature',
    defaultDisplayUnit: 'celsius',
    supportedDisplayUnits: [
      { value: 'celsius', label: '\u00B0C' },
      { value: 'fahrenheit', label: '\u00B0F' },
    ],
    showUnitsByDefault: true,
    formatter: 'temperature',
    icon: {
      source: 'shared',
      assetFile: 'widget-temperature.svg',
    },
  },
  pace: {
    label: 'Pace',
    defaultDisplayUnit: 'min_per_km',
    supportedDisplayUnits: [
      { value: 'min_per_km', label: 'min/km' },
      { value: 'min_per_mi', label: 'min/mi' },
    ],
    showUnitsByDefault: true,
    formatter: 'pace',
    icon: {
      source: 'lucide',
      name: 'Footprints',
      assetFile: 'widget-pace.svg',
    },
  },
  g_force: {
    label: 'G-Force',
    defaultDisplayUnit: 'g',
    supportedDisplayUnits: [
      { value: 'g', label: 'g' },
      { value: 'mps2', label: 'm/s^2' },
    ],
    showUnitsByDefault: true,
    formatter: 'decimal',
    icon: {
      source: 'custom',
      assetFile: 'widget-g-force.svg',
    },
  },
  air_pressure: {
    label: 'Air Pressure',
    defaultDisplayUnit: 'hpa',
    supportedDisplayUnits: [
      { value: 'hpa', label: 'hPa' },
      { value: 'mbar', label: 'mbar' },
      { value: 'inhg', label: 'inHg' },
      { value: 'mmhg', label: 'mmHg' },
    ],
    showUnitsByDefault: true,
    formatter: 'integer',
    icon: {
      source: 'lucide',
      name: 'Wind',
      assetFile: 'widget-air-pressure.svg',
    },
  },
  ground_contact_time: {
    label: 'Ground Contact Time',
    defaultDisplayUnit: 'ms',
    supportedDisplayUnits: [{ value: 'ms', label: 'ms' }],
    showUnitsByDefault: true,
    formatter: 'integer',
    icon: {
      source: 'custom',
      assetFile: 'widget-ground-contact-time.svg',
    },
  },
  left_right_balance: {
    label: 'Left/Right Balance',
    defaultDisplayUnit: 'percent',
    supportedDisplayUnits: [{ value: 'percent', label: '52% / 48%' }],
    showUnitsByDefault: false,
    formatter: 'balance',
    icon: {
      source: 'lucide',
      name: 'Scale',
      assetFile: 'widget-left-right-balance.svg',
    },
  },
  stride_length: {
    label: 'Stride Length',
    defaultDisplayUnit: 'm',
    supportedDisplayUnits: [
      { value: 'm', label: 'm' },
      { value: 'cm', label: 'cm' },
      { value: 'ft', label: 'ft' },
      { value: 'in', label: 'in' },
    ],
    showUnitsByDefault: true,
    formatter: 'decimal',
    icon: {
      source: 'lucide',
      name: 'Ruler',
      assetFile: 'widget-stride-length.svg',
    },
  },
  stroke_rate: {
    label: 'Stroke Rate',
    defaultDisplayUnit: 'spm',
    supportedDisplayUnits: [{ value: 'spm', label: 'SPM' }],
    showUnitsByDefault: true,
    formatter: 'integer',
    icon: {
      source: 'lucide',
      name: 'Waves',
      assetFile: 'widget-stroke-rate.svg',
    },
  },
  torque: {
    label: 'Torque',
    defaultDisplayUnit: 'nm',
    supportedDisplayUnits: [{ value: 'nm', label: 'Nm' }],
    showUnitsByDefault: true,
    formatter: 'decimal',
    icon: {
      source: 'custom',
      assetFile: 'widget-torque.svg',
    },
  },
  vertical_speed: {
    label: 'Vertical Speed',
    defaultDisplayUnit: 'mps',
    supportedDisplayUnits: [
      { value: 'mps', label: 'm/s' },
      { value: 'ftmin', label: 'ft/min' },
      { value: 'mph_vertical', label: 'm/h' },
    ],
    showUnitsByDefault: true,
    formatter: 'decimal',
    icon: {
      source: 'lucide',
      name: 'TrendingUp',
      assetFile: 'widget-vertical-speed.svg',
    },
  },
  gear_position: {
    label: 'Gear Position',
    defaultDisplayUnit: 'gear',
    supportedDisplayUnits: [{ value: 'gear', label: 'Gear' }],
    showUnitsByDefault: false,
    formatter: 'integer',
    icon: {
      source: 'custom',
      assetFile: 'widget-gear-position.svg',
    },
  },
  vertical_ratio: {
    label: 'Vertical Ratio',
    defaultDisplayUnit: 'percent',
    supportedDisplayUnits: [{ value: 'percent', label: '%' }],
    showUnitsByDefault: true,
    formatter: 'decimal',
    icon: {
      source: 'lucide',
      name: 'Percent',
      assetFile: 'widget-vertical-ratio.svg',
    },
  },
  core_temperature: {
    label: 'Core Temperature',
    defaultDisplayUnit: 'celsius',
    supportedDisplayUnits: [
      { value: 'celsius', label: '\u00B0C' },
      { value: 'fahrenheit', label: '\u00B0F' },
    ],
    showUnitsByDefault: true,
    formatter: 'temperature',
    icon: {
      source: 'lucide',
      name: 'Thermometer',
      assetFile: 'widget-core-temperature.svg',
    },
  },
}

export { CURRENT_STANDARD_METRIC_WIDGET_TYPES }

export const STANDARD_METRIC_WIDGET_TYPES = Object.keys(STANDARD_METRIC_DEFINITIONS)

export function isStandardMetricWidgetType(type) {
  return Object.hasOwn(STANDARD_METRIC_DEFINITIONS, type)
}

export function getStandardMetricDefinition(type) {
  return STANDARD_METRIC_DEFINITIONS[type] ?? null
}

export function getStandardMetricDisplayUnit(type, widgetData = {}) {
  const definition = getStandardMetricDefinition(type)
  return widgetData.display_unit || definition?.defaultDisplayUnit || null
}

export function getStandardMetricUnitOptions(type) {
  return getStandardMetricDefinition(type)?.supportedDisplayUnits ?? []
}

export function getStandardMetricUnitLabel(type, displayUnit) {
  const definition = getStandardMetricDefinition(type)
  const resolvedUnit = displayUnit || definition?.defaultDisplayUnit
  return definition?.supportedDisplayUnits.find((option) => option.value === resolvedUnit)?.label ?? ''
}
