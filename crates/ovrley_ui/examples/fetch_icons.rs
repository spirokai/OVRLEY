//! Generates `ui/components/icons.slint` from the lucide icons OVRLEY uses, fetched from Iconify
//! by the `iconify` crate at compile time. Run with `cargo run -p ovrley_ui --example fetch_icons`
//! and commit the generated file.
//!
//! Each icon becomes one path-commands string: the `d` of every `<path>` in order, with
//! `<circle>`, `<ellipse>` and `<rect>` converted to path commands. Lucide's few filled dots
//! (`r=".5"` with a fill) are drawn by the 2-unit stroke alone, which covers them entirely.

use std::fmt::Write as _;
use std::sync::LazyLock;

use regex::Regex;

const ICONS: &[(&str, &str)] = &[
    ("activity", iconify::svg!("lucide:activity")),
    ("align-center", iconify::svg!("lucide:align-center")),
    ("align-left", iconify::svg!("lucide:align-left")),
    ("align-right", iconify::svg!("lucide:align-right")),
    ("arrow-down", iconify::svg!("lucide:arrow-down")),
    ("arrow-up", iconify::svg!("lucide:arrow-up")),
    ("arrow-up-down", iconify::svg!("lucide:arrow-up-down")),
    ("ban", iconify::svg!("lucide:ban")),
    ("bell", iconify::svg!("lucide:bell")),
    ("blend", iconify::svg!("lucide:blend")),
    ("blocks", iconify::svg!("lucide:blocks")),
    ("calendar-days", iconify::svg!("lucide:calendar-days")),
    ("chart-line", iconify::svg!("lucide:chart-line")),
    (
        "chart-no-axes-gantt",
        iconify::svg!("lucide:chart-no-axes-gantt"),
    ),
    ("check", iconify::svg!("lucide:check")),
    ("chevron-down", iconify::svg!("lucide:chevron-down")),
    ("chevron-up", iconify::svg!("lucide:chevron-up")),
    ("chevrons-right", iconify::svg!("lucide:chevrons-right")),
    ("circle-alert", iconify::svg!("lucide:circle-alert")),
    ("circle-check", iconify::svg!("lucide:circle-check")),
    ("circle-gauge", iconify::svg!("lucide:circle-gauge")),
    ("clock-3", iconify::svg!("lucide:clock-3")),
    ("clock", iconify::svg!("lucide:clock")),
    ("compass", iconify::svg!("lucide:compass")),
    ("copy", iconify::svg!("lucide:copy")),
    ("corner-up-left", iconify::svg!("lucide:corner-up-left")),
    ("corner-up-right", iconify::svg!("lucide:corner-up-right")),
    ("database", iconify::svg!("lucide:database")),
    ("download", iconify::svg!("lucide:download")),
    ("external-link", iconify::svg!("lucide:external-link")),
    ("eye", iconify::svg!("lucide:eye")),
    ("eye-off", iconify::svg!("lucide:eye-off")),
    ("file-box", iconify::svg!("lucide:file-box")),
    ("file-plus-2", iconify::svg!("lucide:file-plus-2")),
    ("file-up", iconify::svg!("lucide:file-up")),
    ("files", iconify::svg!("lucide:files")),
    ("film", iconify::svg!("lucide:film")),
    ("flag", iconify::svg!("lucide:flag")),
    ("folder-kanban", iconify::svg!("lucide:folder-kanban")),
    ("folder-open", iconify::svg!("lucide:folder-open")),
    ("folder-search", iconify::svg!("lucide:folder-search")),
    ("gauge", iconify::svg!("lucide:gauge")),
    ("grid-3x3", iconify::svg!("lucide:grid-3x3")),
    ("grip-vertical", iconify::svg!("lucide:grip-vertical")),
    ("hash", iconify::svg!("lucide:hash")),
    ("heart-pulse", iconify::svg!("lucide:heart-pulse")),
    ("image-down", iconify::svg!("lucide:image-down")),
    ("image", iconify::svg!("lucide:image")),
    ("image-off", iconify::svg!("lucide:image-off")),
    ("info", iconify::svg!("lucide:info")),
    ("italic", iconify::svg!("lucide:italic")),
    ("keyboard", iconify::svg!("lucide:keyboard")),
    ("layout-grid", iconify::svg!("lucide:layout-grid")),
    ("loader-circle", iconify::svg!("lucide:loader-circle")),
    ("magnet", iconify::svg!("lucide:magnet")),
    ("map-pin", iconify::svg!("lucide:map-pin")),
    ("map", iconify::svg!("lucide:map")),
    ("menu", iconify::svg!("lucide:menu")),
    ("minus", iconify::svg!("lucide:minus")),
    ("mountain", iconify::svg!("lucide:mountain")),
    ("move", iconify::svg!("lucide:move")),
    ("navigation", iconify::svg!("lucide:navigation")),
    ("octagon-minus", iconify::svg!("lucide:octagon-minus")),
    ("palette", iconify::svg!("lucide:palette")),
    ("pause", iconify::svg!("lucide:pause")),
    ("pin-off", iconify::svg!("lucide:pin-off")),
    ("pin", iconify::svg!("lucide:pin")),
    ("pipette", iconify::svg!("lucide:pipette")),
    ("play", iconify::svg!("lucide:play")),
    ("plus", iconify::svg!("lucide:plus")),
    ("presentation", iconify::svg!("lucide:presentation")),
    ("redo-2", iconify::svg!("lucide:redo-2")),
    ("refresh-cw", iconify::svg!("lucide:refresh-cw")),
    ("rewind", iconify::svg!("lucide:rewind")),
    ("rotate-ccw", iconify::svg!("lucide:rotate-ccw")),
    ("rotate-cw-clock", iconify::svg!("lucide:rotate-cw-clock")),
    ("route", iconify::svg!("lucide:route")),
    ("rows-3", iconify::svg!("lucide:rows-3")),
    ("ruler", iconify::svg!("lucide:ruler")),
    ("save", iconify::svg!("lucide:save")),
    ("scissors", iconify::svg!("lucide:scissors")),
    ("settings-2", iconify::svg!("lucide:settings-2")),
    ("settings", iconify::svg!("lucide:settings")),
    ("shapes", iconify::svg!("lucide:shapes")),
    (
        "sliders-horizontal",
        iconify::svg!("lucide:sliders-horizontal"),
    ),
    ("sparkles", iconify::svg!("lucide:sparkles")),
    (
        "square-chevron-left",
        iconify::svg!("lucide:square-chevron-left"),
    ),
    ("square", iconify::svg!("lucide:square")),
    ("step-back", iconify::svg!("lucide:step-back")),
    ("step-forward", iconify::svg!("lucide:step-forward")),
    ("tags", iconify::svg!("lucide:tags")),
    ("target", iconify::svg!("lucide:target")),
    ("timer", iconify::svg!("lucide:timer")),
    ("trash-2", iconify::svg!("lucide:trash-2")),
    ("trending-up", iconify::svg!("lucide:trending-up")),
    ("triangle-alert", iconify::svg!("lucide:triangle-alert")),
    ("type", iconify::svg!("lucide:type")),
    ("undo-2", iconify::svg!("lucide:undo-2")),
    ("video", iconify::svg!("lucide:video")),
    ("volume-2", iconify::svg!("lucide:volume-2")),
    ("volume-x", iconify::svg!("lucide:volume-x")),
    ("x", iconify::svg!("lucide:x")),
    ("zoom-in", iconify::svg!("lucide:zoom-in")),
    ("zoom-out", iconify::svg!("lucide:zoom-out")),
];

