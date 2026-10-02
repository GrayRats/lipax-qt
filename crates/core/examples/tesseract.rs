//! Ручная проверка TesseractManager на текущей системе.
use lipa_core::tesseract::TesseractManager;

fn main() {
    let m = TesseractManager::system();
    let info = m.detect();
    println!("{}", serde_json::to_string_pretty(&info).unwrap());
    println!("{:?}", m.missing("jpn+deu", &info.languages));
}
