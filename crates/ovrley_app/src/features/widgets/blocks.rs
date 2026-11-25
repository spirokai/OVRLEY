//! The declarative inspector: every editor, display section and scene settings panel is described
//! as a tree of layout nodes (stacks, grids, padding, accent panels) holding cells, which are the
//! form controls with their values resolved. The tree flattens into keyed rows that one Slint
//! component renders.
//!
//! Spacing is in rem. A row's `gap` is the space above it: the stack's spacing, the grid's row gap,
//! plus any top padding of the element that starts there.

use serde_json::Value;
use crate::features::widgets::FontOption;

/// A select, toggle-group or tab option.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OptionView {
    pub value: String,
    pub label: String,
}

impl OptionView {
    pub fn new(value: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
        }
    }
}

/// A lucide glyph of the `Icons` global, by its canonical name (`"sliders-horizontal"`).
pub type Glyph = &'static str;

/// How an edit of a cell turns into a widget patch. The plain rules patch the cell's own
/// field (its id is the field's JSON path); the named rules live in `rules.rs`.
#[derive(Clone, Debug, PartialEq)]
pub enum Rule {
    /// Not an edit (a heading's trailing control that reports through its own cell).
    None,
    /// The value goes to the field as it is (a string, bool or number).
    Field,
    /// A slider in display units: the field stores `value / divisor` (Transparency's percent).
    Scaled(f64),
    /// A select whose option values are numbers stored as numbers (MSAA samples).
    NumberChoice,
    /// A number field with these storage options.
    Numeric(super::NumericFieldOptions),
    /// A rule named by the editors, in `rules.rs`.
    Named(super::rules::Named),
}

/// A gradient stop as the picker edits it.
#[derive(Clone, Debug, PartialEq)]
pub struct StopView {
    pub offset: f64,
    pub color: String,
}

/// A toggle of the backdrop's shape grid (`ShapeToggleButton`): a corner or an edge strip.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShapeToggle {
    /// The field it toggles: a corner's key, or an edge (`top`, `right`, `bottom`, `left`).
    pub key: String,
    pub edge: bool,
    pub active: bool,
    pub label: String,
}

/// How a label-and-switch row is laid out.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SwitchRowStyle {
    /// A display section's toggle row: a 0.741rem bold, widely tracked label, 0.5rem above and below.
    Toggle,
    /// A 0.667rem uppercase bold label and the switch, 0.5rem apart, with the row's padding in rem
    /// (top, right, bottom, left) and the label's own top padding.
    Label { padding: [f32; 4], label_top: f32 },
    /// A 2.25rem tall row with a 9px label, 0.25rem side padding (the chart's area gradient).
    Tall,
}

impl SwitchRowStyle {
    /// 0.25rem left padding (the map's layer toggles, the G-force invert switch, Renumber laps).
    pub const INLINE: Self = Self::Label { padding: [0.0, 0.0, 0.0, 0.25], label_top: 0.0 };
    /// 0.25rem side padding (Show Indicator, the heading tape's label toggles).
    pub const INLINE_X: Self = Self::Label { padding: [0.0, 0.25, 0.0, 0.25], label_top: 0.0 };
    /// 0.25rem side and 0.5rem vertical padding (Flat Track).
    pub const PADDED: Self = Self::Label { padding: [0.5, 0.25, 0.5, 0.25], label_top: 0.0 };
    /// 0.25rem left and 0.5rem vertical padding, the label 0.25rem lower (Hundredths).
    pub const PADDED_LEFT: Self = Self::Label { padding: [0.5, 0.0, 0.5, 0.25], label_top: 0.25 };
    /// 0.25rem side and 0.5rem bottom padding (the heading tape's tick toggles, the altitude tape's
    /// Decimal).
    pub const BOTTOM: Self = Self::Label { padding: [0.0, 0.25, 0.5, 0.25], label_top: 0.0 };
}

