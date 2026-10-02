//! Ручная проверка portal-захвата: диалог выбора окна, затем кадр сохраняется в PNG (по умолчанию /tmp/lipa-portal.png).
use lipa_core::capture::portal::PortalCapture;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let out = std::env::args().nth(1).unwrap_or_else(|| "/tmp/lipa-portal.png".into());
    let cap = PortalCapture::new(String::new());
    eprintln!("Выберите окно в системном диалоге…");
    match cap.select_window().await {
        Ok(key) => eprintln!("выбрано: {key:?}"),
        Err(e) => return eprintln!("ошибка выбора: {e}"),
    }
    match cap.grab_full().await {
        Ok(img) => {
            img.save(&out).unwrap();
            eprintln!("кадр {}x{} сохранён в {out}", img.width(), img.height());
        }
        Err(e) => eprintln!("ошибка кадра: {e}"),
    }
    eprintln!("restore token: {:?}", cap.token_updates().borrow().as_str());
}
