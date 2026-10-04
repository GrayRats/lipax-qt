//! Шрифты для режима поверх оригинала берутся из фиксированного реестра приложения.
//! Старый парсер fontconfig сохранён только для совместимости с тестовыми фикстурами;
//! `InplaceEngine` не опрашивает шрифты системы.

use super::{FontCategory, FontWeight};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, RwLock};

#[derive(Debug, Clone, PartialEq)]
pub struct FontInfo {
    pub family: String,
    pub supports_latin: bool,
    pub supports_cyrillic: bool,
    pub supports_cjk: bool,
    /// Языковые теги fontconfig, которые шрифт покрывает полностью.
    pub langs: BTreeSet<String>,
    pub category: FontCategory,
    pub available_weights: Vec<FontWeight>,
    pub italic_available: bool,
    pub is_condensed: bool,
    pub is_monospace: bool,
    /// Средняя ширина глифа в долях кегля (оценка по категории; точные метрики — у Qt).
    pub average_glyph_width: f32,
    pub x_height: Option<f32>,
}

impl FontInfo {
    pub fn covers(&self, lang: &str) -> bool {
        self.langs.contains(lang)
    }
}

/// Категория семейства по его имени.
pub fn category_of(family: &str) -> FontCategory {
    let f = family.to_ascii_lowercase();
    let has = |s: &str| f.contains(s);
    let cjk = has("cjk") || has("source han") || has(" sc") || has(" tc") || has(" jp") || has(" kr")
        || has("wenquanyi") || has("droid sans fallback") || has("takao") || has("ipa") || has("nanum");
    if cjk {
        return if has("serif") || has("mincho") || has("myungjo") || has("song") { FontCategory::CjkSerif } else { FontCategory::CjkSans };
    }
    if has("slab") || ["arvo", "bitter", "rockwell", "zilla"].iter().any(|n| has(n)) { return FontCategory::SlabSerif; }
    if has("mono") || has("code") || has("iosevka") || has("courier") || has("consolas") || has("cascadia") { return FontCategory::Monospace; }
    let serif_names = ["georgia", "merriweather", "lora", "garamond", "literata", "times", "baskerville", "playfair", "crimson", "bodoni", "didot"];
    if (has("serif") && !has("sans serif")) || serif_names.iter().any(|n| has(n)) {
        return if has("bodoni") || has("didot") || has("playfair") { FontCategory::ModernSerif } else { FontCategory::Serif };
    }
    if has("condensed") || has("narrow") { return FontCategory::Condensed; }
    if has("rounded") { return FontCategory::Rounded; }
    if ["montserrat", "poppins", "futura", "nunito"].iter().any(|n| has(n)) { return FontCategory::GeometricSans; }
    if ["fira sans", "source sans", "open sans", "ubuntu", "lato", "noto sans"].iter().any(|n| has(n)) { return FontCategory::HumanistSans; }
    if ["inter", "roboto", "helvetica", "arial", "liberation sans", "dejavu sans"].iter().any(|n| has(n)) { return FontCategory::NeoGrotesqueSans; }
    FontCategory::SansSerif
}

/// fontconfig: 0 thin, 40 extralight, 50 light, 80 regular, 100 medium, 180 demibold, 200 bold, 205 extrabold, 210 black.
fn weight_from_fc(w: i32) -> FontWeight {
    match w {
        w if w < 20 => FontWeight::Thin,
        w if w < 45 => FontWeight::ExtraLight,
        w if w < 65 => FontWeight::Light,
        w if w < 90 => FontWeight::Normal,
        w if w < 140 => FontWeight::Medium,
        w if w < 190 => FontWeight::DemiBold,
        w if w < 203 => FontWeight::Bold,
        w if w < 208 => FontWeight::ExtraBold,
        _ => FontWeight::Black,
    }
}

