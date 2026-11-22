//! The Phase 1 component showcase: `pnpm showcase:native`.
//!
//! With `OVRLEY_SHOWCASE_SNAPSHOT=<dir>`, it renders the page at a few scroll positions into
//! `<dir>/showcase-<n>.png`, then with the select list, popover and dialog open, and exits (`OVRLEY_SHOWCASE_SIZE=<width>x<height>` sets the size).

use std::path::PathBuf;

use slint::ComponentHandle;

fn main() -> Result<(), slint::PlatformError> {
    let window = ovrley_ui::ShowcaseWindow::new()?;
    ovrley_ui::install_component_logic(&window);
    if let Some(dir) = std::env::var_os("OVRLEY_SHOWCASE_SNAPSHOT").map(PathBuf::from) {
        if let Some((w, h)) = std::env::var("OVRLEY_SHOWCASE_SIZE")
            .ok()
            .and_then(|s| s.split_once('x').map(|(w, h)| (w.to_owned(), h.to_owned())))
        {
            window.window().set_size(slint::LogicalSize::new(
                w.parse().expect("width"),
                h.parse().expect("height"),
            ));
        }
        let weak = window.as_weak();
        slint::Timer::single_shot(std::time::Duration::from_millis(800), move || {
            snapshot(weak, dir, 0)
        });
    }
    window.run()
}

fn snapshot(weak: slint::Weak<ovrley_ui::ShowcaseWindow>, dir: PathBuf, index: usize) {
    let window = weak.unwrap();
    let pixels = window.window().take_snapshot().expect("snapshot");
    let path = dir.join(format!("showcase-{index}.png"));
    image::save_buffer(
        &path,
        pixels.as_bytes(),
        pixels.width(),
        pixels.height(),
        image::ColorType::Rgba8,
    )
    .expect("save png");
    println!("{}", path.display());
    let page_height = window
        .window()
        .size()
        .to_logical(window.window().scale_factor())
        .height;
    // After the pages: the select list, the popover and the dialog, opened over the overlays section.
    match index {
        11 => {
            slint::quit_event_loop().expect("quit");
            return;
        }
        9 => window.set_scroll_y(-page_height * 0.9),
        10 => window.invoke_focus_group_input(),
        7 => window.set_scroll_y(-2.0 * page_height * 0.9 - page_height * 0.45),
        8 => window.invoke_open_hex_picker(),
        3 => {
            window.set_scroll_y(-2.0 * page_height * 0.9);
            window.invoke_open_select();
        }
        4 => window.set_popover_open(true),
        5 => {
            window.set_popover_open(false);
            window.set_dialog_open(true);
        }
        6 => window.set_dialog_open(false),
        _ => window.set_scroll_y(-(index as f32 + 1.0) * page_height * 0.9),
    }
    slint::Timer::single_shot(std::time::Duration::from_millis(400), move || {
        snapshot(weak, dir, index + 1)
    });
}
