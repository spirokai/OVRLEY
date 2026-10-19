//! Shared placement and pixel revisions for every preview family.
use crate::{
    error::{WidgetError, WidgetResult},
    preview::PreviewFonts,
    preview_widget::{PreviewWidget, WidgetRequest},
    widgets::{raster::preview::PreviewImages, value::preview::WidgetGeometry},
};
use serde::{Deserialize, Serialize};

impl WidgetRequest {
    fn placement(&self) -> WidgetResult<([f64; 2], bool)> {
        let (position, transient) = match self {
            Self::DivingProfile {
                widget, transient, ..
            } => ([widget.x as f64, widget.y as f64], *transient),
            Self::Elevation {
                widget, transient, ..
            } => ([widget.x as f64, widget.y as f64], *transient),
            Self::Route {
                widget, transient, ..
            } => ([widget.x as f64, widget.y as f64], *transient),
            Self::Raster {
                widget, transient, ..
            } => ([widget.x, widget.y], *transient),
            Self::Backdrop {
                widget, transient, ..
            } => (
                [
                    widget
                        .x
                        .ok_or_else(|| WidgetError::Config("backdrop.x is required".into()))?
                        as f64,
                    widget
                        .y
                        .ok_or_else(|| WidgetError::Config("backdrop.y is required".into()))?
                        as f64,
                ],
                *transient,
            ),
            Self::Label {
                widget, transient, ..
            } => ([widget.x as f64, widget.y as f64], *transient),
            Self::Gradient(request)
            | Self::LeanAngle(request)
            | Self::GForce(request)
            | Self::LapTimer(request)
            | Self::HeadingTape(request)
            | Self::Gauge(request)
            | Self::Value(request) => (
                [request.widget.x as f64, request.widget.y as f64],
                request.transient,
            ),
        };
        Ok((position, transient))
    }

    /// Remove only placement and draft state from the pixel identity. All other
    /// required config remains intact and must pass the family's normal ingress.
    fn pixel_config(&mut self) {
        match self {
            Self::DivingProfile {
                widget, transient, ..
            } => {
                widget.x = 0.0;
                widget.y = 0.0;
                *transient = false;
            }
            Self::Elevation {
                widget, transient, ..
            } => {
                widget.x = 0.0;
                widget.y = 0.0;
                *transient = false;
            }
            Self::Route {
                widget, transient, ..
            } => {
                widget.x = 0.0;
                widget.y = 0.0;
                *transient = false;
            }
            Self::Raster {
                widget, transient, ..
            } => {
                widget.x = 0.0;
                widget.y = 0.0;
                *transient = false;
            }
            Self::Backdrop {
                widget, transient, ..
            } => {
                widget.x = Some(0.0);
                widget.y = Some(0.0);
                *transient = false;
            }
            Self::Label {
                widget, transient, ..
            } => {
                widget.x = 0.0;
                widget.y = 0.0;
                *transient = false;
            }
            Self::Gradient(request)
            | Self::LeanAngle(request)
            | Self::GForce(request)
            | Self::LapTimer(request)
            | Self::HeadingTape(request)
            | Self::Gauge(request)
            | Self::Value(request) => {
                request.widget.x = 0.0;
                request.widget.y = 0.0;
                request.transient = false;
            }
        }
    }
}

/// Family geometry has optional rotation metadata, and source rasters also have
/// presentation opacity. Translation changes neither of those properties.
#[derive(Deserialize, Serialize)]
struct PlacementGeometry {
    #[serde(flatten)]
    widget: WidgetGeometry,
    #[serde(skip_serializing_if = "Option::is_none")]
    rotation: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    rotation_origin: Option<[f32; 2]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    opacity: Option<f32>,
}

pub struct PreviewInstance {
    pub widget: PreviewWidget,
    config: WidgetRequest,
    origin: [f64; 2],
    position: [f64; 2],
    placed_geometry: Option<Vec<u8>>,
    rendered_revision: Option<u32>,
    rendered_pixels_revision: Option<u32>,
    revision: u32,
    pixels_revision: u32,
}

impl PreviewInstance {
    pub fn new(
        request: WidgetRequest,
        fonts: &PreviewFonts,
        images: &PreviewImages,
    ) -> WidgetResult<Self> {
        let (position, _) = request.placement()?;
        let mut config = request.clone();
        config.pixel_config();
        let mut instance = Self {
            widget: PreviewWidget::new(request, fonts, images)?,
            config,
            origin: position,
            position,
            placed_geometry: None,
            rendered_revision: None,
            rendered_pixels_revision: None,
            revision: 0,
            pixels_revision: 0,
        };
        instance.sync()?;
        Ok(instance)
    }