/// Формат строки `fc-list` для [`InstalledFontDatabase::parse`].
pub const FC_FORMAT: &str = "%{family[0]}\t%{lang}\t%{spacing}\t%{weight}\t%{width}\t%{slant}\n";

#[derive(Debug, Clone, Default)]
pub struct InstalledFontDatabase {
    fonts: Vec<FontInfo>,
}

static SYSTEM: RwLock<Option<Arc<InstalledFontDatabase>>> = RwLock::new(None);
/// Реестр с учётом того, что Qt реально загрузил; пересоздаётся при `set_bundled_available`.
static BUNDLED_CACHE: RwLock<Option<Arc<InstalledFontDatabase>>> = RwLock::new(None);
static BUNDLED_AVAILABLE: RwLock<Option<BTreeSet<String>>> = RwLock::new(None);

/// Шрифт, поставляемый с приложением. Единственный источник истины: из этой таблицы строятся
/// и выбор шрифта, и список в настройках; загрузчик Qt просто читает каталог со шрифтами.
/// Курсивных файлов нет (ради размера): курсив рисует Qt синтетическим наклоном.
#[derive(Debug, Clone, Copy)]
pub struct BundledFont {
    /// Имя семейства, как его сообщает Qt после загрузки файла.
    pub family: &'static str,
    pub files: &'static [&'static str],
    pub category: FontCategory,
    /// Языковые теги fontconfig, которые шрифт покрывает полностью.
    pub langs: &'static [&'static str],
    pub weights: &'static [FontWeight],
    pub monospace: bool,
    pub condensed: bool,
    /// Ширина букв к высоте прописных — тот же признак, что `FontFeatures::width_ratio`, измеренный
    /// на собственном рендере шрифта (`font_fixtures.rs`): значения сравнимы напрямую.
    pub glyph_width: f32,
}

use FontWeight::{Black, Bold, DemiBold, ExtraBold, ExtraLight, Light, Medium, Normal, Thin};
const LATIN: &[&str] = &["en", "ru", "uk", "el"];
const CJK: &[&str] = &["en", "ru", "ja", "ko", "zh-cn", "zh-tw"];
const W_ALL: &[FontWeight] = &[Thin, ExtraLight, Light, Normal, Medium, DemiBold, Bold, ExtraBold, Black];
const W_200_900: &[FontWeight] = &[ExtraLight, Light, Normal, Medium, DemiBold, Bold, ExtraBold, Black];
const W_300_800: &[FontWeight] = &[Light, Normal, Medium, DemiBold, Bold, ExtraBold];
const W_400_700: &[FontWeight] = &[Normal, Medium, DemiBold, Bold];
const W_400_800: &[FontWeight] = &[Normal, Medium, DemiBold, Bold, ExtraBold];
const W_100_800: &[FontWeight] = &[Thin, ExtraLight, Light, Normal, Medium, DemiBold, Bold, ExtraBold];
const W_300_700: &[FontWeight] = &[Light, Normal, Medium, DemiBold, Bold];
const W_REG_BOLD: &[FontWeight] = &[Normal, Bold];

use FontCategory::{CjkSans, Condensed, HumanistSans, GeometricSans, Monospace, NeoGrotesqueSans, OldStyleSerif, Serif, SlabSerif, TransitionalSerif};
macro_rules! font {
    ($family:literal, [$($file:literal),+], $cat:expr, $weights:expr, $width:expr $(, $flag:ident)?) => {
        BundledFont { family: $family, files: &[$($file),+], category: $cat, langs: LATIN, weights: $weights, glyph_width: $width,
            monospace: font!(@mono $($flag)?), condensed: font!(@cond $($flag)?) }
    };
    (@mono mono) => { true }; (@mono $($x:ident)?) => { false };
    (@cond condensed) => { true }; (@cond $($x:ident)?) => { false };
}

