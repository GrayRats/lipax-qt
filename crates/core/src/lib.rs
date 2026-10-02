//! Ядро переводчика без зависимости от Qt: настройки, change detection,
//! сравнение текста, кэш. Остальные модули (capture, ocr, translate, pipeline)
//! добавляются поэтапно.

pub mod cache;
pub mod capture;
pub mod detect;
pub mod hotkeys;
pub mod layout;
pub mod ocr;
pub mod pipeline;
pub mod settings;
pub mod tesseract;
pub mod text;
pub mod translate;

pub mod history;
pub mod diagnostics;
