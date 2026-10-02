//! Глобальные горячие клавиши через `org.kde.KGlobalAccel` (KDE Plasma).
//!
//! Приложение регистрирует действия в kglobalaccel, а нажатия приходят сигналом
//! `globalShortcutPressed` компонента. Wayland не даёт читать клавиатуру в обход композитора,
//! поэтому используется штатный механизм KDE; пользователь может переназначить клавиши
//! в «Системных настройках → Сочетания клавиш».

use crate::settings::Hotkeys;
use futures_util::StreamExt;
use tokio::sync::{mpsc, watch};
use zbus::zvariant::OwnedObjectPath;

const COMPONENT: &str = "lipa";
const COMPONENT_TITLE: &str = "LipaX";
/// `SetPresent`: действие считается присутствующим; сохранённая пользователем клавиша
/// из конфига kglobalaccel имеет приоритет над заданной нами (автозагрузка не отключена).
const SET_PRESENT: u32 = 2;
/// Не подгружать сохранённое значение: клавиша выбрана пользователем в настройках.
const NO_AUTOLOADING: u32 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyAction {
    Toggle,
    SelectRegion,
    TranslateOnce,
    ToggleOverlay,
    TogglePin,
}

impl HotkeyAction {
    const ALL: [HotkeyAction; 5] = [Self::Toggle, Self::SelectRegion, Self::TranslateOnce, Self::ToggleOverlay, Self::TogglePin];

    fn id(self) -> &'static str {
        match self {
            Self::Toggle => "toggle",
            Self::SelectRegion => "select_region",
            Self::TranslateOnce => "translate_once",
            Self::ToggleOverlay => "toggle_overlay",
            Self::TogglePin => "toggle_pin",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::Toggle => "Запустить / остановить слежение",
            Self::SelectRegion => "Выбрать область перевода",
            Self::TranslateOnce => "Перевести сейчас",
            Self::ToggleOverlay => "Показать / скрыть overlay",
            Self::TogglePin => "Закрепить / открепить перевод",
        }
    }

    fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|a| a.id() == id)
    }

    fn keys(self, h: &Hotkeys) -> &str {
        match self {
            Self::Toggle => &h.toggle,
            Self::SelectRegion => &h.select_region,
            Self::TranslateOnce => &h.translate_once,
            Self::ToggleOverlay => &h.toggle_overlay,
            Self::TogglePin => &h.toggle_pin,
        }
    }
}

/// «Ctrl+Alt+P» -> код клавиши Qt (`Qt::Key | Qt::Modifier`). Пустая строка или
/// нераспознанное сочетание дают `None`.
pub fn parse_key(s: &str) -> Option<i32> {
    const SHIFT: i32 = 0x0200_0000;
    const CTRL: i32 = 0x0400_0000;
    const ALT: i32 = 0x0800_0000;
    const META: i32 = 0x1000_0000;
    let mut mods = 0;
    let mut key = None;
    for part in s.split('+').map(str::trim).filter(|p| !p.is_empty()) {
        match part.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => mods |= CTRL,
            "alt" => mods |= ALT,
            "shift" => mods |= SHIFT,
            "meta" | "super" | "win" => mods |= META,
            p => {
                if key.is_some() {
                    return None;
                }
                key = Some(match p {
                    "space" => 0x20,
                    "esc" | "escape" => 0x0100_0000,
                    "return" | "enter" => 0x0100_0004,
                    "tab" => 0x0100_0001,
                    _ if p.len() == 1 && p.as_bytes()[0].is_ascii_alphanumeric() => p.to_ascii_uppercase().as_bytes()[0] as i32,
                    _ if p.len() >= 2 && p.starts_with('f') => match p[1..].parse::<i32>() {
                        Ok(n @ 1..=24) => 0x0100_0030 + n - 1,
                        _ => return None,
                    },
                    _ => return None,
                });
            }
        }
    }
    key.map(|k| k | mods)
}

#[derive(Debug, thiserror::Error)]
#[error("горячие клавиши недоступны: {0}")]
pub struct HotkeyError(String);

fn err(e: impl std::fmt::Display) -> HotkeyError {
    HotkeyError(e.to_string())
}

fn action_id(a: HotkeyAction) -> Vec<String> {
    vec![COMPONENT.into(), a.id().into(), COMPONENT_TITLE.into(), a.title().into()]
}

/// События слушателя.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyEvent {
    Pressed(HotkeyAction),
    /// Выбранное сочетание не удалось назначить: оно занято или непонятно.
    Conflict(HotkeyAction),
}

/// Сочетание → код Qt. Пустая строка означает «не назначено».
fn keys_of(a: HotkeyAction, h: &Hotkeys) -> Vec<i32> {
    parse_key(a.keys(h)).into_iter().collect()
}