/// Набор независим от шрифтов системы: одинаковый выбор на любом дистрибутиве.
pub const BUNDLED: &[BundledFont] = &[
    font!("Inter", ["Inter.ttf"], NeoGrotesqueSans, W_ALL, 0.68),
    font!("Roboto", ["Roboto.ttf"], NeoGrotesqueSans, W_ALL, 0.65),
    font!("Noto Sans", ["NotoSans.ttf"], HumanistSans, W_ALL, 0.65),
    font!("Open Sans", ["OpenSans.ttf"], HumanistSans, W_300_800, 0.61),
    font!("Fira Sans", ["FiraSans-Regular.ttf", "FiraSans-Bold.ttf"], HumanistSans, W_REG_BOLD, 0.64),
    font!("Montserrat", ["Montserrat.ttf"], GeometricSans, W_ALL, 0.73),
    font!("Roboto Condensed", ["RobotoCondensed.ttf"], Condensed, W_ALL, 0.56, condensed),
    font!("Noto Serif", ["NotoSerif.ttf"], TransitionalSerif, W_ALL, 0.75),
    font!("PT Serif", ["PTSerif-Regular.ttf", "PTSerif-Bold.ttf"], TransitionalSerif, W_REG_BOLD, 0.65),
    font!("Source Serif 4", ["SourceSerif4.ttf"], Serif, W_200_900, 0.65),
    font!("Literata", ["Literata.ttf"], Serif, W_200_900, 0.74),
    font!("Lora", ["Lora.ttf"], Serif, W_400_700, 0.67),
    font!("EB Garamond", ["EBGaramond.ttf"], OldStyleSerif, W_400_800, 0.64),
    font!("Roboto Slab", ["RobotoSlab.ttf"], SlabSerif, W_ALL, 0.74),
    font!("Bitter", ["Bitter.ttf"], SlabSerif, W_ALL, 0.74),
    font!("JetBrains Mono", ["JetBrainsMono.ttf"], Monospace, W_100_800, 0.59, mono),
    font!("Fira Code", ["FiraCode.ttf"], Monospace, W_300_700, 0.64, mono),
    font!("Source Code Pro", ["SourceCodePro.ttf"], Monospace, W_200_900, 0.71, mono),
    BundledFont { family: "Noto Sans CJK SC", files: &["NotoSansCJK-VF.otf"], category: CjkSans, langs: CJK, weights: W_ALL,
        monospace: false, condensed: false, glyph_width: 1.0 },
];

fn bundled_info(f: &BundledFont) -> FontInfo {
    FontInfo {
        family: f.family.into(),
        supports_latin: true,
        supports_cyrillic: true,
        supports_cjk: f.langs.contains(&"ja"),
        langs: f.langs.iter().map(|l| (*l).to_owned()).collect(),
        category: f.category,
        available_weights: f.weights.to_vec(),
        italic_available: false,
        is_condensed: f.condensed,
        is_monospace: f.monospace,
        average_glyph_width: f.glyph_width,
        x_height: None,
    }
}

/// Семейства, реально загруженные Qt из комплекта приложения. До инициализации GUI используется
/// вся таблица (тесты ядра). Каждое незагруженное семейство — ошибка в журнале: без него выбор
/// шрифта молча стал бы беднее.
pub fn set_bundled_available(families: impl IntoIterator<Item = String>) {
    let loaded: BTreeSet<String> = families.into_iter().collect();
    for font in BUNDLED.iter().filter(|f| !loaded.contains(f.family)) {
        tracing::error!(target: "inplace.font", family = font.family, files = ?font.files, "bundled font family is not available");
    }
    *BUNDLED_AVAILABLE.write().unwrap() = Some(loaded);
    *BUNDLED_CACHE.write().unwrap() = None;
}

