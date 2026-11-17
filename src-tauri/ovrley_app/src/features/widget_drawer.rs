//! Port of `features/widget-drawer` and the drawer part of `widget-icons.jsx` (D4): the catalog's
//! groups of widget buttons, each a complete creation selection or a list of them, and adding the
//! chosen widget.

use crate::i18n::catalog as translations;
use crate::lib::activity::resolve_available_metrics;
use crate::store::Store;
use ovrley_widgets::model::catalog::{catalog, IconRef, WidgetDefinition};
use ovrley_widgets::model::document::WidgetSelection;
use serde_json::Value;

/// One creation option of a button (`createMenuOptions`).
#[derive(Clone, Debug, PartialEq)]
pub struct DrawerOptionView {
    pub value: String,
    pub label: String,
    pub icon: IconRef,
    pub selection: WidgetSelection,
}

/// One button of the grid.
#[derive(Clone, Debug, PartialEq)]
pub struct DrawerItemView {
    pub key: String,
    pub icon: IconRef,
    pub short_name: String,
    /// The availability marker: one of the item's types is an available metric, or a label,
    /// backdrop or raster.
    pub available: bool,
    pub options: Vec<DrawerOptionView>,
}

/// A category with its buttons.
#[derive(Clone, Debug, PartialEq)]
pub struct DrawerGroupView {
    pub name: String,
    pub items: Vec<DrawerItemView>,
}

fn selection(kind: &str) -> WidgetSelection {
    WidgetSelection {
        kind: kind.into(),
        ..WidgetSelection::default()
    }
}

/// `createMenuOptions`: a type's displays, each display's modes expanded, or the type alone.
fn menu_options(
    definition: &WidgetDefinition,
    t: &dyn Fn(&str) -> String,
) -> Vec<DrawerOptionView> {
    let catalog = catalog();
    if definition.display_types.is_empty() {
        return vec![DrawerOptionView {
            value: definition.kind.clone(),
            label: String::new(),
            icon: definition.icon.clone(),
            selection: selection(&definition.kind),
        }];
    }
    let mut options = Vec::new();
    for value in &definition.display_types {
        let display = catalog.require_display(value);
        if display.modes.is_empty() {
            let icon = display
                .icon
                .clone()
                .unwrap_or_else(|| panic!("display {value} has no icon"));
            let selection = WidgetSelection {
                display_type: Some(value.clone()),
                ..selection(&definition.kind)
            };
            options.push(DrawerOptionView {
                value: value.clone(),
                label: t(definition.display_type_label_key(catalog, value)),
                icon,
                selection,
            });
            continue;
        }
        for mode in &display.modes {
            let label_key = mode
                .raw
                .get("labelKey")
                .and_then(Value::as_str)
                .unwrap_or_else(|| panic!("mode {} has no labelKey", mode.value));
            let icon: IconRef =
                serde_json::from_value(mode.raw.get("icon").cloned().unwrap_or(Value::Null))
                    .unwrap_or_else(|error| panic!("mode {} has no icon: {error}", mode.value));
            let selection = WidgetSelection {
                mode: Some(mode.value.clone()),
                ..selection(&definition.kind)
            };
            options.push(DrawerOptionView {
                value: mode.value.clone(),
                label: t(label_key),
                icon,
                selection,
            });
        }
    }
    options
}

