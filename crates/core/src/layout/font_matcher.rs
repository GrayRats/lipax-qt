//! Подбор шрифта из ограниченного реестра приложения для поля.
//!
//! Порядок: фильтр покрытия глифов (язык перевода) → кураторский список для категории (или,
//! если категория `Unknown`, — для типа блока и письменности) → оценка кандидатов по
//! категории, ширине, насыщенности, моноширинности, стилю → бонус за место в списке. Если
//! ни один доступный шрифт не покрывает перевод, выбор помечается как недоступный,
//! а движок пропускает поле с предупреждением.
//!
//! Выбор зависит от поля и языка перевода, но не от длины перевода: подбор выполняется
//! один раз, и результат блокируется за полем (см. `tracker`).

use super::font_classifier::FontAnalysis;
use super::font_database::{FontInfo, InstalledFontDatabase};
use super::{FontCategory, Script, TextBlockType};
use std::collections::BTreeMap;

// Кураторские списки: порядок — приоритет. Содержат только семейства из реестра приложения
// (`font_database::BUNDLED`); тест `lists_only_name_bundled_families` следит за этим.
pub const SANS: &[&str] = &["Inter", "Roboto", "Noto Sans", "Open Sans", "Fira Sans", "Montserrat"];
pub const SERIF: &[&str] = &["PT Serif", "Noto Serif", "Source Serif 4", "Literata", "Lora", "EB Garamond"];
pub const SLAB: &[&str] = &["Roboto Slab", "Bitter"];
pub const MONO: &[&str] = &["JetBrains Mono", "Source Code Pro", "Fira Code"];
pub const CONDENSED: &[&str] = &["Roboto Condensed", "Inter"];
/// CJK-шрифт один (Noto Sans CJK, все регионы); CJK-варианта с засечками в комплекте нет.
pub const CJK_SANS: &[&str] = &["Noto Sans CJK SC"];
pub const CJK_SERIF: &[&str] = &["Noto Sans CJK SC"];
/// Общий список для категории `Unknown`, если тип блока тоже неизвестен.
pub const GENERAL: &[&str] = &["PT Serif", "Roboto Slab", "Inter", "Noto Sans", "Noto Serif", "Roboto", "Open Sans",
    "Fira Sans", "Source Serif 4", "Montserrat", "Roboto Condensed", "JetBrains Mono", "Source Code Pro"];
const UI_SANS: &[&str] = &["Inter", "Noto Sans", "Roboto", "Open Sans", "Fira Sans"];
const DIALOGUE_SERIF: &[&str] = &["PT Serif", "Noto Serif", "Source Serif 4", "Literata", "Lora", "Inter"];
const SUBTITLE_SANS: &[&str] = &["Inter", "Noto Sans", "Roboto", "Open Sans"];

/// Пользовательские предпочтения: замена для категории и семейства, проверяемые первыми.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FontPreferences {
    pub category_overrides: BTreeMap<FontCategory, String>,
    pub preferred: Vec<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct FontCandidateScore {
    pub glyph_coverage: f32,
    pub language_match: f32,
    pub category_match: f32,
    pub block_type_match: f32,
    pub width_match: f32,
    pub weight_match: f32,
    pub monospace_match: f32,
    pub condensed_match: f32,
    pub style_match: f32,
    pub priority_bonus: f32,
}

