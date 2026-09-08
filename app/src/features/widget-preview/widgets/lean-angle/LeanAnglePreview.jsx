import { useLeanAnglePreview } from './useLeanAnglePreview'
import { PreviewSvgShadowBlurFilter, PreviewSvgText } from '../../shared/PreviewSvgComponents'

/** Renders the lean-angle track, dynamic signed fill, and centred value. */
export function OverlayLeanAngleWidget({ widget, activity, previewSecond, globalOpacity, globalScale, sceneStyle }) {
  const presentation = useLeanAnglePreview({ widget, activity, previewSecond, globalOpacity, globalScale, sceneStyle })
  const { layout, textLayout } = presentation

  return (
    <svg
      width={layout.width * globalScale}
      height={layout.height * globalScale}
      viewBox={`0 0 ${layout.width} ${layout.height}`}
      className="block overflow-visible"
      data-testid="lean-angle-preview"
    >
      {presentation.shadow ? <PreviewSvgShadowBlurFilter id={presentation.shadowFilterId} shadow={presentation.shadow} /> : null}
      <defs>
        {widget.data.track_border_thickness > 0 ? (
          <mask id={presentation.maskId}>
            <path d={presentation.outerTrackPath} fill="white" fillRule="evenodd" />
            <path d={presentation.innerTrackPath} fill="black" fillRule="evenodd" />
          </mask>
        ) : null}
        <clipPath id={presentation.innerTrackClipId}>
          <path d={presentation.innerTrackPath} fillRule="evenodd" />
        </clipPath>
      </defs>
      {presentation.shadow && widget.data.track_border_thickness > 0 ? (
        <g transform={`translate(${presentation.shadow.distance} ${presentation.shadow.distance})`} filter={`url(#${presentation.shadowFilterId})`}>
          <g mask={`url(#${presentation.maskId})`}>
            <path d={presentation.outerTrackPath} fill={presentation.shadow.color} fillOpacity={presentation.opacity} fillRule="evenodd" />
          </g>
        </g>
      ) : null}
      <path
        data-testid="lean-angle-empty-track"
        d={presentation.innerTrackPath}
        fill={widget.data.track_empty_color}
        fillOpacity={widget.data.track_empty_opacity * presentation.opacity}
        fillRule="evenodd"
      />
      {widget.data.track_border_thickness > 0 ? (
        <path
          data-testid="lean-angle-border"
          d={presentation.outerTrackPath}
          fill={widget.data.track_border_color}
          mask={`url(#${presentation.maskId})`}
          opacity={presentation.opacity}
        />
      ) : null}
      {presentation.fillPath ? (
        <path
          data-testid="lean-angle-filled-track"
          d={presentation.fillPath}
          fill={widget.data.track_filled_color}
          fillOpacity={widget.data.track_filled_opacity * presentation.opacity}
          fillRule="evenodd"
          clipPath={`url(#${presentation.innerTrackClipId})`}
        />
      ) : null}
      <g transform={`translate(${presentation.textOriginX} ${presentation.textOriginY})`}>
        <PreviewSvgText
          text={presentation.valueText}
          x={textLayout.value.left}
          baseline={textLayout.value.baseline}
          color={widget.data.color}
          fontFamily={presentation.fontFamily}
          fontSize={widget.data.font_size}
          opacity={presentation.opacity}
          shadow={presentation.shadow}
          shadowFilterId={presentation.valueShadowFilterId}
          borderColor={sceneStyle?.border_color}
          borderThickness={sceneStyle?.border_thickness}
        />
        {textLayout.units ? (
          <PreviewSvgText
            text={presentation.unitText}
            x={textLayout.units.left}
            baseline={textLayout.units.baseline}
            color={widget.data.unit_color}
            fontFamily={presentation.fontFamily}
            fontSize={textLayout.units.fontSize}
            opacity={presentation.opacity}
            shadow={presentation.shadow}
            shadowFilterId={presentation.unitShadowFilterId}
            borderColor={sceneStyle?.border_color}
            borderThickness={sceneStyle?.border_thickness}
          />
        ) : null}
      </g>
    </svg>
  )
}