/// What a cell shows. Each kind is one of the form controls or a bespoke cell; the values are
/// already resolved and translated.
#[derive(Clone, Debug, PartialEq)]
pub enum CellKind {
    /// `readout` is the text without its suffix. The slider runs in field units;
    /// `scale` is the inline input's display factor (percent sliders over 0–1).
    Slider {
        /// `editable`: the readout turns into an inline input; the altitude tape's interval is not.
        editable: bool,
        value: f64,
        min: f64,
        max: f64,
        step: f64,
        scale: f64,
        readout: String,
        suffix: String,
    },
    /// The color field.
    Color { value: String },
    /// The select field.
    Select {
        value: String,
        options: Vec<OptionView>,
        /// The label row's reset button.
        resettable: bool,
    },
    /// The shown text, whole numbers unless the caller keeps decimals.
    Number {
        value: String,
        placeholder: String,
        suffix: String,
        resettable: bool,
    },
    /// The text field.
    Text { value: String, placeholder: String },
    /// A heading's trailing switch. With a label style it shows its label before it: `Inline` is
    /// 0.667rem (Track Style's Gradient), `Tall` 9px (the chart's Fill).
    Switch { checked: bool, label_style: Option<SwitchRowStyle> },
    /// A label and a switch filling the cell.
    SwitchRow {
        checked: bool,
        style: SwitchRowStyle,
    },
    /// The bundled options for this value; the system
    /// fonts are shared by every font select.
    Font {
        value: String,
        recommended: Vec<FontOption>,
        /// A 9px label instead of 0.667rem.
        small_label: bool,
    },
    /// A full-width single toggle group under its label, with compact items.
    ToggleGroup {
        value: String,
        options: Vec<OptionView>,
        /// Under its label; the heart zones' indicator group has none.
        labelled: bool,
        /// A multiple group: `pressed` lists the pressed values, and a click reports the item
        /// it toggles as text.
        multiple: bool,
        pressed: Vec<String>,
    },
    /// The content alignment control, a heading's trailing control. The options carry the items'
    /// accessible labels (left, centre, right).
    Alignment {
        value: String,
        options: Vec<OptionView>,
    },
    /// The label heading's italic toggle (a compact multiple toggle group of one item).
    Italic { pressed: bool },
    /// An icon button: a heading's reset or add button.
    IconButton { icon: Glyph, outline: bool },
    /// The gradient picker.
    Gradient { stops: Vec<StopView> },
    /// The backdrop's shape grid, its toggles in grid order without the centre.
    ShapeGrid { toggles: Vec<ShapeToggle> },
    /// The metric select, its colour and the move and remove buttons.
    ChartSeries {
        metric: String,
        options: Vec<OptionView>,
        color: String,
        first: bool,
        last: bool,
        only: bool,
    },
    /// The G-force axis row: the axis tabs and the invert switch.
    GForceAxis {
        axis: String,
        inverted: bool,
        invert_label: String,
    },
    /// The raster image picker: the read-only path, the folder button and the error.
    RasterPicker {
        path: String,
        placeholder: String,
        button_label: String,
        loading: bool,
        error: String,
    },
    /// A destructive alert with a warning icon (the map's GPU warning).
    Alert { text: String },
    /// The chart's window-seconds input.
    WindowSeconds { text: String, invalid: bool },
    /// Scene settings: "Locked to N fps".
    Locked { text: String },
    /// Scene settings: the resolution mismatch note.
    Note { text: String },
    /// Scene settings: the widget update rate, its label beside the Gauge icon over the tabs.
    Tabs {
        value: String,
        options: Vec<OptionView>,
    },
    /// An empty grid cell (`<div />`).
    Empty,
}

/// One control and how its edits apply.
#[derive(Clone, Debug, PartialEq)]
pub struct Cell {
    /// The field's JSON path (`font_size`, `display_variants.arc.track_thickness`), or a stable
    /// name for a cell that is not one field (`series.0`, `shape`).
    pub id: String,
    pub label: String,
    pub disabled: bool,
    pub tooltip: String,
    /// `self-end`: the cell sits at the bottom of its row.
    pub end: bool,
    pub kind: CellKind,
    pub rule: Rule,
}

impl Cell {
    pub fn new(id: impl Into<String>, label: impl Into<String>, kind: CellKind) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            disabled: false,
            tooltip: String::new(),
            end: false,
            kind,
            rule: Rule::Field,
        }
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn rule(mut self, rule: Rule) -> Self {
        self.rule = rule;
        self
    }

    /// `self-end`.
    pub fn end(mut self) -> Self {
        self.end = true;
        self
    }

    /// An empty grid cell; `id` keeps the row's key unique.
    pub fn empty(id: impl Into<String>) -> Self {
        Self::new(id, "", CellKind::Empty).rule(Rule::None)
    }

    pub fn tooltip(mut self, tooltip: impl Into<String>) -> Self {
        self.tooltip = tooltip.into();
        self
    }
}

/// Top, bottom, or the grid's default (cells start at the top of their track).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Align {
    #[default]
    Start,
    End,
}

/// A section heading with its icon and title, and an optional trailing control.
#[derive(Clone, Debug, PartialEq)]
pub struct Heading {
    pub icon: Glyph,
    pub title: String,
    pub trailing: Option<Cell>,
    /// Scene settings' heading: 0.75rem gap, no vertical padding, the widest tracking, fully muted.
    pub scene: bool,
}

