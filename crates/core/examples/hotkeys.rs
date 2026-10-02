//! Ручная проверка: регистрирует горячие клавиши, печатает нажатия и через 2 с меняет одну из клавиш.
use lipa_core::{hotkeys, settings::Settings};
use std::time::Duration;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let (keep, hk) = tokio::sync::watch::channel(Settings::load().hotkeys);
    tokio::spawn(async move {
        if let Err(e) = hotkeys::listen(hk, tx).await {
            eprintln!("{e}");
        }
    });
    if std::env::args().nth(1).as_deref() == Some("rebind") {
        tokio::time::sleep(Duration::from_secs(2)).await;
        keep.send_modify(|h| h.translate_once = "Ctrl+Alt+J".into());
        tokio::time::sleep(Duration::from_secs(1)).await;
        keep.send_modify(|h| h.translate_once = "Ctrl+Alt+F".into()); // занято в системе
    }
    while let Some(a) = rx.recv().await {
        println!("{a:?}");
    }
    drop(keep);
}