impl InstalledFontDatabase {
    /// Шрифты приложения. Результат общий и неизменяемый: повторные вызовы не пересобирают таблицу.
    pub fn bundled() -> Arc<Self> {
        if let Some(db) = BUNDLED_CACHE.read().unwrap().as_ref() { return db.clone(); }
        let loaded = BUNDLED_AVAILABLE.read().unwrap();
        let fonts = BUNDLED.iter()
            .filter(|f| loaded.as_ref().is_none_or(|set| set.contains(f.family)))
            .map(bundled_info).collect();
        let db = Arc::new(Self { fonts });
        *BUNDLED_CACHE.write().unwrap() = Some(db.clone());
        db
    }
}

impl InstalledFontDatabase {
    /// Разбор вывода `fc-list -f FC_FORMAT`: строки одного семейства объединяются.
    pub fn parse(text: &str) -> Self {
        let mut map: BTreeMap<String, FontInfo> = BTreeMap::new();
        for line in text.lines() {
            let f: Vec<&str> = line.split('\t').collect();
            if f.len() < 6 || f[0].trim().is_empty() { continue; }
            let family = f[0].trim().to_string();
            let category = category_of(&family);
            let e = map.entry(family.clone()).or_insert_with(|| FontInfo {
                family,
                supports_latin: false,
                supports_cyrillic: false,
                supports_cjk: false,
                langs: BTreeSet::new(),
                category,
                available_weights: Vec::new(),
                italic_available: false,
                is_condensed: false,
                is_monospace: false,
                average_glyph_width: match category { FontCategory::Condensed => 0.42, FontCategory::Monospace => 0.6, c if c.is_cjk() => 1.0, _ => 0.52 },
                x_height: None,
            });
            for lang in f[1].split('|').filter(|l| !l.is_empty()) { e.langs.insert(lang.to_string()); }
            if f[2].trim().parse::<i32>().is_ok_and(|s| s >= 90) { e.is_monospace = true; }
            if let Ok(w) = f[3].trim().trim_start_matches('[').split_whitespace().next().unwrap_or("").parse::<i32>() {
                let w = weight_from_fc(w);
                if !e.available_weights.contains(&w) { e.available_weights.push(w); }
            }
            if f[4].trim().trim_start_matches('[').split_whitespace().next().and_then(|v| v.parse::<i32>().ok()).is_some_and(|w| w < 100) {
                e.is_condensed = true;
            }
            if f[5].trim().parse::<i32>().is_ok_and(|s| s > 0) { e.italic_available = true; }
        }
        let fonts = map.into_values().map(|mut f| {
            f.supports_latin = f.covers("en");
            f.supports_cyrillic = f.covers("ru");
            f.supports_cjk = ["ja", "zh-cn", "zh-tw", "ko"].iter().any(|l| f.covers(l));
            if f.is_monospace && f.category != FontCategory::SlabSerif && !f.category.is_cjk() { f.category = FontCategory::Monospace; }
            f.available_weights.sort();
            if f.available_weights.is_empty() { f.available_weights.push(FontWeight::Normal); }
            f
        }).collect();
        Self { fonts }
    }

    pub fn from_fonts(fonts: Vec<FontInfo>) -> Self {
        Self { fonts }
    }

    /// Шрифты системы; `fc-list` вызывается при первом обращении и после `reload`.
    pub fn system() -> Arc<InstalledFontDatabase> {
        if let Some(db) = SYSTEM.read().unwrap().as_ref() { return db.clone(); }
        let text = match std::process::Command::new("fc-list").args(["-f", FC_FORMAT]).output() {
            Ok(output) if output.status.success() => String::from_utf8_lossy(&output.stdout).into_owned(),
            Ok(output) => { tracing::warn!(status = %output.status, "fontconfig: список шрифтов недоступен, используется семейство Qt"); String::new() },
            Err(e) => { tracing::warn!(error = %e, "Не удалось запустить fc-list, используется семейство Qt"); String::new() },
        };
        let db = Arc::new(Self::parse(&text));
        tracing::debug!(families = db.fonts.len(), "База установленных шрифтов обновлена");
        *SYSTEM.write().unwrap() = Some(db.clone());
        db
    }