/// One grid row: up to `columns` cells in equal columns, 1rem apart or 0.75rem.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub cells: Vec<Cell>,
    pub columns: u8,
    pub column_gap: f32,
    pub align: Align,
    /// The first cell at its own width, the others stacked in the second column, each half of the
    /// 8rem height (the backdrop's shape grid).
    pub aside: bool,
}

/// Where a block sits in an accent panel.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PanelPart {
    #[default]
    None,
    First,
    Middle,
    Last,
    Only,
}

#[derive(Clone, Debug, PartialEq)]
pub enum BlockContent {
    Heading(Box<Heading>),
    Row(Row),
}

/// A flattened row of the inspector, keyed for the projection's diff.
#[derive(Clone, Debug, PartialEq)]
pub struct Block {
    pub key: String,
    /// The space above, in rem.
    pub gap: f32,
    pub panel: PanelPart,
    pub content: BlockContent,
}

// ── The layout tree ─────────────────────────────────────────────────────────────────────────────

/// A layout node, as the descriptions write it.
#[derive(Clone, Debug, PartialEq)]
pub enum Node {
    /// A conditional element that renders nothing.
    Empty,
    Heading(Box<Heading>),
    /// Cells fill rows of `columns`; a row gap separates the rows.
    Grid {
        columns: u8,
        gap: f32,
        align: Align,
        cells: Vec<Cell>,
    },
    /// Spacing between its children (a fragment is a stack with the parent's spacing).
    Stack {
        space: f32,
        children: Vec<Node>,
    },
    /// Padding above and below the node, or a margin above it (the previous element's bottom
    /// margin). The bottom padding adds to the space above whatever follows.
    Pad {
        top: f32,
        bottom: f32,
        node: Box<Node>,
    },
    /// The first cell beside a column of the others (`Row::aside`).
    Aside {
        cells: Vec<Cell>,
    },
    /// An accent panel with spacing between its children.
    Panel {
        space: f32,
        children: Vec<Node>,
    },
}

impl Node {
    /// Padding (or a margin) above this node.
    pub fn pad_top(self, top: f32) -> Node {
        Node::Pad {
            top,
            bottom: 0.0,
            node: Box::new(self),
        }
    }

    /// The same padding above and below this node.
    pub fn pad_y(self, padding: f32) -> Node {
        Node::Pad {
            top: padding,
            bottom: padding,
            node: Box::new(self),
        }
    }

    fn is_empty(&self) -> bool {
        match self {
            Node::Empty => true,
            Node::Heading(_) => false,
            Node::Aside { cells } => cells.is_empty(),
            Node::Grid { cells, .. } => cells.is_empty(),
            Node::Stack { children, .. } | Node::Panel { children, .. } => {
                children.iter().all(Node::is_empty)
            }
            Node::Pad { node, .. } => node.is_empty(),
        }
    }
}

/// A stack with `space` rem between its children.
pub fn stack(space: f32, children: impl IntoIterator<Item = Node>) -> Node {
    Node::Stack {
        space,
        children: children.into_iter().collect(),
    }
}

/// A grid of `columns` with a 1rem gap, skipping absent cells.
pub fn grid<C: Into<Option<Cell>>>(columns: u8, cells: impl IntoIterator<Item = C>) -> Node {
    Node::Grid {
        columns,
        gap: 1.0,
        align: Align::Start,
        cells: cells.into_iter().filter_map(Into::into).collect(),
    }
}

/// A grid of `columns` with a 1rem gap, its cells aligned to the bottom.
pub fn grid_end<C: Into<Option<Cell>>>(columns: u8, cells: impl IntoIterator<Item = C>) -> Node {
    Node::Grid {
        columns,
        gap: 1.0,
        align: Align::End,
        cells: cells.into_iter().filter_map(Into::into).collect(),
    }
}

/// A grid with its own gap.
pub fn grid_gap<C: Into<Option<Cell>>>(
    columns: u8,
    gap: f32,
    cells: impl IntoIterator<Item = C>,
) -> Node {
    Node::Grid {
        columns,
        gap,
        align: Align::Start,
        cells: cells.into_iter().filter_map(Into::into).collect(),
    }
}

/// One full-width cell (a field outside a grid).
pub fn full(cell: Cell) -> Node {
    Node::Grid {
        columns: 1,
        gap: 0.0,
        align: Align::Start,
        cells: vec![cell],
    }
}

pub fn heading(icon: Glyph, title: impl Into<String>, trailing: Option<Cell>) -> Node {
    Node::Heading(Box::new(Heading {
        icon,
        title: title.into(),
        trailing,
        scene: false,
    }))
}

/// Scene settings' section heading.
pub fn scene_heading(icon: Glyph, title: impl Into<String>, trailing: Option<Cell>) -> Node {
    Node::Heading(Box::new(Heading {
        icon,
        title: title.into(),
        trailing,
        scene: true,
    }))
}

