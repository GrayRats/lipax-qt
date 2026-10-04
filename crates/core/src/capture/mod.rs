//! Захват изображения области окна. Бэкенды (KWin ScreenShot2, xdg-desktop-portal)
//! реализуют [`Capture`]; pipeline от конкретного бэкенда не зависит.

mod geometry;
pub use geometry::shutdown as shutdown_geometry;
pub use geometry::{arm_floating_focus_restore, clear_floating_focus_restore};
pub mod kwin;
pub mod portal;
pub mod title_bar;

use crate::settings::{NormRect, Settings, WindowKey};
use image::DynamicImage;
use serde::Serialize;
use std::future::Future;

/// What a capture backend can do. Every layer asks this instead of checking which backend it is,
/// so a new backend only has to declare itself and the UI and the pipeline follow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct CaptureCapabilities {
    /// The client rectangle of the window on the desktop is known and tracked
    /// (region outlines, the frame around a freshly chosen window).
    pub window_geometry: bool,
    /// That rectangle is in global desktop coordinates, so something can be placed over the window.
    pub global_coordinates: bool,
    /// The frame holds only the client area: no title bar, border or shadow.
    pub exclude_decorations: bool,
    /// The translation can be drawn over the original text.
    pub inplace_overlay: bool,
    /// The buffer is pixel-exact, not rescaled by the compositor.
    pub native_resolution: bool,
}

impl CaptureCapabilities {
    /// KWin ScreenShot2 plus the KWin script that reports client geometry.
    pub const KWIN: Self = Self { window_geometry: true, global_coordinates: true, exclude_decorations: true, inplace_overlay: true, native_resolution: true };
    /// ScreenCast through xdg-desktop-portal: the compositor tells neither where the window is nor what is around it.
    pub const PORTAL: Self = Self { window_geometry: false, global_coordinates: false, exclude_decorations: false, inplace_overlay: false, native_resolution: false };
    /// The same, when the user says the window fills a whole monitor (a fullscreen game): the monitor is its
    /// geometry, there is no decoration around a fullscreen window, the stream may still be rescaled.
    pub const PORTAL_FULLSCREEN: Self = Self { window_geometry: true, global_coordinates: true, exclude_decorations: true, inplace_overlay: true, native_resolution: false };

    /// Why the translation cannot be drawn over the original text; `None` if it can.
    pub fn inplace_blocker(&self) -> Option<&'static str> {
        if !self.window_geometry || !self.global_coordinates {
            Some("положение окна на экране неизвестно, перевод показывается в окне перевода")
        } else if !self.inplace_overlay {
            Some("этот способ захвата не поддерживает перевод поверх оригинала, перевод показывается в окне перевода")
        } else {
            None
        }
    }

    /// Why region outlines cannot be drawn around the game; `None` if they can.
    pub fn frame_blocker(&self) -> Option<&'static str> {
        (!self.window_geometry || !self.global_coordinates).then_some("положение окна на экране неизвестно, рамка не показывается")
    }
}

/// Capabilities of the backend that serves `window` (the key decides, see [`AnyCapture`]); the user's
/// statement that a portal window fills its monitor (`Settings::portal_fills_monitor`) counts.
pub fn capabilities_for(window: &WindowKey, settings: &Settings) -> CaptureCapabilities {
    match (portal::is_portal_window(window), settings.portal_fills_monitor && !settings.overlay_screen.is_empty()) {
        (false, _) => CaptureCapabilities::KWIN,
        (true, false) => CaptureCapabilities::PORTAL,
        (true, true) => CaptureCapabilities::PORTAL_FULLSCREEN,
    }
}

/// Name of that backend for messages.
pub fn backend_name(window: &WindowKey) -> &'static str {
    if portal::is_portal_window(window) { "xdg-desktop-portal" } else { "KWin ScreenShot2" }
}

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

    /// What this backend can do for `window`, given the user's settings.
    fn capabilities(&self, window: &WindowKey, settings: &Settings) -> CaptureCapabilities;
}

impl<T: Capture> Capture for std::sync::Arc<T> {
    fn capabilities(&self, window: &WindowKey, settings: &Settings) -> CaptureCapabilities {
        self.as_ref().capabilities(window, settings)
    }

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
    fn capabilities(&self, window: &WindowKey, settings: &Settings) -> CaptureCapabilities { capabilities_for(window, settings) }

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
    fn the_backend_decides_what_the_ui_offers() {
        let kwin = WindowKey { uuid: "{abc}".into(), resource_class: "game".into(), caption: "g".into() };
        let settings = Settings::default();
        assert_eq!(capabilities_for(&kwin, &settings), CaptureCapabilities::KWIN);
        assert_eq!(capabilities_for(&portal::portal_window_key(), &settings), CaptureCapabilities::PORTAL);
        // A fullscreen game through the portal: the monitor is the window, so the translation can go over the text.
        let fullscreen = Settings { portal_fills_monitor: true, overlay_screen: "DP-1".into(), ..Settings::default() };
        assert_eq!(capabilities_for(&portal::portal_window_key(), &fullscreen), CaptureCapabilities::PORTAL_FULLSCREEN);
        assert_eq!(CaptureCapabilities::PORTAL_FULLSCREEN.inplace_blocker(), None);
        assert_eq!(CaptureCapabilities::PORTAL_FULLSCREEN.frame_blocker(), None);
        assert_eq!(capabilities_for(&kwin, &fullscreen), CaptureCapabilities::KWIN, "the statement is about portal windows only");
        assert_eq!((backend_name(&kwin), backend_name(&portal::portal_window_key())), ("KWin ScreenShot2", "xdg-desktop-portal"));
        assert_eq!(CaptureCapabilities::KWIN.inplace_blocker(), None);
        assert_eq!(CaptureCapabilities::KWIN.frame_blocker(), None);
        assert!(CaptureCapabilities::PORTAL.inplace_blocker().unwrap().contains("положение окна"));
        assert!(CaptureCapabilities::PORTAL.frame_blocker().is_some());
        // A backend that knows the geometry but cannot draw over the text says so itself.
        let no_inplace = CaptureCapabilities { inplace_overlay: false, ..CaptureCapabilities::KWIN };
        assert!(no_inplace.inplace_blocker().unwrap().contains("не поддерживает"));
        assert_eq!(no_inplace.frame_blocker(), None);
    }

    #[test]
    fn rect_px_scales_and_clamps() {
        let r = NormRect { x: 0.1, y: 0.5, w: 0.5, h: 0.25 };
        assert_eq!(rect_px(r, 1000, 800), (100, 400, 500, 200));
        let out = NormRect { x: 0.9, y: 0.9, w: 0.5, h: 0.5 };
        let (x, y, w, h) = rect_px(out, 100, 100);
        assert!(x + w <= 100 && y + h <= 100);
    }
}