    pub fn update(
        &mut self,
        request: WidgetRequest,
        fonts: &PreviewFonts,
        images: &PreviewImages,
    ) -> WidgetResult<()> {
        let (position, transient) = request.placement()?;
        let mut config = request.clone();
        config.pixel_config();
        let delta = [position[0] - self.origin[0], position[1] - self.origin[1]];
        let source_raster = matches!(&self.widget, PreviewWidget::Raster(_));
        // A live drag may resample cached pixels at a fractional position. Once
        // committed, rebuild at that position to match native export exactly.
        if config == self.config
            && (transient || source_raster || delta.iter().all(|value| value.fract() == 0.0))
        {
            // Pixel-changing requests use the family's validation below. Only
            // placement-only input bypasses it and validates its new x/y here.
            if position.iter().any(|value| !(*value as f32).is_finite()) {
                return Err(WidgetError::Config(
                    "widget x/y must be finite scene coordinates".into(),
                ));
            }
            if position != self.position {
                self.position = position;
                self.place_geometry()?;
                self.revision = self.revision.wrapping_add(1);
            }
            return Ok(());
        }
        match (&mut self.widget, request) {
            (
                PreviewWidget::DivingProfile(profile),
                WidgetRequest::DivingProfile {
                    widget,
                    scene,
                    transient,
                },
            ) => profile.update(widget, scene, transient)?,
            (
                PreviewWidget::Elevation(elevation),
                WidgetRequest::Elevation {
                    widget,
                    scene,
                    transient,
                },
            ) => elevation.update(widget, scene, transient)?,
            (
                PreviewWidget::Route(route),
                WidgetRequest::Route {
                    widget,
                    scene,
                    transient,
                },
            ) => route.update(widget, scene, transient)?,
            (PreviewWidget::Raster(raster), WidgetRequest::Raster { widget, scene, .. }) => {
                raster.update(widget, scene, images)?
            }
            (_, request) => {
                self.widget = PreviewWidget::new(request, fonts, images)?;
                self.rendered_revision = None;
                self.rendered_pixels_revision = None;
            }
        }
        self.config = config;
        self.origin = position;
        self.position = position;
        self.placed_geometry = None;
        self.sync()
    }

    /// Called after the existing family render. A placement edit does not touch
    /// family state, so its cached frame returns without drawing or readback.
    pub fn sync(&mut self) -> WidgetResult<()> {
        let revision = self.widget.revision();
        if self.rendered_revision == Some(revision) {
            return Ok(());
        }
        self.rendered_revision = Some(revision);
        let pixels_revision = match &self.widget {
            PreviewWidget::Raster(raster) => raster.pixels_revision,
            _ => revision,
        };
        if self.rendered_pixels_revision != Some(pixels_revision) {
            self.rendered_pixels_revision = Some(pixels_revision);
            self.pixels_revision = self.pixels_revision.wrapping_add(1);
        }
        self.place_geometry()?;
        self.revision = self.revision.wrapping_add(1);
        Ok(())
    }

    fn place_geometry(&mut self) -> WidgetResult<()> {
        if self.position == self.origin || self.widget.geometry().is_empty() {
            self.placed_geometry = None;
            return Ok(());
        }
        // Always translate the original frame, avoiding accumulated drag error.
        let mut geometry: PlacementGeometry = serde_json::from_slice(self.widget.geometry())
            .map_err(|error| WidgetError::Render(error.to_string()))?;
        let [dx, dy] = [
            self.position[0] - self.origin[0],
            self.position[1] - self.origin[1],
        ];
        geometry.widget.left = (geometry.widget.left as f64 + dx) as f32;
        geometry.widget.top = (geometry.widget.top as f64 + dy) as f32;
        geometry.widget.content.left = (geometry.widget.content.left as f64 + dx) as f32;
        geometry.widget.content.top = (geometry.widget.content.top as f64 + dy) as f32;
        self.placed_geometry = Some(
            serde_json::to_vec(&geometry)
                .map_err(|error| WidgetError::Render(error.to_string()))?,
        );
        Ok(())
    }

    pub fn pixels(&self) -> &Vec<u8> {
        self.widget.pixels()
    }
    pub fn geometry(&self) -> &Vec<u8> {
        self.placed_geometry
            .as_ref()
            .unwrap_or_else(|| self.widget.geometry())
    }
    pub fn revision(&self) -> u32 {
        self.revision
    }
    pub fn pixels_revision(&self) -> u32 {
        self.pixels_revision
    }
}
