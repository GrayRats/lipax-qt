//! Desktop notifications over `org.freedesktop.Notifications` (KDE shows them as its own popups).
//! Only failures of the pipeline are reported, and the same message not more often than once a minute:
//! a region that fails every second would otherwise cover the screen.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use zbus::zvariant::Value;

const REPEAT_AFTER: Duration = Duration::from_secs(60);
/// Remembered messages; the oldest are forgotten, so the map cannot grow without bound.
const REMEMBERED: usize = 64;

static LAST: Mutex<Option<HashMap<String, Instant>>> = Mutex::new(None);

/// Whether `key` may be shown now. Asking marks it as shown.
pub fn due(key: &str, now: Instant) -> bool {
    let mut guard = LAST.lock().unwrap();
    let map = guard.get_or_insert_with(HashMap::new);
    if map.get(key).is_some_and(|t| now.saturating_duration_since(*t) < REPEAT_AFTER) { return false; }
    if map.len() >= REMEMBERED { map.retain(|_, t| now.saturating_duration_since(*t) < REPEAT_AFTER); }
    if map.len() >= REMEMBERED { map.clear(); }
    map.insert(key.to_owned(), now);
    true
}

/// Show a notification; an error (no session bus, no notification service) is the caller's to log.
pub async fn send(summary: &str, body: &str) -> zbus::Result<()> {
    let connection = zbus::Connection::session().await?;
    connection.call_method(
        Some("org.freedesktop.Notifications"),
        "/org/freedesktop/Notifications",
        Some("org.freedesktop.Notifications"),
        "Notify",
        &("LipaX", 0u32, "io.lipa.Translator", summary, body, Vec::<&str>::new(), HashMap::<&str, Value<'_>>::new(), 8000i32),
    ).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_message_is_shown_once_a_minute() {
        let t0 = Instant::now();
        assert!(due("test-region\u{1}capture failed", t0));
        assert!(!due("test-region\u{1}capture failed", t0 + Duration::from_secs(30)));
        assert!(due("test-region\u{1}another failure", t0 + Duration::from_secs(30)), "a different message is not held back");
        assert!(due("test-region\u{1}capture failed", t0 + Duration::from_secs(61)), "and after a minute it may come again");
    }
}
