//! The declarative inspector (plan D1): every editor, display section and scene settings panel is
//! described as a tree of the JSX's own layout (`space-y-*` stacks, `grid grid-cols-n gap-*` grids,
//! `pt-*` padding, accent panels) holding cells, which are the Phase 3 form controls with their
//! props resolved. The tree flattens into keyed rows that one Slint component renders.
//!
//! Spacing is in rem, as Tailwind's classes give it. A row's `gap` is the space above it: the
//! stack's `space-y`, the grid's row gap, plus any `pt-*` of the element that starts there.

use crate::lib::fonts::FontOption;
use serde_json::Value;

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

/// How an edit of a cell turns into a widget patch (D1, D2). The plain rules patch the cell's own
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
    /// A number field through `setNumericField` with these options.
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

/// How a label-and-switch row is laid out; React styles each a little differently.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SwitchRowStyle {
    /// `TextDisplaySection`'s toggle rows: 0.741rem bold `tracking-wider`, `py-2`.
    Toggle,
    /// `flex items-center justify-between gap-2` with a 0.667rem uppercase bold label, its
    /// padding in rem (top, right, bottom, left) and the label's own top padding.
    Label { padding: [f32; 4], label_top: f32 },
    /// `flex h-9 items-center justify-between px-1` with a 9px label (the chart's area gradient).
    Tall,
}

impl SwitchRowStyle {
    /// `pl-1` (the map's layer toggles, the G-force invert switch, Renumber laps).
    pub const INLINE: Self = Self::Label { padding: [0.0, 0.0, 0.0, 0.25], label_top: 0.0 };
    /// `px-1` (Show Indicator, the heading tape's label toggles).
    pub const INLINE_X: Self = Self::Label { padding: [0.0, 0.25, 0.0, 0.25], label_top: 0.0 };
    /// `px-1 pt-2 pb-2` (Flat Track).
    pub const PADDED: Self = Self::Label { padding: [0.5, 0.25, 0.5, 0.25], label_top: 0.0 };
    /// `pl-1 pt-2 pb-2`, the label `pt-1` (Hundredths).
    pub const PADDED_LEFT: Self = Self::Label { padding: [0.5, 0.0, 0.5, 0.25], label_top: 0.25 };
    /// `px-1 pb-2` (the heading tape's tick toggles, the altitude tape's Decimal).
    pub const BOTTOM: Self = Self::Label { padding: [0.0, 0.25, 0.5, 0.25], label_top: 0.0 };
}

/// What a cell shows. Each kind is one of the Phase 3 controls or a bespoke cell (D3); the values
/// are what React passes as props, already resolved and translated.
#[derive(Clone, Debug, PartialEq)]
pub enum CellKind {
    /// `SliderField`: `readout` is the text without its suffix. The slider runs in field units;
    /// `scale` is `valueScale`, the inline input's display factor (percent sliders over 0–1).
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
    /// `ColorField`.
    Color { value: String },
    /// `SelectField`.
    Select {
        value: String,
        options: Vec<OptionView>,
        /// The label row\'s reset button (`onReset`).
        resettable: bool,
    },
    /// `NumberField`: the shown text, whole numbers unless the caller keeps decimals.
    Number {
        value: String,
        placeholder: String,
        suffix: String,
        resettable: bool,
    },
    /// `TextField`.
    Text { value: String, placeholder: String },
    /// `ToggleField`, a heading's trailing switch. With a label style it shows its label before it
    /// (`flex items-center gap-1.5`): `Inline` is 0.667rem (Track Style's Gradient), `Tall` 9px
    /// (the chart's Fill).
    Switch { checked: bool, label_style: Option<SwitchRowStyle> },
    /// A label and a switch filling the cell.
    SwitchRow {
        checked: bool,
        style: SwitchRowStyle,
    },
    /// `FontSelectField`: the bundled options for this value (`getFontSelectOptions`); the system
    /// fonts are shared by every font select.
    Font {
        value: String,
        recommended: Vec<FontOption>,
        /// A 9px label instead of 0.667rem.
        small_label: bool,
    },
    /// `FieldBlock` with a full-width single `ToggleGroup` (`h-7 text-xs` items).
    ToggleGroup {
        value: String,
        options: Vec<OptionView>,
        /// In a `FieldBlock` with its label; the heart zones' indicator group has none.
        labelled: bool,
        /// A multiple group: `pressed` lists the pressed values, and a click reports the item
        /// it toggles as text.
        multiple: bool,
        pressed: Vec<String>,
    },
    /// `ContentAlignmentControl`, a heading's trailing control. The options carry the items'
    /// accessible labels (left, centre, right).
    Alignment {
        value: String,
        options: Vec<OptionView>,
    },
    /// The label heading's italic toggle (a compact multiple `ToggleGroup` of one item).
    Italic { pressed: bool },
    /// An icon button: a heading's reset or add button.
    IconButton { icon: Glyph, outline: bool },
    /// `GradientPicker`.
    Gradient { stops: Vec<StopView> },
    /// The backdrop's `ShapeGrid` (D3), its toggles in `SHAPE_GRID_CELLS` order without the centre.
    ShapeGrid { toggles: Vec<ShapeToggle> },
    /// `ChartSeriesRow` (D3): the metric select, its colour and the move and remove buttons.
    ChartSeries {
        metric: String,
        options: Vec<OptionView>,
        color: String,
        first: bool,
        last: bool,
        only: bool,
    },
    /// `GForceAxisRow` (D3): the axis tabs and the invert switch.
    GForceAxis {
        axis: String,
        inverted: bool,
        invert_label: String,
    },
    /// The raster image picker (D5): the read-only path, the folder button and the error.
    RasterPicker {
        path: String,
        placeholder: String,
        button_label: String,
        loading: bool,
        error: String,
    },
    /// A destructive `Alert` with `TriangleAlert` (the map's GPU warning).
    Alert { text: String },
    /// The chart's window-seconds input (`useWindowSecondsInput`).
    WindowSeconds { text: String, invalid: bool },
    /// Scene settings: "Locked to N fps".
    Locked { text: String },
    /// Scene settings: the resolution mismatch note.
    Note { text: String },
    /// Scene settings: the widget update rate tabs.
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

/// `items-start`, `items-end`, or the grid's default (cells start at the top of their track).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Align {
    #[default]
    Start,
    End,
}