/// Назначает клавиши в kglobalaccel. `force` — выбор пользователя из настроек: перекрывает сохранённое
/// значение (без автозагрузки). Занятое другим действием сочетание не назначается и попадает в результат:
/// kglobalaccel на такую попытку отвечает «успехом» и записывает неактивную клавишу, поэтому
/// владелец проверяется заранее через `action(key)`.
async fn bind(conn: &zbus::Connection, h: &Hotkeys, force: bool) -> Result<Vec<HotkeyAction>, HotkeyError> {
    let flags = if force { SET_PRESENT | NO_AUTOLOADING } else { SET_PRESENT };
    let call = |method: &'static str| (Some("org.kde.kglobalaccel"), "/kglobalaccel", Some("org.kde.KGlobalAccel"), method);
    let mut conflicts = vec![];
    for a in HotkeyAction::ALL {
        let (d, p, i, m) = call("doRegister");
        conn.call_method(d, p, i, m, &(action_id(a),)).await.map_err(err)?;
        let want = keys_of(a, h);
        if force {
            if let Some(&key) = want.first() {
                let (d, p, i, m) = call("action");
                let owner: Vec<String> = conn.call_method(d, p, i, m, &(key,)).await.map_err(err)?.body().deserialize().map_err(err)?;
                let ours = owner.first().map(String::as_str) == Some(COMPONENT) && owner.get(1).map(String::as_str) == Some(a.id());
                if !owner.is_empty() && !ours {
                    conflicts.push(a);
                    continue;
                }
            } else if !a.keys(h).trim().is_empty() {
                conflicts.push(a); // не удалось разобрать сочетание
                continue;
            }
        }
        let (d, p, i, m) = call("setShortcut");
        conn.call_method(d, p, i, m, &(action_id(a), want, flags)).await.map_err(err)?;
        if !force {
            // Действующая клавиша (с учётом переназначения в системных настройках) не должна принадлежать чужому действию.
            let (d, p, i, m) = call("shortcut");
            let keys: Vec<i32> = conn.call_method(d, p, i, m, &(action_id(a),)).await.map_err(err)?.body().deserialize().map_err(err)?;
            if let Some(&key) = keys.first() {
                let (d, p, i, m) = call("action");
                let owner: Vec<String> = conn.call_method(d, p, i, m, &(key,)).await.map_err(err)?.body().deserialize().map_err(err)?;
                if !owner.is_empty() && !(owner.first().map(String::as_str) == Some(COMPONENT) && owner.get(1).map(String::as_str) == Some(a.id())) {
                    conflicts.push(a);
                }
            }
        }
    }
    Ok(conflicts)
}

/// Регистрирует действия, пересылает нажатия и применяет смену клавиш из настроек на лету.
/// Работает, пока не закроется канал `tx`.
pub async fn listen(
    mut hotkeys: watch::Receiver<Hotkeys>,
    tx: mpsc::UnboundedSender<HotkeyEvent>,
) -> Result<(), HotkeyError> {
    let conn = zbus::Connection::session().await.map_err(err)?;
    let initial = hotkeys.borrow_and_update().clone();
    for a in bind(&conn, &initial, false).await? {
        let _ = tx.send(HotkeyEvent::Conflict(a));
    }

    let reply = conn
        .call_method(Some("org.kde.kglobalaccel"), "/kglobalaccel", Some("org.kde.KGlobalAccel"), "getComponent", &(COMPONENT,))
        .await
        .map_err(err)?;
    let path: OwnedObjectPath = reply.body().deserialize().map_err(err)?;
    let proxy = zbus::Proxy::new(&conn, "org.kde.kglobalaccel", path, "org.kde.kglobalaccel.Component").await.map_err(err)?;
    let mut signals = proxy.receive_signal("globalShortcutPressed").await.map_err(err)?;
    loop {
        tokio::select! {
            msg = signals.next() => {
                let Some(msg) = msg else { break };
                let Ok((component, action, _ts)) = msg.body().deserialize::<(String, String, i64)>() else { continue };
                if component != COMPONENT {
                    continue;
                }
                if let Some(a) = HotkeyAction::from_id(&action) {
                    if tx.send(HotkeyEvent::Pressed(a)).is_err() {
                        break;
                    }
                }
            }
            changed = hotkeys.changed() => {
                if changed.is_err() {
                    break;
                }
                let h = hotkeys.borrow_and_update().clone();
                for a in bind(&conn, &h, true).await? {
                    if tx.send(HotkeyEvent::Conflict(a)).is_err() {
                        return Ok(());
                    }
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_letters_and_modifiers() {
        assert_eq!(parse_key("Ctrl+Alt+P"), Some(0x0400_0000 | 0x0800_0000 | 'P' as i32));
        assert_eq!(parse_key("ctrl + shift + r"), Some(0x0400_0000 | 0x0200_0000 | 'R' as i32));
        assert_eq!(parse_key("Meta+F5"), Some(0x1000_0000 | 0x0100_0034));
    }

    #[test]
    fn rejects_garbage() {
        assert_eq!(parse_key(""), None);
        assert_eq!(parse_key("Ctrl+"), None);
        assert_eq!(parse_key("Ctrl+A+B"), None);
        assert_eq!(parse_key("Ctrl+F99"), None);
        assert_eq!(parse_key("Ctrl+Банан"), None);
    }
}
