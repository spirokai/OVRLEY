/**
 * Scene settings constants — aspect ratios and resolution presets for the overlay canvas.
 */

export const ASPECT_RATIOS = [
  { id: '16:9', name: 'Widescreen (16:9)' },
  { id: '9:16', name: 'Vertical (9:16)' },
  { id: '1:1', name: 'Square (1:1)' },
  { id: '4:3', name: 'Portrait (4:3)' },
  { id: '21:9', name: 'Ultrawide (21:9)' },
  { id: 'custom', name: 'Custom' },
]

export const RESOLUTIONS = {
  '16:9': [
    { id: '4k', name: '4K (3840x2160)', w: 3840, h: 2160 },
    { id: '1080p', name: '1080p (1920x1080)', w: 1920, h: 1080 },
    { id: '720p', name: '720p (1280x720)', w: 1280, h: 720 },
  ],
  '9:16': [
    { id: '4k-v', name: '4K Vertical (2160x3840)', w: 2160, h: 3840 },
    { id: '1080p-v', name: '1080p Vertical (1080x1920)', w: 1080, h: 1920 },
  ],
  '1:1': [
    { id: '1080s', name: '1080p Square (1080x1080)', w: 1080, h: 1080 },
    { id: '2160s', name: '4K Square (2160x2160)', w: 2160, h: 2160 },
  ],
  '4:3': [
    { id: 'sxga', name: 'SXGA+ (1400x1050)', w: 1400, h: 1050 },
    { id: 'uxga', name: 'UXGA (1600x1200)', w: 1600, h: 1200 },
    { id: 'hires', name: 'XGA (1920x1440)', w: 1920, h: 1440 },
    { id: 'qxga', name: 'QXGA (2048x1536)', w: 2048, h: 1536 },
  ],
  '21:9': [{ id: 'ultra', name: 'Ultrawide (3440x1440)', w: 3440, h: 1440 }],
}
