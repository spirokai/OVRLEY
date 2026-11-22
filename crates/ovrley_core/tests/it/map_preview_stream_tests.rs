//! The native editor preview's streamed map frames (phase 5 D9): the widget never waits for a
//! frame, keeps the last one on show until the next arrives, asks for one frame at a time, asks
//! again once the renderer is rebuilt, and shows its placeholder for a frame that failed.
use ovrley_core::fonts::NativeFonts;
use ovrley_map::{MapStyleBuildings, MapStyleCatalog, MapStyleId, MapStyleInfo};
use ovrley_widgets::activity::ActivitySampler;
use ovrley_widgets::error::WidgetResult;
use ovrley_widgets::render::preview::PreviewRasterScale;
use ovrley_widgets::widgets::map::preview::{
    MapFrameFactory, MapFramePixels, MapFrameProvider, MapFrameReply, MapFrameRequest, PreviewMap, PreviewMaps,
};
use serde_json::json;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};

type Answer = mpsc::Sender<Result<MapFramePixels, String>>;

/// Holds every request with its answer, for the test to answer when it likes.
#[derive(Default)]
struct HeldFrames {
    requests: Mutex<Vec<(MapFrameRequest, Answer, bool)>>,
    generation: AtomicU64,
}

impl MapFrameProvider for HeldFrames {
    fn frame(&self, request: &MapFrameRequest, spare: Option<Vec<u8>>) -> WidgetResult<MapFrameReply> {
        let (answer, reply) = mpsc::channel();
        self.requests.lock().unwrap().push((request.clone(), answer, spare.is_some()));
        Ok(reply)
    }

    fn generation(&self) -> u64 {
        self.generation.load(Ordering::SeqCst)
    }
}

struct Factory(Arc<HeldFrames>);

impl MapFrameFactory for Factory {
    fn provider(&self, _style: &MapStyleInfo, _sample_count: u32) -> Arc<dyn MapFrameProvider> {
        self.0.clone()
    }
}

fn solid(request: &MapFrameRequest) -> MapFramePixels {
    let (width, height) = (request.viewport.width_px(), request.viewport.height_px());
    MapFramePixels::new(width, height, [200, 40, 40, 255].repeat((width * height) as usize)).unwrap()
}

fn activity() -> ActivitySampler {
    let points = json!([[48.0, 12.0], [48.004, 12.002], [48.01, 12.005], [48.013, 12.02], [48.016, 12.018]]);
    let parsed = ovrley_core::activity::parse_activity_json(&json!({
        "sample_elapsed_seconds": [0.0, 1.0, 2.0, 3.0, 4.0], "trim_end_seconds": 4.0,
        "sample_distance_progress": [0.0, 0.25, 0.5, 0.75, 1.0],
        "sample_course_points": points, "course": points,
    })
    .to_string())
    .unwrap();
    ActivitySampler::from_json(&serde_json::to_vec(&parsed).unwrap()).unwrap()
}

fn map(frames: &Arc<HeldFrames>) -> PreviewMap {
    let style = MapStyleInfo { id: MapStyleId::new("liberty").unwrap(), label: "Liberty".into(), background: [0xf8, 0xf4, 0xf0], buildings: MapStyleBuildings::None };
    let maps = PreviewMaps { styles: Some(MapStyleCatalog::new(vec![style]).unwrap()), frames: Some(Arc::new(Factory(frames.clone()))) };
    let widget = serde_json::from_value(json!({
        "value": "map", "x": 40, "y": 40, "opacity": 1, "font": "Inter.ttf", "width": 120, "height": 80,
        "map_style": "liberty", "show_buildings": false, "show_paths": true, "show_labels": false, "local_names": false,
        "camera_mode": "follow", "orientation": "rider_up", "pitch": 0, "follow_zoom": 16, "fixed_zoom": 14,
        "fixed_pan_x": 0, "fixed_pan_y": 0, "msaa_samples": 4, "shape": "rectangle", "corner_radius": 0,
        "border_color": "#ffffff", "border_thickness": 0, "border_opacity": 1, "route_smoothing_m": 8,
        "show_full_activity": true, "line_width": 6, "completed_line_opacity": 100, "remaining_line_opacity": 60,
        "completed_line_color": "#ff4d1f", "remaining_line_color": "#1f7bff", "marker_variant": "single",
        "marker_variant_diameter": 44, "marker_size": 10, "marker_opacity": 100, "marker_color": "#ffffff"
    }))
    .unwrap();
    let scene = serde_json::from_value(json!({
        "width": 1920, "height": 1080, "scale": 1, "opacity": 1, "shadow_strength": 0, "shadow_distance": 0,
        "shadow_color": "#000000", "border_thickness": 0, "border_color": "#000000", "preview_duration_seconds": 4
    }))
    .unwrap();
    let fonts = [PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fonts")];
    PreviewMap::new(widget, scene, PreviewRasterScale::NATIVE, &maps, &NativeFonts(&fonts)).unwrap()
}

fn answer(frames: &HeldFrames, index: usize, result: impl FnOnce(&MapFrameRequest) -> Result<MapFramePixels, String>) {
    let requests = frames.requests.lock().unwrap();
    let (request, answer, _) = &requests[index];
    answer.send(result(request)).unwrap();
}

#[test]
fn streamed_frames_never_block_and_keep_the_last_frame_on_show() {
    let frames = Arc::new(HeldFrames::default());
    let mut map = map(&frames);
    let activity = activity();

    map.render(Some(&activity), 1.0).unwrap();
    let placeholder = (map.revision, map.pixels.clone());
    assert_eq!(frames.requests.lock().unwrap().len(), 1, "the first frame is asked for, and the placeholder shows meanwhile");
    map.render(Some(&activity), 1.0).unwrap();
    assert_eq!(map.revision, placeholder.0, "nothing arrived, so nothing redraws");

    answer(&frames, 0, |request| Ok(solid(request)));
    map.render(Some(&activity), 1.0).unwrap();
    assert_ne!(map.revision, placeholder.0, "an arriving frame redraws the same state");
    assert_ne!(map.pixels, placeholder.1);
    let shown = map.pixels.clone();

    map.render(Some(&activity), 2.0).unwrap();
    map.render(Some(&activity), 3.0).unwrap();
    {
        let requests = frames.requests.lock().unwrap();
        assert_eq!(requests.len(), 2, "one frame at a time");
        assert!(!requests[1].2, "the frame on show keeps its buffer");
    }
    answer(&frames, 1, |request| Ok(solid(request)));
    map.render(Some(&activity), 3.0).unwrap();
    assert_eq!(frames.requests.lock().unwrap().len(), 3, "the newest state follows once the previous frame is in");
    assert_eq!(map.pixels.len(), shown.len());

    answer(&frames, 2, |request| Ok(solid(request)));
    map.render(Some(&activity), 3.0).unwrap();
    assert_eq!(frames.requests.lock().unwrap().len(), 3, "a held frame is not asked for again");
    frames.generation.fetch_add(1, Ordering::SeqCst);
    map.render(Some(&activity), 3.0).unwrap();
    assert_eq!(frames.requests.lock().unwrap().len(), 4, "a rebuilt renderer draws the held frame again");

    answer(&frames, 3, |_| Err("tile unavailable".into()));
    map.render(Some(&activity), 3.0).unwrap();
    assert_eq!(map.pixels, placeholder.1, "a failed frame shows the placeholder");
    map.render(Some(&activity), 3.0).unwrap();
    assert_eq!(frames.requests.lock().unwrap().len(), 4, "and is not asked for again");
}
