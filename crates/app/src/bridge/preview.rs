//! Backdrop and OCR-preview files of the inspector.

use lipa_core::sync::LockExt;
use super::*;

pub(super) fn backdrop_path(region_id: &str, block: u64) -> std::path::PathBuf {
    let name: String = region_id.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-').collect();
    lipa_core::runtime_dir().join("lipa").join(format!("inplace-{}-{name}-{block}.png", std::process::id()))
}

/// Размытая подложка поля рядом с превью окна; ревизия в URL заставляет QML перечитать файл.
fn save_backdrop(region_id: &str, block: u64, revision: u64, img: &image::RgbaImage) -> Option<String> {
    let path = backdrop_path(region_id, block);
    std::fs::create_dir_all(path.parent()?).inspect_err(|e| tracing::error!(component = "inplace", error = %e, "Не удалось создать каталог подложек")).ok()?;
    img.save(&path).inspect_err(|e| tracing::error!(component = "inplace", error = %e, "Не удалось сохранить подложку")).ok()?;
    Some(format!("file://{}?r={revision}", path.display()))
}

/// The URL of the backdrop of a field at `revision`: the file written earlier, or written now (encoding a PNG takes
/// milliseconds, so the lock is not held while it is done).
pub(super) fn ensure_backdrop(shared: &Shared, region_id: &str, block: u64, revision: u64, img: &image::RgbaImage) -> Option<String> {
    let key = (region_id.to_owned(), block);
    if let Some((rev, url)) = shared.backdrops.lock_recover().get(&key) && *rev == revision { return Some(url.clone()); }
    let url = save_backdrop(region_id, block, revision, img)?;
    shared.backdrops.lock_recover().insert(key, (revision, url.clone()));
    Some(url)
}

/// Writes the backdrops of the fields of a frame that are new or changed. Runs off the GUI thread.
pub(super) fn write_backdrops(shared: &Shared, region_id: &str, frame: &InplaceFrame) {
    for b in &frame.blocks {
        let Some(img) = b.background.image.as_ref() else { continue };
        if ensure_backdrop(shared, region_id, b.id, b.revision, img).is_none() {
            tracing::warn!(target: "inplace.render", region = %region_id, block_id = b.id, "backdrop image could not be written");
        }
    }
}

pub(super) fn preview_dir() -> std::path::PathBuf { lipa_core::runtime_dir().join("lipa") }

pub(super) fn preview_prefix() -> String { format!("ocr-{}-", std::process::id()) }

pub(super) fn remove_preview_files() {
    let Ok(entries) = std::fs::read_dir(preview_dir()) else { return };
    let prefix = preview_prefix();
    for entry in entries.flatten().filter(|e| e.file_name().to_string_lossy().starts_with(&prefix)) {
        let _ = std::fs::remove_file(entry.path());
    }
}

/// Запись области для окна просмотра: кадр лежит в PNG рядом с превью окна, блоки — в JSON.
pub(super) fn preview_entry(region_id: &str, region_name: &str, preview: &lipa_core::pipeline::OcrPreview, url: &str, filtered_url: Option<&str>) -> serde_json::Value {
    serde_json::json!({
        "id": region_id,
        "name": region_name,
        "width": preview.image.width(),
        "height": preview.image.height(),
        "image": url,
        "filtered_image": filtered_url,
        "minimum_confidence": preview.minimum_confidence,
        "filters_auto": preview.filters_auto,
        "filters": { "binarize": preview.filters.binarize, "auto_invert": preview.filters.auto_invert, "sharpen": preview.filters.sharpen, "contrast": preview.filters.contrast, "filter_noise": preview.filter_noise },
        "original": preview.original,
        "translation": preview.translation,
        "phase": preview.phase.label(),
        "timings": preview.timings.iter().map(|t| serde_json::json!({
            "stage": t.stage, "last_ms": t.last_ms, "p50_ms": t.p50_ms, "p95_ms": t.p95_ms, "samples": t.samples,
        })).collect::<Vec<_>>(),
        "boxes": preview.boxes.iter().map(|b| serde_json::json!({
            "x": b.rect.x, "y": b.rect.y, "w": b.rect.w, "h": b.rect.h, "original": b.original, "translation": b.translation, "details": b.details, "block": b.block, "confidence": b.confidence,
        })).collect::<Vec<_>>(),
    })
}

/// Сохранить кадр и обновить JSON всех областей. `None` — окно просмотра уже закрыто или кадр не записан.
pub(super) fn publish_preview(shared: &Shared, region_id: String, region_name: String, preview: lipa_core::pipeline::OcrPreview) -> Option<String> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static REVISION: AtomicU64 = AtomicU64::new(0);
    if !shared.preview.load(Ordering::Relaxed) { return None; }
    let name: String = region_id.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-').collect();
    let path = preview_dir().join(format!("{}{name}.png", preview_prefix()));
    std::fs::create_dir_all(path.parent()?).ok()?;
    preview.image.save(&path).inspect_err(|e| tracing::warn!(target: "ocr.preview", error = %e, "preview frame could not be written")).ok()?;
    let revision = REVISION.fetch_add(1, Ordering::Relaxed);
    let url = format!("file://{}?r={revision}", path.display());
    // `_` is not allowed in the region part of the name, so this cannot be another region's file.
    let filtered_path = path.with_file_name(format!("{}{name}_filtered.png", preview_prefix()));
    let filtered_url = match &preview.filtered {
        Some(img) => img.save(&filtered_path).inspect_err(|e| tracing::warn!(target: "ocr.preview", error = %e, "filtered preview frame could not be written")).ok()
            .map(|()| format!("file://{}?r={revision}", filtered_path.display())),
        None => { let _ = std::fs::remove_file(&filtered_path); None }
    };
    let active: Vec<String> = shared.settings.borrow().capture_regions().into_iter().map(|r| r.id).collect();
    let mut entries = shared.preview_entries.lock_recover();
    // The window may have been closed while the frame was being encoded.
    if !shared.preview.load(Ordering::Relaxed) { return None; }
    entries.insert(region_id.clone(), preview_entry(&region_id, &region_name, &preview, &url, filtered_url.as_deref()));
    entries.retain(|id, _| active.contains(id));
    // Regions keep the order of the settings.
    let ordered: Vec<&serde_json::Value> = active.iter().filter_map(|id| entries.get(id)).collect();
    serde_json::to_string(&ordered).ok()
}
