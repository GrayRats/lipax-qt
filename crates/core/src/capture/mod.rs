//! Захват изображения области окна. Бэкенды (KWin ScreenShot2, xdg-desktop-portal)
//! реализуют [`Capture`]; pipeline от конкретного бэкенда не зависит.

pub mod kwin;
pub mod portal;

use crate::settings::{NormRect, WindowKey};
use image::DynamicImage;
use std::future::Future;

#[derive(Debug, thiserror::Error)]
pub enum CaptureError {
    #[error("окно не найдено")]
    WindowGone,
    #[error("захват недоступен: {0}")]
    Unavailable(String),
}

pub trait Capture: Send + Sync {
    /// Кадр области `rect` (доли от геометрии окна) выбранного окна.
    fn grab(
        &self,
        window: &WindowKey,
        rect: NormRect,
    ) -> impl Future<Output = Result<DynamicImage, CaptureError>> + Send;
}

impl<T: Capture> Capture for std::sync::Arc<T> {
    fn grab(
        &self,
        window: &WindowKey,
        rect: NormRect,
    ) -> impl Future<Output = Result<DynamicImage, CaptureError>> + Send {
        self.as_ref().grab(window, rect)
    }
}

/// Оба бэкенда за одним `Capture`: какой использовать, решает ключ окна (`portal:window` — portal,
/// иначе uuid KWin). Поэтому смена бэкенда в настройках не ломает уже выбранное окно, а KWin
/// подключается только когда действительно нужен.
pub struct AnyCapture {
    kwin: tokio::sync::OnceCell<std::sync::Arc<kwin::KwinCapture>>,
    pub portal: portal::PortalCapture,
}

impl AnyCapture {
    pub fn new(portal_token: String) -> Self {
        Self { kwin: tokio::sync::OnceCell::new(), portal: portal::PortalCapture::new(portal_token) }
    }

    pub async fn kwin(&self) -> Result<std::sync::Arc<kwin::KwinCapture>, CaptureError> {
        self.kwin.get_or_try_init(|| async { kwin::KwinCapture::connect().await.map(std::sync::Arc::new) }).await.cloned()
    }

    /// Полный кадр окна (для выбора области).
    pub async fn grab_full(&self, window: &WindowKey) -> Result<DynamicImage, CaptureError> {
        if portal::is_portal_window(window) {
            self.portal.grab_full().await
        } else {
            self.kwin().await?.grab_window(&window.uuid).await
        }
    }
}

impl Capture for AnyCapture {
    async fn grab(&self, window: &WindowKey, rect: NormRect) -> Result<DynamicImage, CaptureError> {
        if portal::is_portal_window(window) {
            self.portal.grab(window, rect).await
        } else {
            self.kwin().await?.grab(window, rect).await
        }
    }
}

/// Перевод нормализованной области в пиксельный прямоугольник внутри кадра окна.
pub fn rect_px(rect: NormRect, width: u32, height: u32) -> (u32, u32, u32, u32) {
    let c = |v: f64| v.clamp(0.0, 1.0);
    let x = (c(rect.x) * width as f64).round() as u32;
    let y = (c(rect.y) * height as f64).round() as u32;
    let w = ((c(rect.w) * width as f64).round() as u32).min(width - x.min(width)).max(1);
    let h = ((c(rect.h) * height as f64).round() as u32).min(height - y.min(height)).max(1);
    (x.min(width.saturating_sub(1)), y.min(height.saturating_sub(1)), w, h)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_px_scales_and_clamps() {
        let r = NormRect { x: 0.1, y: 0.5, w: 0.5, h: 0.25 };
        assert_eq!(rect_px(r, 1000, 800), (100, 400, 500, 200));
        let out = NormRect { x: 0.9, y: 0.9, w: 0.5, h: 0.5 };
        let (x, y, w, h) = rect_px(out, 100, 100);
        assert!(x + w <= 100 && y + h <= 100);
    }
}
