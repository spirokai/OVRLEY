/**
 * Scene settings constants — aspect ratios and resolution presets for the overlay canvas.
 */

export const ASPECT_RATIOS = [
  { id: '16:9', labelKey: 'scene-settings.widescreen' },
  { id: '9:16', labelKey: 'scene-settings.vertical' },
  { id: '1:1', labelKey: 'scene-settings.square' },
  { id: '4:3', labelKey: 'scene-settings.portrait' },
  { id: '21:9', labelKey: 'scene-settings.ultrawide' },
  { id: 'custom', labelKey: 'scene-settings.custom' },
]

export const RESOLUTIONS = {
  '16:9': [
    { id: '4k', labelKey: 'scene-settings.resolution4k', w: 3840, h: 2160 },
    { id: '1080p', labelKey: 'scene-settings.resolution1080p', w: 1920, h: 1080 },
    { id: '720p', labelKey: 'scene-settings.resolution720p', w: 1280, h: 720 },
  ],
  '9:16': [
    { id: '4k-v', labelKey: 'scene-settings.resolution4kVertical', w: 2160, h: 3840 },
    { id: '1080p-v', labelKey: 'scene-settings.resolution1080pVertical', w: 1080, h: 1920 },
  ],
  '1:1': [
    { id: '1080s', labelKey: 'scene-settings.resolution1080pSquare', w: 1080, h: 1080 },
    { id: '2160s', labelKey: 'scene-settings.resolution4kSquare', w: 2160, h: 2160 },
  ],
  '4:3': [
    { id: 'sxga', labelKey: 'scene-settings.resolutionSxga', w: 1400, h: 1050 },
    { id: 'uxga', labelKey: 'scene-settings.resolutionUxga', w: 1600, h: 1200 },
    { id: 'hires', labelKey: 'scene-settings.resolutionXga', w: 1920, h: 1440 },
    { id: 'qxga', labelKey: 'scene-settings.resolutionQxga', w: 2048, h: 1536 },
  ],
  '21:9': [{ id: 'ultra', labelKey: 'scene-settings.resolutionUltrawide', w: 3440, h: 1440 }],
}