static ELEMENT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"<(path|circle|ellipse|rect)\s([^>]*?)/?>").unwrap());
static ATTRIBUTE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"([a-z-]+)="([^"]*)""#).unwrap());
static NUMBER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[+-]?(?:[0-9]+\.?[0-9]*|\.[0-9]+)(?:[eE][+-]?[0-9]+)?").unwrap());

fn attribute(attributes: &str, name: &str) -> Option<f64> {
    ATTRIBUTE
        .captures_iter(attributes)
        .find(|caps| &caps[1] == name)
        .map(|caps| {
            caps[2]
                .parse()
                .unwrap_or_else(|_| panic!("attribute {name} is a number"))
        })
}

fn required(attributes: &str, name: &str) -> f64 {
    attribute(attributes, name)
        .unwrap_or_else(|| panic!("missing attribute {name} in {attributes}"))
}

/// A path's `d`, made safe to append after other paths: a leading relative `m` would start from
/// the previous path's end, so it becomes an absolute `M` (relative to the origin, where a lone
/// path starts) and its implicit line-to pairs become an explicit `l`.
fn standalone_path(d: &str) -> String {
    let d = d.trim();
    let Some(rest) = d.strip_prefix('m') else {
        return d.to_string();
    };
    let command_end = rest
        .find(|c: char| c.is_ascii_alphabetic() && c != 'e' && c != 'E')
        .unwrap_or(rest.len());
    let numbers: Vec<&str> = NUMBER
        .find_iter(&rest[..command_end])
        .map(|m| m.as_str())
        .collect();
    let mut out = format!("M{} {}", numbers[0], numbers[1]);
    if numbers.len() > 2 {
        write!(out, "l{}", numbers[2..].join(" ")).unwrap();
    }
    out.push_str(&rest[command_end..]);
    out
}

fn ellipse(cx: f64, cy: f64, rx: f64, ry: f64) -> String {
    // Two half arcs: a single 360° arc has no defined sweep.
    format!(
        "M{} {}a{rx} {ry} 0 1 0 {} 0a{rx} {ry} 0 1 0 {} 0",
        cx - rx,
        cy,
        2.0 * rx,
        -2.0 * rx
    )
}

fn rect(x: f64, y: f64, w: f64, h: f64, rx: f64, ry: f64) -> String {
    if rx == 0.0 {
        return format!("M{x} {y}h{w}v{h}h{}z", -w);
    }
    let (hw, hh) = (w - 2.0 * rx, h - 2.0 * ry);
    format!(
        "M{} {y}h{hw}a{rx} {ry} 0 0 1 {rx} {ry}v{hh}a{rx} {ry} 0 0 1 {} {ry}h{}a{rx} {ry} 0 0 1 {} {}v{}a{rx} {ry} 0 0 1 {rx} {}z",
        x + rx,
        -rx,
        -hw,
        -rx,
        -ry,
        -hh,
        -ry
    )
}

fn commands(svg: &str) -> String {
    ELEMENT
        .captures_iter(svg)
        .map(|caps| {
            let a = &caps[2];
            match &caps[1] {
                "path" => standalone_path(
                    &ATTRIBUTE
                        .captures_iter(a)
                        .find(|c| &c[1] == "d")
                        .expect("path has d")[2],
                ),
                "circle" => {
                    let r = required(a, "r");
                    ellipse(required(a, "cx"), required(a, "cy"), r, r)
                }
                "ellipse" => ellipse(
                    required(a, "cx"),
                    required(a, "cy"),
                    required(a, "rx"),
                    required(a, "ry"),
                ),
                "rect" => {
                    let rx = attribute(a, "rx").or(attribute(a, "ry")).unwrap_or(0.0);
                    let ry = attribute(a, "ry").unwrap_or(rx);
                    rect(
                        attribute(a, "x").unwrap_or(0.0),
                        attribute(a, "y").unwrap_or(0.0),
                        required(a, "width"),
                        required(a, "height"),
                        rx,
                        ry,
                    )
                }
                other => unreachable!("matched element {other}"),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn main() {
    let mut out = String::from(
        "// GENERATED by `cargo run -p ovrley_ui --example fetch_icons`. Do not edit.\n\
         //\n\
         // The lucide icons OVRLEY uses (Appendix A of the Phase 1 plan), fetched from Iconify. Each\n\
         // property is one icon's SVG path commands on lucide's 24×24 grid, named after the\n\
         // canonical lucide name.\n\
         import { Theme } from \"../theme/theme.slint\";\n\n\
         export global Icons {\n",
    );
    for (name, svg) in ICONS {
        writeln!(
            out,
            "    out property <string> {name}: \"{}\";",
            commands(svg)
        )
        .unwrap();
    }
    out.push_str(
        "}\n\n\
         // A lucide icon, stroked at lucide's width of 2 on the 24 grid, so strokes scale with the size\n\
         // as in the browser. Set `glyph` to an `Icons` property. It takes the height it is given and\n\
         // centres the glyph, so icons sit level with text in a row (layouts ignore `y`).\n\
         export component Icon {\n    \
             in property <string> glyph;\n    \
             in property <length> size: 1rem;\n    \
             in property <color> color: Theme.foreground;\n    \
             // lucide's stroke width on the 24 grid.\n    \
             in property <float> stroke: 2;\n    \
             width: root.size;\n    \
             min-height: root.size;\n    \
             preferred-height: root.size;\n    \
             max-height: 100000px;\n    \
             accessible-role: AccessibleRole.none;\n\n    \
             // The glyph snaps to the physical pixel grid, as a browser snaps an element's box: drawn at a\n    \
             // whole number of physical pixels, centred in the icon's box, from a physical pixel. Its box in\n    \
             // the layout keeps `size`.\n    \
             property <length> drawn: max(1phx, round(root.size / 1phx) * 1phx);\n    \
             property <length> left: round((root.absolute-position.x + (root.width - root.drawn) / 2) / 1phx) * 1phx - root.absolute-position.x;\n    \
             property <length> top: round((root.absolute-position.y + (root.height - root.drawn) / 2) / 1phx) * 1phx - root.absolute-position.y;\n\n    \
             Path {\n        \
                 x: root.left;\n        \
                 y: root.top;\n        \
                 width: root.drawn;\n        \
                 height: root.drawn;\n        \
                 viewbox-width: 24;\n        \
                 viewbox-height: 24;\n        \
                 commands: root.glyph;\n        \
                 fill: transparent;\n        \
                 stroke: root.color;\n        \
                 stroke-width: root.drawn * root.stroke / 24;\n        \
                 stroke-line-cap: round;\n        \
                 stroke-line-join: round;\n    \
             }\n\
         }\n",
    );
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("ui/components/icons.slint");
    std::fs::write(&path, out).expect("write icons.slint");
    println!("wrote {} icons to {}", ICONS.len(), path.display());
}
