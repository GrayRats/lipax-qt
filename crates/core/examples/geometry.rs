//! Live check: cargo run -p lipa-core --example geometry -- <KWin UUID> [capture.png]
//! Capture requires a desktop entry allowing ScreenShot2 for this example's executable.
use lipa_core::capture::kwin::KwinCapture;
#[tokio::main]
async fn main() {
    let uuid = std::env::args().nth(1).expect("KWin window UUID");
    let capture = KwinCapture::connect().await.unwrap();
    let geometry = capture.window_geometry(&uuid).await;
    println!("client geometry: {geometry:?}");
    assert!(geometry.is_some(), "KWin client geometry unavailable");
    println!("window frames: {:?}", capture.window_frames(&uuid).await);
    if let Some(out) = std::env::args().nth(2) {
        let frame = capture.grab_window(&uuid).await.expect("capture client area");
        println!("capture: {}x{}", frame.width(), frame.height());
        frame.save(out).expect("save capture");
    }
    drop(capture);
    lipa_core::capture::shutdown_geometry().await;
}
