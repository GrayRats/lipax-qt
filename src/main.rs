use std::process::Command;
use tokio;
use image::GenericImageView;
use std::path::PathBuf;

// Ищем qml файл, чтобы запустить окошко
fn find_qml() -> PathBuf {
    let paths = [
        PathBuf::from("lipa.qml"),
        PathBuf::from("/usr/share/lipa/lipa.qml"),
        dirs::config_dir().map(|d| d.join("lipa/lipa.qml")).unwrap_or_default(),
    ];
    paths.into_iter().find(|p| p.exists()).unwrap_or_else(|| PathBuf::from("lipa.qml"))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Выделяем мышкой кусок экрана
    let slurp = Command::new("slurp")
        .args(["-d", "-b", "00000044", "-c", "ff0000ff", "-s", "00000000"])
        .output()?;
    if !slurp.status.success() { return Ok(()); }
    let geom = String::from_utf8(slurp.stdout)?.trim().to_string();

    // Фоткаем выделенную зону
    let img_path = "/tmp/lipa_img.png";
    if !Command::new("grim").args(["-g", &geom, img_path]).status()?.success() {
        return Ok(());
    }

    // Делаем картинку четче, чтобы текст лучше распознался
    let img = image::open(img_path)?;
    let (w, h) = img.dimensions();
    img.resize_exact(w * 2, h * 2, image::imageops::FilterType::Lanczos3)
        .grayscale()
        .save(img_path)?;

    // Читаем текст с картинки
    if !Command::new("tesseract").args([img_path, "/tmp/out", "-l", "rus+eng", "--psm", "6"]).status()?.success() {
        return Ok(());
    }

    let text = std::fs::read_to_string("/tmp/out.txt").unwrap_or_default();
    let cleaned = text.trim();
    if cleaned.is_empty() { return Ok(()); }

    // Переводим через гугл
    let url = format!(
        "https://translate.googleapis.com/translate_a/single?client=gtx&sl=auto&tl=ru&dt=t&q={}",
        urlencoding::encode(cleaned)
    );

    let resp = reqwest::get(&url).await?.text().await?;
    let json: serde_json::Value = serde_json::from_str(&resp)?;
    
    // Достаем готовый перевод
    let mut translation = String::new();
    if let Some(arr) = json[0].as_array() {
        for item in arr {
            if let Some(part) = item[0].as_str() { translation.push_str(part); }
        }
    }

    // Выводим результат на экран в окошке quickshell
    let final_text = translation.trim();
    if !final_text.is_empty() {
        let _ = Command::new("pkill").arg("quickshell").status();
        let _ = Command::new("quickshell")
            .arg("-p").arg(find_qml())
            .env("LIPA_TEXT", final_text)
            .spawn();
    }

    // Убираем за собой мусор
    let _ = std::fs::remove_file(img_path);
    let _ = std::fs::remove_file("/tmp/out.txt");

    Ok(())
}