/// `GROUPED_QUICKMENU_ITEMS` with the availability markers (`WidgetButtonGrid`). A drawer group
/// takes the place of its first member, so the drawer keeps the manifest's order.
pub fn drawer_view(store: &Store) -> Vec<DrawerGroupView> {
    let catalog = catalog();
    let locale = store.i18n.language;
    let t = |key: &str| translations().t(locale, key).to_string();
    let clips = &store.player.timeline.timeline().committed.clips;
    let mut available: Vec<String> =
        resolve_available_metrics(store.media.activity_summary.as_ref(), clips)
            .into_iter()
            .map(|metric| metric.attribute)
            .collect();
    available.extend(["label", "backdrop", "raster"].map(String::from));
    let mut groups: Vec<(String, DrawerGroupView)> = catalog
        .categories
        .iter()
        .map(|(category, name_key)| {
            (
                category.clone(),
                DrawerGroupView {
                    name: t(name_key),
                    items: Vec::new(),
                },
            )
        })
        .collect();
    let mut placed = Vec::new();
    for definition in catalog
        .widgets()
        .iter()
        .filter(|definition| definition.current)
    {
        let (category, item) = match &definition.drawer_group {
            Some(name) => {
                if placed.contains(name) {
                    continue;
                }
                placed.push(name.clone());
                let group = catalog
                    .drawer_groups
                    .iter()
                    .find(|group| &group.name == name)
                    .unwrap_or_else(|| panic!("drawer group {name} is not in the catalog"));
                let options: Vec<DrawerOptionView> = group
                    .options
                    .iter()
                    .map(|option| DrawerOptionView {
                        value: option.value.clone(),
                        label: t(&option.label_key),
                        icon: option.icon.clone(),
                        selection: WidgetSelection {
                            preset: option.preset.clone().unwrap_or_default(),
                            ..selection(&option.kind)
                        },
                    })
                    .collect();
                let item = DrawerItemView {
                    key: group.name.clone(),
                    icon: group.icon.clone(),
                    short_name: t(&group.short_name_key),
                    available: group
                        .options
                        .iter()
                        .any(|option| available.contains(&option.kind)),
                    options,
                };
                (group.category.clone(), item)
            }
            None => {
                let item = DrawerItemView {
                    key: definition.kind.clone(),
                    icon: definition.icon.clone(),
                    short_name: t(&definition.short_name_key),
                    available: available.contains(&definition.kind),
                    options: menu_options(definition, &t),
                };
                (definition.category.clone(), item)
            }
        };
        let group = groups
            .iter_mut()
            .find(|(name, _)| *name == category)
            .unwrap_or_else(|| panic!("category {category} is not in the catalog"));
        group.1.items.push(item);
    }
    groups
        .into_iter()
        .map(|(_, group)| group)
        .filter(|group| !group.items.is_empty())
        .collect()
}

/// `handleAddWidget`: adds the widget and dismisses an overlay drawer (a pinned one stays).
pub fn add_from_drawer(store: &mut Store, selection: &WidgetSelection) -> Result<String, String> {
    let id = crate::features::widget_editor::add_widget(store, selection)?;
    store.layout.dismiss_left_drawer_overlay();
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item<'a>(groups: &'a [DrawerGroupView], key: &str) -> &'a DrawerItemView {
        groups
            .iter()
            .flat_map(|group| &group.items)
            .find(|item| item.key == key)
            .unwrap_or_else(|| panic!("no drawer item {key}"))
    }

    #[test]
    fn the_drawer_groups_widgets_by_category_with_their_options() {
        let store = Store::default();
        let groups = drawer_view(&store);
        assert!(!groups.is_empty());
        assert!(groups
            .iter()
            .all(|group| !group.items.is_empty() && !group.name.is_empty()));
        let speed = item(&groups, "speed");
        assert!(speed.options.len() > 1, "speed offers its displays");
        assert!(speed
            .options
            .iter()
            .all(|option| option.selection.kind == "speed"
                && option.selection.display_type.is_some()));
        let label = item(&groups, "label");
        assert_eq!(label.options.len(), 1);
        assert!(label.available, "labels need no metric");
        assert!(
            !speed.available,
            "without an activity no metric is available"
        );
        let map = item(&groups, "map");
        assert!(map
            .options
            .iter()
            .any(|option| option.selection.kind == "course"));
        assert!(map
            .options
            .iter()
            .any(|option| option.selection.kind == "map" && !option.selection.preset.is_empty()));
        let lap_timer = groups.iter().flat_map(|group| &group.items).find(|item| {
            item.options
                .iter()
                .any(|option| option.selection.mode.is_some())
        });
        assert!(lap_timer.is_some(), "a display with modes offers each mode");
    }
}
