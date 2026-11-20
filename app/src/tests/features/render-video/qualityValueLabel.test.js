import { describe, expect, test } from 'vite-plus/test'
import { formatQualityValueLabel } from '@/features/render-video/utils/codecUtils'

describe('formatQualityValueLabel', () => {
  test('shows CRF for encoders that take CRF directly', () => {
    expect(formatQualityValueLabel({ codec: 'libx264', qualityType: 'quality', qualityValue: 21 })).toBe('CRF 21')
    expect(formatQualityValueLabel({ codec: 'hevc_nvenc', qualityType: 'quality', qualityValue: 26 })).toBe('CRF 26')
  })

  test('shows the VideoToolbox quality the backend sends, with the slider best end at 100', () => {
    expect(formatQualityValueLabel({ codec: 'h264_videotoolbox', qualityType: 'quality', qualityValue: 10 })).toBe('Quality 100')
    expect(formatQualityValueLabel({ codec: 'hevc_videotoolbox', qualityType: 'quality', qualityValue: 10 })).toBe('Quality 100')
    expect(formatQualityValueLabel({ codec: 'h264_videotoolbox', qualityType: 'quality', qualityValue: 21 })).toBe('Quality 72')
    expect(formatQualityValueLabel({ codec: 'hevc_videotoolbox', qualityType: 'quality', qualityValue: 26 })).toBe('Quality 70')
    expect(formatQualityValueLabel({ codec: 'h264_videotoolbox', qualityType: 'quality', qualityValue: 18 })).toBe('Quality 80')
    expect(formatQualityValueLabel({ codec: 'hevc_videotoolbox', qualityType: 'quality', qualityValue: 18 })).toBe('Quality 85')
  })

  test('saturates VideoToolbox quality at the scale limits', () => {
    expect(formatQualityValueLabel({ codec: 'h264_videotoolbox', qualityType: 'quality', qualityValue: 1 })).toBe('Quality 100')
    expect(formatQualityValueLabel({ codec: 'h264_videotoolbox', qualityType: 'quality', qualityValue: 51 })).toBe('Quality 1')
  })

  test('shows bitrate in Mbps for every encoder', () => {
    expect(formatQualityValueLabel({ codec: 'hevc_videotoolbox', qualityType: 'bitrate', qualityValue: 40 })).toBe('40 Mbps')
  })
})