/// A heading: `SectionHeading` with its icon and title, and an optional trailing control.
#[derive(Clone, Debug, PartialEq)]
pub struct Heading {
    pub icon: Glyph,
    pub title: String,
    pub trailing: Option<Cell>,
}

/// One grid row: up to `columns` cells in equal columns, 1rem apart (`gap-4`) or 0.75rem (`gap-3`).
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub cells: Vec<Cell>,
    pub columns: u8,
    pub column_gap: f32,
    pub align: Align,
    /// `grid-cols-[auto_minmax(0,1fr)] h-32`: the first cell at its own width, the others stacked
    /// in the second column, each half of the 8rem height (the backdrop's shape grid).
    pub aside: bool,
}

/// Where a block sits in an accent panel (`rounded-xs bg-surface-accent-soft/70 p-4`).
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
    Heading(Heading),
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

/// The JSX's layout, as the descriptions write it.
#[derive(Clone, Debug, PartialEq)]
pub enum Node {
    /// A conditional element that renders nothing.
    Empty,
    Heading(Heading),
    /// `grid grid-cols-n gap-*`: cells fill rows of `columns`; a row gap separates the rows.
    Grid {
        columns: u8,
        gap: f32,
        align: Align,
        cells: Vec<Cell>,
    },
    /// `space-y-*` around its children (a fragment is a stack with the parent's spacing).
    Stack {
        space: f32,
        children: Vec<Node>,
    },
    /// Padding above and below the node (`pt-*`, `pb-*`), or a margin above it (`mb-*` of the
    /// previous element). The bottom padding adds to the space above whatever follows.
    Pad {
        top: f32,
        bottom: f32,
        node: Box<Node>,
    },
    /// The first cell beside a column of the others (`Row::aside`).
    Aside {
        cells: Vec<Cell>,
    },
    /// An accent panel with `space-y-*` between its children.
    Panel {
        space: f32,
        children: Vec<Node>,
    },
}

impl Node {
    /// `pt-*` (or a margin) above this node.
    pub fn pt(self, top: f32) -> Node {
        Node::Pad {
            top,
            bottom: 0.0,
            node: Box::new(self),
        }
    }

    /// `py-*`.
    pub fn py(self, padding: f32) -> Node {
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

/// `space-y-*`.
pub fn stack(space: f32, children: impl IntoIterator<Item = Node>) -> Node {
    Node::Stack {
        space,
        children: children.into_iter().collect(),
    }
}

/// `grid grid-cols-n gap-4`, skipping absent cells.
pub fn grid<C: Into<Option<Cell>>>(columns: u8, cells: impl IntoIterator<Item = C>) -> Node {
    Node::Grid {
        columns,
        gap: 1.0,
        align: Align::Start,
        cells: cells.into_iter().filter_map(Into::into).collect(),
    }
}

/// `grid grid-cols-n gap-4 items-end`.
pub fn grid_end<C: Into<Option<Cell>>>(columns: u8, cells: impl IntoIterator<Item = C>) -> Node {
    Node::Grid {
        columns,
        gap: 1.0,
        align: Align::End,
        cells: cells.into_iter().filter_map(Into::into).collect(),
    }
}

/// A grid with its own gap (`gap-3`).
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
    Node::Heading(Heading {
        icon,
        title: title.into(),
        trailing,
    })
}

/// `cond ? node : null`.
pub fn when(condition: bool, node: impl FnOnce() -> Node) -> Node {
    if condition {
        node()
    } else {
        Node::Empty
    }
}

/// Flattens the editor's top-level nodes, which sit in `space-y-*` of `space`.
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

/// JavaScript's `String(number)` for a readout: whole numbers without a fraction.
pub fn js_string(value: f64) -> String {
    crate::js::number_to_string(value)
}