/// `node` when `condition` holds, else nothing.
pub fn when(condition: bool, node: impl FnOnce() -> Node) -> Node {
    if condition {
        node()
    } else {
        Node::Empty
    }
}

/// Flattens the editor's top-level nodes, which sit in a stack of `space`.
pub fn flatten(space: f32, nodes: impl IntoIterator<Item = Node>) -> Vec<Block> {
    let mut out = Flat::default();
    out.stack(&nodes.into_iter().collect::<Vec<_>>(), space, 0.0, false);
    out.blocks
}

#[derive(Default)]
struct Flat {
    blocks: Vec<Block>,
    keys: std::collections::HashSet<String>,
    /// A bottom padding not yet added to the next block's gap.
    pending: f32,
}

impl Flat {
    /// Pushes a block; a key seen before gets a numeric suffix, so keys stay unique.
    fn push(&mut self, key: String, gap: f32, in_panel: bool, content: BlockContent) {
        let mut unique = key.clone();
        let mut index = 1;
        while !self.keys.insert(unique.clone()) {
            index += 1;
            unique = format!("{key}#{index}");
        }
        let panel = if in_panel {
            PanelPart::Middle
        } else {
            PanelPart::None
        };
        let gap = gap + std::mem::take(&mut self.pending);
        self.blocks.push(Block {
            key: unique,
            gap,
            panel,
            content,
        });
    }

    /// Lays out `nodes` with `space` between them; the first takes `incoming`.
    fn stack(&mut self, nodes: &[Node], space: f32, incoming: f32, in_panel: bool) {
        let mut first = true;
        for node in nodes.iter().filter(|node| !node.is_empty()) {
            let gap = if first { incoming } else { space };
            first = false;
            self.node(node, gap, in_panel);
        }
    }

    fn node(&mut self, node: &Node, gap: f32, in_panel: bool) {
        match node {
            Node::Empty => {}
            Node::Heading(heading) => self.push(
                format!("heading:{}", heading.title),
                gap,
                in_panel,
                BlockContent::Heading(heading.clone()),
            ),
            Node::Grid {
                columns,
                gap: row_gap,
                align,
                cells,
            } => {
                for (index, chunk) in cells.chunks(usize::from(*columns)).enumerate() {
                    let row = Row {
                        cells: chunk.to_vec(),
                        columns: *columns,
                        column_gap: if *row_gap == 0.0 { 1.0 } else { *row_gap },
                        align: *align,
                        aside: false,
                    };
                    self.push(
                        chunk[0].id.clone(),
                        if index == 0 { gap } else { *row_gap },
                        in_panel,
                        BlockContent::Row(row),
                    );
                }
            }
            Node::Stack { space, children } => self.stack(children, *space, gap, in_panel),
            Node::Aside { cells } => {
                let row = Row { cells: cells.clone(), columns: 2, column_gap: 1.0, align: Align::Start, aside: true };
                self.push(cells[0].id.clone(), gap, in_panel, BlockContent::Row(row));
            }
            Node::Pad { top, bottom, node } => {
                self.node(node, gap + top, in_panel);
                self.pending += bottom;
            }
            Node::Panel { space, children } => {
                let start = self.blocks.len();
                self.stack(children, *space, gap, true);
                let end = self.blocks.len();
                match end - start {
                    0 => {}
                    1 => self.blocks[start].panel = PanelPart::Only,
                    _ => {
                        self.blocks[start].panel = PanelPart::First;
                        self.blocks[end - 1].panel = PanelPart::Last;
                    }
                }
            }
        }
    }
}

// ── Reading a description ───────────────────────────────────────────────────────────────────────

/// Every cell of the blocks, heading controls included.
pub fn cells(blocks: &[Block]) -> impl Iterator<Item = &Cell> {
    blocks.iter().flat_map(|block| match &block.content {
        BlockContent::Heading(heading) => heading.trailing.iter().collect::<Vec<_>>(),
        BlockContent::Row(row) => row.cells.iter().collect(),
    })
}

/// The cell with this id.
pub fn find<'a>(blocks: &'a [Block], id: &str) -> Option<&'a Cell> {
    cells(blocks).find(|cell| cell.id == id)
}

// ── Value helpers for the descriptions ──────────────────────────────────────────────────────────

/// Reads a JSON path (`display_variants.arc.track_thickness`).
pub fn at<'a>(data: &'a serde_json::Map<String, Value>, path: &str) -> Option<&'a Value> {
    let mut parts = path.split('.');
    let mut value = data.get(parts.next()?)?;
    for part in parts {
        value = match value {
            Value::Object(map) => map.get(part)?,
            Value::Array(items) => items.get(part.parse::<usize>().ok()?)?,
            _ => return None,
        };
    }
    Some(value)
}
