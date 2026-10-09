//! Installs or removes an OCR model of the catalog from the command line, with the code behind the buttons in the
//! settings (staging directory, SHA-256 check, atomic commit). Developer tool; the network is used only by `download`.
//!
//!   cargo run -p lipa-core --example ocr_model -- list
//!   cargo run -p lipa-core --example ocr_model -- download meiki-ja
//!   cargo run -p lipa-core --example ocr_model -- remove meiki-ja
use lipa_core::ocr::rapid_models::{self, OcrModel};

fn find(id: Option<String>) -> Result<&'static OcrModel, String> {
    let id = id.ok_or("model id needed (see `list`)")?;
    rapid_models::catalog().iter().find(|m| m.id == id).ok_or_else(|| format!("no model «{id}» in the catalog"))
}

#[tokio::main]
async fn main() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let root = rapid_models::cache_root();
    match args.next().as_deref() {
        Some("list") => {
            for m in rapid_models::catalog() {
                println!("{:16} {:9} {:>6.1} МБ  {}  {}", m.id, m.engine, m.size() as f64 / 1e6, if rapid_models::verified(&root, m) { "installed" } else { "-" }, m.license);
            }
            Ok(())
        }
        Some("download") => {
            let model = find(args.next())?;
            let mut last = -1;
            let dir = rapid_models::download(model, &root, |p| {
                if p / 10 != last / 10 {
                    last = p;
                    eprintln!("{} {p} %", model.id);
                }
            }).await?;
            println!("{}", dir.display());
            Ok(())
        }
        Some("remove") => rapid_models::remove(&root, find(args.next())?),
        _ => Err("usage: ocr_model list | download ID | remove ID".into()),
    }
}
