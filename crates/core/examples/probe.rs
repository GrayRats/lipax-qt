//! Ручная проверка захвата KWin: выбрать окно кликом, сохранить кадр, прогнать OCR.
use lipa_core::capture::kwin::KwinCapture;
use lipa_core::ocr::{Ocr, Tesseract};
use lipa_core::settings::Settings;

#[tokio::main]
async fn main() {
    let out = std::env::args().nth(1).unwrap_or_else(|| "probe.png".into());
    let kwin = KwinCapture::connect().await.expect("D-Bus");
    println!("Кликните по окну (Esc — отмена)...");
    let key = match kwin.pick_window().await {
        Ok(Some(k)) => k,
        other => return println!("окно не выбрано: {other:?}"),
    };
    println!("окно: {key:?}, существует: {}", kwin.window_exists(&key.uuid).await);
    let t = std::time::Instant::now();
    match kwin.grab_window(&key.uuid).await {
        Ok(img) => {
            println!("кадр {}x{} за {:?}", img.width(), img.height(), t.elapsed());
            img.save(&out).expect("save");
            let text = Tesseract.recognize(&img, &Settings::default()).await;
            println!("OCR: {:?}", text.map(|s| s.chars().take(300).collect::<String>()));
        }
        Err(e) => println!("ошибка захвата: {e}"),
    }
}
