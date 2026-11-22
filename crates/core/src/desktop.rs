//! What the application asks of the desktop about the game window, apart from the pixels (R-14).
//!
//! Everything compositor-specific sits behind a few seams: capturing the window ([`crate::capture::Capture`]), where the
//! window is and whether it is in use ([`DesktopBackend`]), the global shortcuts (KGlobalAccel, or the command line for
//! the compositors that cannot do it) and the overlay layer (layer-shell). The application asks this trait instead of
//! KWin, so another compositor is one more implementation, not a change of every caller. KWin is the only implementation
//! and the only one that has been run; [`support_for`] says that honestly for every other desktop.

use crate::capture::kwin::{KwinCapture, WindowGeometry};
use std::future::Future;

/// What is known about the game window at one moment, the same for every compositor.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct WindowState {
    /// The client rectangle on the desktop (logical px); `None` if the compositor does not tell or the window is unknown.
    pub geometry: Option<WindowGeometry>,
    /// The window has the focus (or a window of LipaX itself has it); `None` if not asked or not known.
    pub active: Option<bool>,
    /// The window is minimised: it has no picture worth reading.
    pub minimized: bool,
}

/// The compositor-specific part of "where is the game window and may the overlay be shown".
pub trait DesktopBackend: Send + Sync {
    /// The name for messages and the Status tab.
    fn name(&self) -> &'static str;

    /// The state of the window `uuid`; `want_active` asks for the focus too (it costs nothing on KWin but another
    /// compositor may need a round trip for it).
    fn window_state(&self, uuid: &str, want_active: bool) -> impl Future<Output = WindowState> + Send;
}

impl DesktopBackend for KwinCapture {
    fn name(&self) -> &'static str { "KWin" }

    async fn window_state(&self, uuid: &str, want_active: bool) -> WindowState {
        WindowState {
            geometry: self.window_geometry(uuid).await,
            active: if want_active { self.window_active(uuid).await } else { None },
            minimized: self.window_minimized(uuid).await.unwrap_or(false),
        }
    }
}

/// Whether the overlay may be shown and whether recognition is paused, for a window in `state`. Recognition stops while
/// the overlay is hidden for lack of focus, and always while the window is minimised: whatever the settings say, a
/// minimised window has no picture worth reading. An unknown focus counts as active (nothing to hide by).
pub fn overlay_policy(state: &WindowState) -> (bool, bool) {
    let allowed = state.active.unwrap_or(true);
    (allowed, !allowed || state.minimized)
}

/// How much of LipaX a desktop gets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SupportTier {
    /// Window capture, geometry, focus, overlay over the game, shortcuts: KWin.
    Full,
    /// Intended: window or region capture and a separate translation window, shortcuts through the command line.
    Partial,
    /// Intended: portal capture, a separate window, a manual region.
    PortalOnly,
    /// Not classified.
    Unknown,
}

/// What a desktop is promised, and whether that has been run.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Support {
    pub tier: SupportTier,
    /// The desktop as the environment names it.
    pub desktop: String,
    /// `true` only where the whole promise has been run on a real or nested compositor.
    pub verified: bool,
    pub note: &'static str,
}

/// The support tier for `XDG_CURRENT_DESKTOP` (a colon-separated list). Pure, so it can be tested; the promise per tier is
/// that of `docs/IMPROVEMENTS-ROADMAP.md` R-14. Only KWin is `verified`.
pub fn support_for(xdg_current_desktop: &str) -> Support {
    let names: Vec<String> = xdg_current_desktop.split(':').map(|n| n.trim().to_ascii_lowercase()).filter(|n| !n.is_empty()).collect();
    let has = |wanted: &[&str]| names.iter().any(|n| wanted.contains(&n.as_str()));
    let desktop = xdg_current_desktop.to_owned();
    if has(&["kde"]) {
        Support { tier: SupportTier::Full, desktop, verified: true,
            note: "KWin: захват окна, геометрия, фокус, перевод поверх игры и горячие клавиши (проверено во вложенном KWin; живая игра под Proton не проверялась)." }
    } else if has(&["sway", "hyprland", "niri", "labwc", "wayfire", "river"]) {
        Support { tier: SupportTier::Partial, desktop, verified: false,
            note: "Sway: окно выбирается по фокусу, геометрия и фокус из IPC, захват через grim; приложение целиком проверено в headless sway (кадры, распознавание, геометрия), оверлей поверх игры и физический экран не проверялись. Остальные композиторы этого семейства (Hyprland, niri и др.): бэкенда нет, не проверено; ожидаются portal, отдельное окно перевода и клавиши через командную строку (`lipax --toggle` и др.)." }
    } else if has(&["gnome", "ubuntu", "unity", "cosmic", "budgie", "pantheon", "x-cinnamon", "mate", "xfce"]) {
        Support { tier: SupportTier::PortalOnly, desktop, verified: false,
            note: "Не проверено: ожидается только захват через portal, отдельное окно перевода и ручная область; перевод поверх игры недоступен." }
    } else {
        Support { tier: SupportTier::Unknown, desktop, verified: false, note: "Окружение не распознано: поддержка не заявляется." }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fake(WindowState);
    impl DesktopBackend for Fake {
        fn name(&self) -> &'static str { "fake" }
        async fn window_state(&self, _: &str, want_active: bool) -> WindowState {
            WindowState { active: if want_active { self.0.active } else { None }, ..self.0 }
        }
    }

    /// The calling code is generic over the trait: any compositor that fills `WindowState` drives the overlay.
    async fn policy_of(desk: &impl DesktopBackend, want_active: bool) -> (bool, bool) {
        overlay_policy(&desk.window_state("w", want_active).await)
    }

    #[tokio::test]
    async fn a_backend_other_than_kwin_drives_the_same_overlay_policy() {
        let rect = Some(WindowGeometry { x: 10.0, y: 20.0, w: 800.0, h: 600.0 });
        let in_front = Fake(WindowState { geometry: rect, active: Some(true), minimized: false });
        assert_eq!(policy_of(&in_front, true).await, (true, false));
        let behind = Fake(WindowState { geometry: rect, active: Some(false), minimized: false });
        assert_eq!(policy_of(&behind, true).await, (false, true), "another window in front: hidden and paused");
        assert_eq!(policy_of(&behind, false).await, (true, false), "focus not asked (the option is off): nothing to hide by");
        let minimized = Fake(WindowState { geometry: rect, active: Some(true), minimized: true });
        assert_eq!(policy_of(&minimized, true).await, (true, true), "minimised: shown as is but never read");
        assert_eq!(in_front.name(), "fake");
    }

    #[test]
    fn only_kde_is_verified_and_every_other_desktop_is_an_honest_not_verified() {
        let kde = support_for("KDE");
        assert_eq!((kde.tier, kde.verified), (SupportTier::Full, true));
        for (name, tier) in [("sway", SupportTier::Partial), ("Hyprland", SupportTier::Partial), ("niri", SupportTier::Partial),
                             ("GNOME", SupportTier::PortalOnly), ("ubuntu:GNOME", SupportTier::PortalOnly), ("XFCE", SupportTier::PortalOnly),
                             ("", SupportTier::Unknown), ("something-new", SupportTier::Unknown)] {
            let support = support_for(name);
            assert_eq!(support.tier, tier, "{name}");
            assert!(!support.verified, "{name}: nothing but KWin has been run");
        }
        assert_eq!(support_for("KDE").desktop, "KDE");
    }
}
