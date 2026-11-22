//! Live check of the sway backend (R-14 step 3): lists the windows, takes the first one whose class or title contains the
//! argument, prints its state and grabs it. Run by `tests/inplace/live/run_sway.sh` inside a headless sway.
//!
//!   sway_probe <class-or-title-part> <out.png>
use lipa_core::capture::{Capture, sway::SwayCapture};
use lipa_core::desktop::DesktopBackend;
use lipa_core::settings::NormRect;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let mut args = std::env::args().skip(1);
    let (needle, out) = (args.next().unwrap_or_default(), args.next().unwrap_or_else(|| "sway-capture.png".into()));
    let Some(sway) = SwayCapture::from_env() else { eprintln!("sway-probe: SWAYSOCK is not set"); std::process::exit(2) };
    let windows = sway.windows().await.unwrap_or_else(|e| { eprintln!("sway-probe: {e}"); std::process::exit(2) });
    for w in &windows { println!("window {} class={:?} title={:?} pid={} client={:?} focused={} visible={}", w.id, w.class, w.title, w.pid, w.client, w.focused, w.visible); }
    let Some(window) = windows.iter().find(|w| w.class.contains(&needle) || w.title.contains(&needle)) else { eprintln!("sway-probe: no window matches {needle:?}"); std::process::exit(2) };
    let key = window.key();
    println!("key: {key:?}");
    let state = sway.window_state(&key.uuid, true).await;
    println!("state: {state:?}");
    let whole = NormRect { x: 0.0, y: 0.0, w: 1.0, h: 1.0 };
    match sway.grab(&key, whole).await {
        Ok(image) => {
            image.save(&out).expect("save the capture");
            println!("capture: {}x{}", image.width(), image.height());
            let half = sway.grab(&key, NormRect { x: 0.0, y: 0.0, w: 0.5, h: 0.5 }).await.expect("a region");
            println!("region: {}x{}", half.width(), half.height());
        }
        Err(e) => { eprintln!("sway-probe: capture failed: {e}"); std::process::exit(1) }
    }
    let gone = sway.grab(&lipa_core::settings::WindowKey { uuid: "999999".into(), ..key }, whole).await;
    println!("unknown window: {}", if matches!(gone, Err(lipa_core::capture::CaptureError::WindowGone)) { "WindowGone" } else { "unexpected" });
}
