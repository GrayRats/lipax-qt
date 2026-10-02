//! Live check: cargo run -p lipa-core --example geometry -- <KWin UUID>
use lipa_core::capture::kwin::KwinCapture;
#[tokio::main]
async fn main() {
    let uuid = std::env::args().nth(1).expect("KWin window UUID");
    let capture = KwinCapture::connect().await.unwrap();
    let geometry = capture.window_geometry(&uuid).await;
    println!("client geometry: {geometry:?}");
    assert!(geometry.is_some(), "KWin client geometry unavailable");
    drop(capture);
    lipa_core::capture::shutdown_geometry().await;
}
