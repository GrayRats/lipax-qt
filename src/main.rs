use std::process::Command;
use tokio;
use image::GenericImageView;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let slurp_output = Command::new("slurp")
        .args(["-d", "-b", "00000044", "-c", "ff0000ff", "-s", "00000000"])
        .output()?;

    if !slurp_output.status.success() {
        return Ok(());
    }

    let geometry = String::from_utf8(slurp_output.stdout)?.trim().to_string();

    let tmp_image = "/tmp/screenshot.png";
    let processed_image = "/tmp/screenshot_processed.png";
    
    let grim_status = Command::new("grim")
        .args(["-g", &geometry, tmp_image])
        .status()?;

    if !grim_status.success() {
        return Ok(());
    }

    let img = image::open(tmp_image)?;
    let (width, height) = img.dimensions();
    let img = img.resize_exact(width * 2, height * 2, image::imageops::FilterType::Lanczos3);
    let img = img.grayscale();
    img.save(processed_image)?;

    let tesseract_status = Command::new("tesseract")
        .args([processed_image, "/tmp/out", "-l", "rus+eng", "--psm", "6"])
        .status()?;

    if !tesseract_status.success() {
        return Ok(());
    }

    let recognized_text = std::fs::read_to_string("/tmp/out.txt")
        .unwrap_or_else(|_| "".to_string());
    
    let cleaned_text = recognized_text.trim();
    if cleaned_text.is_empty() {
        return Ok(());
    }

    let encoded_text = urlencoding::encode(cleaned_text);
    let url = format!(
        "https://translate.googleapis.com/translate_a/single?client=gtx&sl=auto&tl=ru&dt=t&q={}",
        encoded_text
    );

    let response = reqwest::get(&url).await?.text().await?;
    let parsed: serde_json::Value = serde_json::from_str(&response)?;
    
    let mut full_translation = String::new();
    if let Some(array) = parsed[0].as_array() {
        for item in array {
            if let Some(part) = item[0].as_str() {
                full_translation.push_str(part);
            }
        }
    }

    let final_translation = full_translation.trim();
    if !final_translation.is_empty() {
        // Убиваем старый quickshell, чтобы переменная окружения гарантированно обновилась
        let _ = Command::new("pkill").arg("quickshell").status();

        let _ = Command::new("quickshell")
            .env("LIPA_TEXT", final_translation)
            .spawn();
    }

    let _ = std::fs::remove_file(tmp_image);
    let _ = std::fs::remove_file(processed_image);
    let _ = std::fs::remove_file("/tmp/out.txt");

    Ok(())
}