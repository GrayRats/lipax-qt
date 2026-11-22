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
        // LIPAX_GRAB_LOOP=N: time N further captures (R-13, P-09: the cost of the capture stage and of a re-capture during OCR).
        if let Some(n) = std::env::var("LIPAX_GRAB_LOOP").ok().and_then(|n| n.parse::<u32>().ok()) {
            let mut times: Vec<f64> = Vec::new();
            for _ in 0..n {
                let started = std::time::Instant::now();
                capture.grab_window(&uuid).await.expect("capture client area");
                times.push(started.elapsed().as_secs_f64() * 1000.0);
            }
            times.sort_by(f64::total_cmp);
            println!("grab-loop: n={n} min={:.1} ms median={:.1} ms p95={:.1} ms max={:.1} ms", times[0], times[times.len() / 2], times[(times.len() * 95 / 100).min(times.len() - 1)], times[times.len() - 1]);
        }
    }
    drop(capture);
    lipa_core::capture::shutdown_geometry().await;
}
