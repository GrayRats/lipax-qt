//! Runtime style policy. Requested configuration is persisted independently of the loaded style.
use std::sync::{Mutex, OnceLock};
static STARTUP: OnceLock<(String, bool)> = OnceLock::new();
static ACTIVE_THEME: OnceLock<Mutex<String>> = OnceLock::new();
pub fn initialize(theme: &str) {
    let _ = STARTUP.set((theme.to_owned(), std::env::var_os("QT_QUICK_CONTROLS_STYLE").is_some()));
}
fn style(theme: &str) -> &str {
    match theme { "dark" | "light" => "Universal", "fusion" => "Fusion", "breeze" => "org.kde.breeze", _ => "system" }
}
fn policy(startup: &str, requested: &str, overridden: bool) -> serde_json::Value {
    let pending = !overridden && style(startup) != style(requested);
    serde_json::json!({"restartRequired": pending, "activeTheme": if pending { startup } else { requested },
        "styleOverridden": overridden})
}
pub fn theme_policy(requested: &str) -> serde_json::Value {
    let (startup, overridden) = STARTUP.get().cloned().unwrap_or(("dark".into(), false));
    let mut result = policy(&startup, requested, overridden);
    let mut active = ACTIVE_THEME.get_or_init(|| Mutex::new(startup))
        .lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if result["restartRequired"] == true {
        // Keep the last live palette, including dark/light changes made since startup.
        result["activeTheme"] = serde_json::Value::String(active.clone());
    } else {
        *active = requested.to_owned();
    }
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_style_transitions_wait_for_restart() {
        assert_eq!(policy("dark", "light", false)["restartRequired"], false);
        for (from, to) in [("fusion", "dark"), ("dark", "system"), ("breeze", "light")] {
            let p = policy(from, to, false);
            assert_eq!(p["restartRequired"], true);
            assert_eq!(p["activeTheme"], from);
            assert_eq!(policy(from, from, false)["restartRequired"], false);
        }
        assert_eq!(policy("dark", "fusion", true)["restartRequired"], false);
    }
}