impl FontCandidateScore {
    pub fn total(&self) -> f32 {
        self.glyph_coverage + self.language_match + 2.0 * self.category_match + self.block_type_match + self.width_match
            + self.weight_match + self.monospace_match + self.condensed_match + self.style_match + 3.0 * self.priority_bonus
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FontSelection {
    pub family: String,
    pub category: FontCategory,
    pub italic_available: bool,
    pub available_weights: Vec<super::FontWeight>,
    /// Не нашлось подходящего шрифта: движок не должен рисовать такое поле.
    pub generic: bool,
    /// Узкий вариант того же склада для слишком длинного перевода (если установлен).
    pub condensed_family: Option<String>,
    pub confidence: f32,
}

impl FontSelection {
    pub fn generic(category: FontCategory) -> Self {
        Self { family: category.generic_family().into(), category, italic_available: true,
            available_weights: super::FontWeight::ALL.to_vec(), generic: true, condensed_family: None, confidence: 0.1 }
    }
}

#[derive(Default)]
pub struct FontMatcher;

fn is_cjk_family(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.contains("cjk") || n.contains("source han") || [" sc", " tc", " jp", " kr"].iter().any(|s| n.ends_with(s))
}

impl FontMatcher {
    /// Кураторский список с учётом категории, типа блока и письменности перевода.
    pub fn priority_list(category: FontCategory, block_type: TextBlockType, script: Script) -> Vec<&'static str> {
        let base: &[&str] = match category {
            c if c.is_cjk() => if c == FontCategory::CjkSerif { CJK_SERIF } else { CJK_SANS },
            c if c.is_serif() => SERIF,
            FontCategory::SlabSerif => SLAB,
            FontCategory::Monospace => MONO,
            FontCategory::Condensed => CONDENSED,
            FontCategory::Unknown => match block_type {
                TextBlockType::UiLabel | TextBlockType::Button | TextBlockType::MenuItem | TextBlockType::Notification => UI_SANS,
                TextBlockType::Dialogue => DIALOGUE_SERIF,
                TextBlockType::Subtitle => SUBTITLE_SANS,
                _ => GENERAL,
            },
            _ => SANS,
        };
        let mut list: Vec<&str> = base.to_vec();
        if script.is_cjk() && !category.is_cjk() {
            // Латинский/кириллический образ при переводе на CJK: нужны CJK-шрифты того же склада.
            let cjk = if category.is_serif() || category == FontCategory::SlabSerif { CJK_SERIF } else { CJK_SANS };
            list = cjk.iter().chain(list.iter()).copied().collect();
        } else if !script.is_cjk() {
            // Для латиницы и кириллицы CJK-семейства из общего списка не предпочитаются.
            list.retain(|f| !is_cjk_family(f));
        }
        list
    }

    pub fn score(analysis: &FontAnalysis, block_type: TextBlockType, list: &[&str], font: &FontInfo) -> FontCandidateScore {
        let pos = list.iter().position(|f| f.eq_ignore_ascii_case(&font.family));
        let c = analysis.category;
        let same_group = (c.is_sans() && font.category.is_sans()) || (c.is_serif() && font.category.is_serif())
            || (c.is_cjk() && font.category.is_cjk());
        let ui = matches!(block_type, TextBlockType::UiLabel | TextBlockType::Button | TextBlockType::MenuItem | TextBlockType::Notification | TextBlockType::Subtitle);
        let weight = analysis.weight.closest(&font.available_weights);
        FontCandidateScore {
            glyph_coverage: 1.0,
            language_match: 1.0,
            category_match: if c == FontCategory::Unknown { 0.0 } else if font.category == c { 1.0 } else if same_group { 0.6 } else { 0.0 },
            block_type_match: if ui && font.category.is_sans() { 0.5 } else if block_type == TextBlockType::Dialogue && font.category.is_serif() { 0.3 } else { 0.0 },
            width_match: 1.0 - (font.average_glyph_width - analysis.width_ratio.clamp(0.3, 1.0)).abs().min(1.0),
            weight_match: if (weight.value() - analysis.weight.value()).abs() <= 100 { 0.5 } else { 0.0 },
            monospace_match: match (analysis.monospace, font.is_monospace) { (true, true) => 1.0, (false, false) => 0.3, (true, false) => -0.5, (false, true) => -1.0 },
            condensed_match: if analysis.condensed == font.is_condensed { 0.3 } else { -0.3 },
            style_match: if analysis.italic && font.italic_available { 0.3 } else { 0.0 },
            priority_bonus: pos.map(|p| 1.0 - p as f32 / list.len().max(1) as f32).unwrap_or(0.0),
        }
    }

    /// «<Семейство> Condensed», иначе любой узкий шрифт того же склада с нужными глифами.
    fn condensed_variant(f: &FontInfo, covering: &[&FontInfo]) -> Option<String> {
        if f.is_condensed { return None; }
        let own = format!("{} Condensed", f.family);
        covering.iter().find(|c| c.family.eq_ignore_ascii_case(&own))
            .or_else(|| covering.iter().find(|c| (c.is_condensed || c.category == FontCategory::Condensed)
                && (f.category.is_serif() == c.family.to_ascii_lowercase().contains("serif"))))
            .map(|c| c.family.clone())
    }

    pub fn select_font(&self, analysis: &FontAnalysis, script: Script, block_type: TextBlockType,
                       db: &InstalledFontDatabase, prefs: &FontPreferences) -> FontSelection {
        let lang = script.fontconfig_lang();
        // Никогда не выбирать шрифт, в котором нет глифов перевода.
        let covering: Vec<&FontInfo> = db.fonts().iter().filter(|f| f.covers(lang)).collect();
        let pick = |f: &FontInfo, confidence: f32| FontSelection {
            family: f.family.clone(), category: f.category, italic_available: f.italic_available,
            available_weights: f.available_weights.clone(), generic: false,
            condensed_family: Self::condensed_variant(f, &covering), confidence,
        };
        if let Some(f) = prefs.category_overrides.get(&analysis.category).and_then(|name| covering.iter().find(|f| f.family.eq_ignore_ascii_case(name))) {
            return pick(f, 1.0);
        }
        let mut list = Self::priority_list(analysis.category, block_type, script);
        let preferred: Vec<&str> = prefs.preferred.iter().map(String::as_str).collect();
        list = preferred.iter().chain(list.iter()).copied().collect();
        let best = covering.iter()
            .map(|f| (Self::score(analysis, block_type, &list, f).total(), *f))
            .max_by(|a, b| a.0.total_cmp(&b.0));
        match best {
            Some((score, f)) => pick(f, (score / 12.0).clamp(0.1, 1.0) * analysis.confidence.max(0.3)),
            None => {
                let category = if script.is_cjk() && !analysis.category.is_cjk() { FontCategory::CjkSans } else { analysis.category };
                FontSelection::generic(category)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::FontWeight;
    use crate::layout::font_database::tests::SAMPLE;

    #[test]
    fn every_cjk_script_uses_the_single_bundled_cjk_font() {
        let db = InstalledFontDatabase::bundled();
        for script in [Script::Japanese, Script::Korean, Script::ChineseSimplified, Script::ChineseTraditional] {
            for category in [FontCategory::CjkSans, FontCategory::Serif, FontCategory::Unknown] {
                let picked = FontMatcher.select_font(&analysis(category), script, TextBlockType::Subtitle, &db, &Default::default());
                assert_eq!((picked.family.as_str(), picked.generic), ("Noto Sans CJK SC", false), "{script:?} {category:?}");
            }
        }
    }

    #[test]
    fn lists_only_name_bundled_families() {
        let db = InstalledFontDatabase::bundled();
        for list in [SANS, SERIF, SLAB, MONO, CONDENSED, CJK_SANS, CJK_SERIF, GENERAL, UI_SANS, DIALOGUE_SERIF, SUBTITLE_SANS] {
            for family in list { assert!(db.find(family).is_some(), "{family} is not a bundled family"); }
        }
    }

    #[test]
    fn every_category_resolves_to_a_bundled_font_for_latin_and_cyrillic() {
        let db = InstalledFontDatabase::bundled();
        for category in [FontCategory::SansSerif, FontCategory::Serif, FontCategory::SlabSerif, FontCategory::Monospace,
                         FontCategory::Condensed, FontCategory::Display, FontCategory::Unknown] {
            for script in [Script::Latin, Script::Cyrillic] {
                let a = FontAnalysis { monospace: category == FontCategory::Monospace, condensed: category == FontCategory::Condensed, ..analysis(category) };
                let picked = FontMatcher.select_font(&a, script, TextBlockType::Dialogue, &db, &Default::default());
                assert!(!picked.generic, "{category:?} {script:?} fell back to a generic family");
                assert!(db.find(&picked.family).is_some_and(|f| f.covers(script.fontconfig_lang())));
            }
        }
        let slab = FontMatcher.select_font(&analysis(FontCategory::SlabSerif), Script::Cyrillic, TextBlockType::Dialogue, &db, &Default::default());
        assert!(["Roboto Slab", "Bitter"].contains(&slab.family.as_str()), "slab text gets a slab font, got {}", slab.family);
    }

    fn analysis(category: FontCategory) -> FontAnalysis {
        FontAnalysis { category, weight: FontWeight::Normal, italic: false, stroke_px: 2.0, cap_height_px: 20.0,
            width_ratio: 0.55, slant: 0.0, monospace: false, condensed: false, confidence: 0.9 }
    }

    fn select(category: FontCategory, script: Script, block: TextBlockType) -> String {
        let db = InstalledFontDatabase::bundled();
        FontMatcher.select_font(&analysis(category), script, block, &db, &FontPreferences::default()).family
    }

    #[test]
    fn unknown_dialogue_prefers_serif_and_ui_prefers_sans() {
        assert_eq!(select(FontCategory::Unknown, Script::Cyrillic, TextBlockType::Dialogue), "PT Serif");
        assert_eq!(select(FontCategory::Unknown, Script::Cyrillic, TextBlockType::Button), "Inter");
        let db = InstalledFontDatabase::bundled();
        let inter = FontMatcher.select_font(&analysis(FontCategory::Unknown), Script::Cyrillic, TextBlockType::Button, &db, &Default::default());
        assert_eq!(inter.condensed_family.as_deref(), Some("Roboto Condensed"), "narrow sans for overlong translations");
        assert_eq!(select(FontCategory::Unknown, Script::Cyrillic, TextBlockType::Unknown), "PT Serif", "general list starts with PT Serif");
    }

    #[test]
    fn categories_map_to_matching_families() {
        assert_eq!(select(FontCategory::SlabSerif, Script::Cyrillic, TextBlockType::Unknown), "Roboto Slab");
        let mut mono = analysis(FontCategory::Monospace);
        mono.monospace = true;
        let db = InstalledFontDatabase::bundled();
        assert_eq!(FontMatcher.select_font(&mono, Script::Latin, TextBlockType::Unknown, &db, &Default::default()).family, "JetBrains Mono");
    }

    #[test]
    fn translation_glyphs_are_always_covered() {
        // Перевод на русский: шрифт без кириллицы не выбирается, даже если он в списке.
        let only_latin = "Fancy Latin Only\ten|de\t0\t80\t100\t0\nPT Serif\ten|de\t0\t80\t100\t0\n";
        let db = InstalledFontDatabase::parse(only_latin);
        let s = FontMatcher.select_font(&analysis(FontCategory::Serif), Script::Cyrillic, TextBlockType::Dialogue, &db, &Default::default());
        assert!(s.generic, "no Cyrillic font installed → generic family");
        assert_eq!(s.family, "serif");
        // Перевод на японский — только CJK-шрифт.
        assert_eq!(select(FontCategory::SansSerif, Script::Japanese, TextBlockType::Subtitle), "Noto Sans CJK SC");
    }

    #[test]
    fn user_override_wins() {
        let db = InstalledFontDatabase::parse(SAMPLE);
        let mut prefs = FontPreferences::default();
        prefs.category_overrides.insert(FontCategory::Serif, "Inter".into());
        assert_eq!(FontMatcher.select_font(&analysis(FontCategory::Serif), Script::Cyrillic, TextBlockType::Dialogue, &db, &prefs).family, "Inter");
    }
}
