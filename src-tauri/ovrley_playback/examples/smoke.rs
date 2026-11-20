//! Step 1 smoke run: open a clip, wait for the first frame (`"size"` event), read its size and
//! rotation, tear down. No render target is set; MDK decodes without drawing.
//!
//! `MDK_SDK=… cargo run -p ovrley_playback --example smoke -- <clip>`; on Windows
//! `$MDK_SDK/bin/x64` must be on `PATH`.

use std::sync::mpsc;
use std::time::{Duration, Instant};

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info,mdk=debug"))
        .init();
    let path = std::env::args().nth(1).expect("usage: smoke <clip>");
    ovrley_playback::player::install_log_handler();
    log::info!("MDK {}", ovrley_playback::player::version());

    let (sender, receiver) = mpsc::channel();
    let decoders = ovrley_playback::player::decoders_from_env();
    log::info!("decoders {decoders:?}");
    let player = ovrley_playback::player::Player::new(
        &decoders,
        ovrley_playback::player::Callbacks {
            render: Box::new(|| {}),
            event: Box::new(move |event| {
                log::info!("event {event:?}");
                let _ = sender.send(event);
            }),
        },
    );
    let started = Instant::now();
    player.open(&path, 0);
    // Without a render target nothing pulls frames; playing paused-at-start still decodes one.
    player.set_state(ovrley_playback::player::State::Paused);

    let deadline = started + Duration::from_secs(10);
    let mut decoder = String::new();
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        match receiver.recv_timeout(left) {
            // MDK's `video.decoder` property is empty on Windows; the event names the decoder.
            Ok(ovrley_playback::player::Event::Media {
                category, detail, ..
            }) if category == "decoder.video" => decoder = detail,
            Ok(ovrley_playback::player::Event::Media {
                category, detail, ..
            }) if detail == "size" => {
                log::info!("first frame after {:?} ({category})", started.elapsed());
                break;
            }
            Ok(ovrley_playback::player::Event::Status { new, .. })
                if new & ovrley_playback::player::status::INVALID != 0 =>
            {
                panic!("MDK could not open {path}")
            }
            Ok(_) => {}
            Err(_) => panic!("no first frame within 10 s"),
        }
    }
    log::info!(
        "media {:?}",
        player.media_summary().expect("a video stream")
    );
    log::info!("frame rotation {:?}", player.frame_rotation());
    log::info!("video decoder {decoder:?}");
    drop(player);
    log::info!("torn down; waiting for the delayed delete");
    std::thread::sleep(Duration::from_millis(1500));
    log::info!("done");
}
