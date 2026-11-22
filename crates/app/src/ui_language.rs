//! Which language the interface uses. The QML source strings are Russian; French, Chinese (Simplified), Japanese and
//! Spanish have their own catalogues (`i18n/lipax_<code>.ts`), every other language gets the English one. The language comes from the environment, in this order of priority:
//! `LIPAX_UI_LANGUAGE`, `LANGUAGE` (a colon-separated list, as set by KDE), `LC_MESSAGES`, `LANG`, `LC_ALL`.
//! `LC_ALL` is read last on purpose: many setups pin it to `C.UTF-8` for sorting while the user's language is in
//! `LANG`, and a value of `C`/`POSIX` says nothing about the language, so it is skipped.
//! With no usable value the interface stays Russian, as before.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiLanguage { Russian, English, French, Chinese, Japanese, Spanish }

const ORDER: [&str; 5] = ["LIPAX_UI_LANGUAGE", "LANGUAGE", "LC_MESSAGES", "LANG", "LC_ALL"];

fn language_of(value: &str) -> Option<UiLanguage> {
    // `LANGUAGE` is a list: the first entry that says something decides.
    value.split(':').map(str::trim).find_map(|entry| {
        let code = entry.split(['.', '@']).next().unwrap_or("").to_ascii_lowercase();
        match code.as_str() {
            "" | "c" | "posix" => None,
            c => Some(match c.split(['_', '-']).next().unwrap_or("") {
                "ru" => UiLanguage::Russian,
                "fr" => UiLanguage::French,
                "zh" => UiLanguage::Chinese, // Traditional Chinese users get the Simplified catalogue
                "ja" => UiLanguage::Japanese,
                "es" => UiLanguage::Spanish,
                _ => UiLanguage::English,
            }),
        }
    })
}

#[cfg(test)]
pub fn choose(var: impl Fn(&str) -> Option<String>) -> UiLanguage {
    choose_with_setting("auto", var)
}

/// `LIPAX_UI_LANGUAGE` wins, then the saved setting (`auto` = ask the environment), then the locale variables.
pub fn choose_with_setting(setting: &str, var: impl Fn(&str) -> Option<String>) -> UiLanguage {
    if let Some(explicit) = var(ORDER[0]).and_then(|v| language_of(&v)) { return explicit; }
    if matches!(setting, "ru" | "en" | "fr" | "zh_CN" | "ja" | "es")
        && let Some(chosen) = language_of(setting)
    {
        return chosen;
    }
    ORDER.iter().filter_map(|name| var(name)).find_map(|value| language_of(&value)).unwrap_or(UiLanguage::Russian)
}

impl UiLanguage {
    /// The `lipax_<code>` catalogue name; `None` for Russian, the language of the source.
    pub fn code(self) -> Option<&'static str> {
        match self {
            Self::Russian => None, Self::English => Some("en"), Self::French => Some("fr"),
            Self::Chinese => Some("zh_CN"), Self::Japanese => Some("ja"), Self::Spanish => Some("es"),
        }
    }
}

pub fn from_settings(setting: &str) -> UiLanguage { choose_with_setting(setting, |name| std::env::var(name).ok()) }

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    fn pick(pairs: &[(&str, &str)]) -> UiLanguage {
        let map: HashMap<String, String> = pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        choose(|name| map.get(name).cloned())
    }
    #[test]
    fn nothing_set_stays_russian() { assert_eq!(pick(&[]), UiLanguage::Russian); }
    #[test]
    fn russian_and_english_locales() {
        assert_eq!(pick(&[("LANG", "ru_RU.UTF-8")]), UiLanguage::Russian);
        assert_eq!(pick(&[("LANG", "en_US.UTF-8")]), UiLanguage::English);
        assert_eq!(pick(&[("LANG", "de_DE.UTF-8")]), UiLanguage::English);
    }
    #[test]
    fn a_saved_setting_beats_the_locale_but_not_the_explicit_variable() {
        let map: HashMap<String, String> = [("LANG", "ru_RU.UTF-8")].iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        let var = |n: &str| map.get(n).cloned();
        assert_eq!(choose_with_setting("auto", var), UiLanguage::Russian);
        assert_eq!(choose_with_setting("ja", var), UiLanguage::Japanese);
        assert_eq!(choose_with_setting("nonsense", var), UiLanguage::Russian, "unknown value: back to the locale");
        let explicit: HashMap<String, String> = [("LIPAX_UI_LANGUAGE", "fr")].iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        assert_eq!(choose_with_setting("ja", |n| explicit.get(n).cloned()), UiLanguage::French);
    }
    #[test]
    fn the_other_supported_languages() {
        assert_eq!(pick(&[("LANG", "fr_FR.UTF-8")]), UiLanguage::French);
        assert_eq!(pick(&[("LANG", "zh_CN.UTF-8")]), UiLanguage::Chinese);
        assert_eq!(pick(&[("LANGUAGE", "zh-Hans:en")]), UiLanguage::Chinese);
        assert_eq!(pick(&[("LANG", "ja_JP.UTF-8")]), UiLanguage::Japanese);
        assert_eq!(pick(&[("LANG", "es_MX.UTF-8")]), UiLanguage::Spanish);
        assert_eq!(pick(&[("LIPAX_UI_LANGUAGE", "fr")]).code(), Some("fr"));
        assert_eq!(UiLanguage::Russian.code(), None);
    }
    #[test]
    fn c_locale_in_lc_all_does_not_hide_the_users_language() {
        assert_eq!(pick(&[("LC_ALL", "C.UTF-8"), ("LANG", "ru_RU.UTF-8")]), UiLanguage::Russian);
        assert_eq!(pick(&[("LC_ALL", "C"), ("LANG", "en_US.UTF-8")]), UiLanguage::English);
        assert_eq!(pick(&[("LC_ALL", "C.UTF-8")]), UiLanguage::Russian, "only C: no information");
    }
    #[test]
    fn language_list_and_explicit_override() {
        assert_eq!(pick(&[("LANGUAGE", "ru:en"), ("LANG", "en_US.UTF-8")]), UiLanguage::Russian);
        assert_eq!(pick(&[("LANGUAGE", "en_GB:ru")]), UiLanguage::English);
        assert_eq!(pick(&[("LIPAX_UI_LANGUAGE", "en"), ("LANG", "ru_RU.UTF-8")]), UiLanguage::English);
        assert_eq!(pick(&[("LIPAX_UI_LANGUAGE", "ru"), ("LANG", "en_US.UTF-8")]), UiLanguage::Russian);
        assert_eq!(pick(&[("LANGUAGE", ""), ("LANG", "ru_RU.UTF-8")]), UiLanguage::Russian);
    }
}