    /// Шрифты установлены или удалены: следующий `system()` опросит fontconfig заново.
    pub fn reload() {
        *SYSTEM.write().unwrap() = None;
    }

    pub fn fonts(&self) -> &[FontInfo] {
        &self.fonts
    }

    pub fn find(&self, family: &str) -> Option<&FontInfo> {
        self.fonts.iter().find(|f| f.family.eq_ignore_ascii_case(family))
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub const SAMPLE: &str = "PT Serif\ten|ru|uk|de\t0\t80\t100\t0\n\
PT Serif\ten|ru|uk|de\t0\t200\t100\t100\n\
Noto Serif\ten|ru|uk|de\t0\t80\t100\t0\n\
Inter\ten|ru|de|el\t0\t80\t100\t0\n\
Inter\ten|ru|de|el\t0\t[100 900]\t100\t0\n\
Noto Sans CJK JP\tja|zh-cn|ko|en|ru\t0\t80\t100\t0\n\
Iosevka Slab\ten|ru\t100\t80\t100\t0\n\
JetBrains Mono\ten|ru\t100\t80\t100\t0\n\
Roboto Condensed\ten|ru\t0\t80\t75\t0\n\
Fancy Latin Only\ten|de|fr\t0\t80\t100\t0\n";

    #[test]
    fn parses_fc_list_and_merges_styles() {
        let db = InstalledFontDatabase::parse(SAMPLE);
        let pt = db.find("pt serif").unwrap();
        assert!(pt.supports_cyrillic && pt.italic_available);
        assert_eq!(pt.available_weights, vec![FontWeight::Normal, FontWeight::Bold]);
        assert_eq!(pt.category, FontCategory::Serif);
        assert!(db.find("Noto Sans CJK JP").unwrap().supports_cjk);
        assert_eq!(db.find("Noto Sans CJK JP").unwrap().category, FontCategory::CjkSans);
        let slab = db.find("Iosevka Slab").unwrap();
        assert_eq!(slab.category, FontCategory::SlabSerif);
        assert!(slab.is_monospace);
        assert_eq!(db.find("JetBrains Mono").unwrap().category, FontCategory::Monospace);
        assert!(db.find("Roboto Condensed").unwrap().is_condensed);
        assert!(!db.find("Fancy Latin Only").unwrap().supports_cyrillic);
    }

    #[test]
    fn bundled_registry_has_fixed_families_and_cjk_coverage() {
        let db = InstalledFontDatabase::bundled();
        assert_eq!(db.fonts().len(), BUNDLED.len());
        let cjk = db.find("Noto Sans CJK SC").unwrap();
        assert!(["ja", "ko", "zh-cn", "zh-tw"].iter().all(|l| cjk.covers(l)));
        assert!(!db.find("Inter").unwrap().covers("ja"));
        // Every family covers the scripts the matcher relies on, and no file is shipped twice.
        assert!(db.fonts().iter().all(|f| f.covers("en") && f.covers("ru")));
        let mut files: Vec<_> = BUNDLED.iter().flat_map(|f| f.files).collect();
        files.sort();
        assert!(files.windows(2).all(|w| w[0] != w[1]), "duplicate font file in the registry");
        assert!(files.iter().all(|f| f.ends_with(".ttf") || f.ends_with(".otf")), "no italic or collection files");
        assert!(BUNDLED.iter().all(|f| f.files.iter().all(|n| !n.to_ascii_lowercase().contains("italic"))));
        // Every category used by the matcher has at least one bundled family.
        for (name, cat) in [("slab", FontCategory::SlabSerif), ("mono", FontCategory::Monospace), ("condensed", FontCategory::Condensed), ("cjk", FontCategory::CjkSans)] {
            assert!(db.fonts().iter().any(|f| f.category == cat), "no bundled {name} font");
        }
    }
}
