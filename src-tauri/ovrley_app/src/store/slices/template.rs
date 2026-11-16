//! Port of `store/slices/createTemplateSlice.js`, Phase 3 part. Global-default and aspect-ratio
//! actions arrive with scene settings in Phase 5; their fields are here because the preview and
//! the history need them.

use super::editor::{default_config, EditorSlice};
use crate::store::sync::sync_scene_timing_from_config;
use ovrley_core::normalize::template::{template_manifest, DurableTemplateState};
use ovrley_widgets::model::document::OverlayConfig;
use serde::Deserialize;
use serde_json::{Map, Value};
use std::rc::Rc;

/// Where the loaded template came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TemplateSource {
    /// A bundled template, by its list id.
    Bundled { template_id: String },
    /// A template file: a user template, an import or a save target.
    File { path: String },
}

/// Whether a listed template is bundled or the user's.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub enum TemplateKind {
    #[serde(rename = "built-in")]
    BuiltIn,
    #[serde(rename = "user")]
    User,
}

/// One entry of `backend_list_templates`.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TemplateDescriptor {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: TemplateKind,
    pub width: u32,
    pub height: u32,
    /// User templates only.
    #[serde(default)]
    pub path: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TemplateSlice {
    pub loaded_template_source: Option<TemplateSource>,
    pub templates: Vec<TemplateDescriptor>,
    pub global_defaults: Rc<Map<String, Value>>,
    pub aspect_ratio: String,
    pub last_saved_template_state: Option<DurableTemplateState>,
}

impl Default for TemplateSlice {
    fn default() -> Self {
        Self {
            loaded_template_source: None,
            templates: Vec::new(),
            global_defaults: Rc::new(template_manifest().globals.clone()),
            aspect_ratio: "16:9".into(),
            last_saved_template_state: None,
        }
    }
}

impl TemplateSlice {
    pub fn set_templates(&mut self, templates: Vec<TemplateDescriptor>) {
        self.templates = templates;
    }

    pub fn set_last_saved_template_state(&mut self, state: Option<DurableTemplateState>) {
        self.last_saved_template_state = state;
    }

    pub fn set_loaded_template_source(&mut self, source: Option<TemplateSource>) {
        self.loaded_template_source = source;
    }

    pub fn find_template(&self, id: &str) -> Option<&TemplateDescriptor> {
        self.templates.iter().find(|template| template.id == id)
    }
}

/// `createNewTemplate`: the default document and global defaults, no source, nothing saved.
pub fn create_new_template(editor: &mut EditorSlice, template: &mut TemplateSlice) {
    editor.config = Rc::new(default_config());
    template.global_defaults = Rc::new(template_manifest().globals.clone());
    template.aspect_ratio = "16:9".into();
    template.loaded_template_source = None;
    template.last_saved_template_state = None;
    sync_scene_timing_from_config(editor, None, true);
}

/// `hydrateTemplateState`: a loaded document, its global defaults and its source; the aspect
/// ratio stays.
pub fn hydrate_template_state(editor: &mut EditorSlice, template: &mut TemplateSlice, config: OverlayConfig, global_defaults: Map<String, Value>, source: Option<TemplateSource>) {
    editor.config = Rc::new(config);
    template.global_defaults = Rc::new(global_defaults);
    template.loaded_template_source = source;
    sync_scene_timing_from_config(editor, None, true);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn descriptors_parse_the_template_list() {
        let list: Vec<TemplateDescriptor> = serde_json::from_value(json!([
            {"id": "a.json", "name": "A", "type": "built-in", "width": 1920, "height": 1080},
            {"id": "user:b.json", "name": "B", "type": "user", "width": 0, "height": 0, "path": "/t/b.json"}
        ]))
        .unwrap();
        assert_eq!(list[0].kind, TemplateKind::BuiltIn);
        assert_eq!(list[1].path.as_deref(), Some("/t/b.json"));
    }

    #[test]
    fn a_new_template_resets_document_defaults_source_and_saved_state() {
        let mut editor = EditorSlice::default();
        let mut template = TemplateSlice::default();
        editor.set_config(OverlayConfig::from_value(&json!({"labels": [{"id": "a"}], "scene": {"start": 9}})).unwrap());
        editor.selected_second = 30.0;
        template.loaded_template_source = Some(TemplateSource::File { path: "/a.json".into() });
        template.aspect_ratio = "custom".into();
        create_new_template(&mut editor, &mut template);
        assert!(editor.config.labels.is_empty());
        assert_eq!(template.loaded_template_source, None);
        assert_eq!(template.aspect_ratio, "16:9");
        assert_eq!(editor.selected_second, 30.0, "the default scene has no start to move to");
    }

    #[test]
    fn hydration_keeps_the_aspect_ratio_and_moves_the_playhead_to_the_scene_start() {
        let mut editor = EditorSlice::default();
        let mut template = TemplateSlice { aspect_ratio: "custom".into(), ..TemplateSlice::default() };
        let config = OverlayConfig::from_value(&json!({"scene": {"start": 4, "end": 20}})).unwrap();
        hydrate_template_state(&mut editor, &mut template, config, Map::new(), Some(TemplateSource::Bundled { template_id: "a.json".into() }));
        assert_eq!(template.aspect_ratio, "custom");
        assert_eq!((editor.start_second, editor.end_second, editor.selected_second), (4.0, 20.0, 4.0));
    }
}
