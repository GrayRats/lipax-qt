//! Property modes and the settings of the in-place translation mode.

use super::*;

/// Свойство, которое оценивается автоматически или задано пользователем. Каждое свойство
/// переключается отдельно: ручной шрифт не отключает автоматический размер и т. п.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", content = "value", rename_all = "snake_case")]
#[derive(Default)]
pub enum PropertyMode<T> {
    #[default]
    Auto,
    Manual(T),
}


impl<T: Clone> PropertyMode<T> {
    pub fn manual(&self) -> Option<&T> {
        match self { Self::Manual(v) => Some(v), Self::Auto => None }
    }
    /// Ручное значение или результат автоматической оценки.
    pub fn resolve(&self, auto: T) -> T {
        self.manual().cloned().unwrap_or(auto)
    }
}

/// Чем закрывается оригинал под переводом. Не связано с фоном окна перевода (`WindowAppearance::background_style`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InplaceBackgroundMode {
    #[default]
    Auto,
    #[serde(alias = "inpaint_blur", alias = "solid_fill")]
    TextReplacement,
    #[serde(alias = "transparent")]
    TransparentOutline,
    #[serde(alias = "adaptive_padding_fill")]
    PaddedFill,
}

/// Настройки режима «перевод поверх оригинала».
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct InplaceSettings {
    pub background_mode: InplaceBackgroundMode,
    pub font_family: PropertyMode<String>,
    /// Кегль, px экрана.
    pub font_size: PropertyMode<f32>,
    pub font_weight: PropertyMode<FontWeight>,
    pub italic: PropertyMode<bool>,
    /// Межстрочный интервал, множитель высоты строки.
    pub line_height: PropertyMode<f32>,
    /// Дополнительный трекинг, px экрана.
    pub letter_spacing: PropertyMode<f32>,
    pub alignment: PropertyMode<TextAlignment>,
    pub wrap_mode: PropertyMode<WrapMode>,
    /// `#rrggbb`.
    pub text_color: PropertyMode<String>,
    pub outline_color: PropertyMode<String>,
    pub outline_width: f32,
    pub shadow: bool,
    pub text_opacity: f32,
    pub fill_color: PropertyMode<String>,
    pub fill_opacity: f32,
    pub padding_x: f32,
    pub padding_y: f32,
    pub extra_margin: f32,
    pub corner_radius: f32,
    /// Поля, px экрана.
    pub padding: PropertyMode<Padding>,
    pub minimum_font_size: f32,
    pub maximum_font_size: f32,
    /// Разрешить узкий вариант шрифта, если перевод не помещается.
    pub allow_condensed_fallback: bool,
    /// Замена шрифта для категории: `"serif" = "PT Serif"` (ключ — категория в snake_case).
    pub font_overrides: BTreeMap<String, String>,
    /// Семейства, которые проверяются первыми при автоматическом выборе.
    pub preferred_fonts: Vec<String>,
    /// Блок делится там, где шаг между строками (центр–центр) больше `k` средних высот строки; см. `layout::split`.
    /// Меньше — дробнее (склеенные имя и реплика разделяются раньше), больше — крупнее блоки.
    pub line_gap_factor: f32,
    /// Resource bound on text blocks found in one frame (each costs an OCR job and a font probe). Blocks
    /// beyond it, the smallest first, are not read; they are counted in the inplace diagnostics.
    pub max_blocks: usize,
    /// The overlay is shown only while the game window is the active one (not minimised, not behind another window).
    pub only_when_active: bool,
    /// The field windows do not take the mouse: clicks go to the game widgets under the translation. The middle
    /// button then no longer hides the overlay (the hotkey and the switch in the main window still do).
    pub click_through: bool,
    /// Buttons and menu items of the game's interface ("SAVE", "LOAD", "Options") are left as they are: not read, not
    /// translated, nothing drawn over them. Off by default: the answers to choose from in a dialogue look like buttons too.
    pub skip_controls: bool,
    /// The names of the speakers over the dialogue are left as they are: not read, not translated, nothing drawn over
    /// them. Off by default; a name is often a proper noun, and a small translation over a large original name (or a
    /// name cut by the edge of the region and read as nonsense) looks worse than the original.
    pub skip_names: bool,
    /// Text the top edge of the region cuts (a speaker name just above the dialogue) is captured whole: the region is
    /// read higher by the height of that line, the overlay follows. Blocks that lie wholly in the added strip are not
    /// read. On by default; off keeps the capture exactly the drawn region.
    pub extend_up: bool,
}

impl Default for InplaceSettings {
    fn default() -> Self {
        Self {
            background_mode: InplaceBackgroundMode::Auto,
            font_family: PropertyMode::Auto,
            font_size: PropertyMode::Auto,
            font_weight: PropertyMode::Auto,
            italic: PropertyMode::Auto,
            line_height: PropertyMode::Auto,
            letter_spacing: PropertyMode::Auto,
            alignment: PropertyMode::Auto,
            wrap_mode: PropertyMode::Auto,
            text_color: PropertyMode::Auto,
            outline_color: PropertyMode::Auto,
            outline_width: 1.0,
            shadow: false,
            text_opacity: 1.0,
            fill_color: PropertyMode::Auto,
            fill_opacity: 1.0,
            padding_x: 0.0,
            padding_y: 0.0,
            extra_margin: 0.0,
            corner_radius: 4.0,
            padding: PropertyMode::Auto,
            minimum_font_size: 8.0,
            maximum_font_size: 96.0,
            allow_condensed_fallback: true,
            font_overrides: BTreeMap::new(),
            preferred_fonts: Vec::new(),
            line_gap_factor: crate::layout::split::DEFAULT_LINE_GAP_FACTOR,
            max_blocks: crate::layout::block_detector::DEFAULT_MAX_BLOCKS,
            only_when_active: false,
            click_through: false,
            skip_controls: false,
            skip_names: false,
            extend_up: true,
        }
    }
}

impl InplaceSettings {
    pub(super) fn sanitize(&mut self) {
        let finite = |v: f32, d: f32| if v.is_finite() { v } else { d };
        // Below 1.3 ordinary line spacing (1.2–1.5 heights) would be cut; above 4 nothing would ever be.
        self.line_gap_factor = finite(self.line_gap_factor, crate::layout::split::DEFAULT_LINE_GAP_FACTOR).clamp(1.3, 4.0);
        self.max_blocks = self.max_blocks.clamp(4, 256);
        self.minimum_font_size = finite(self.minimum_font_size, 8.0).clamp(4.0, 200.0);
        self.maximum_font_size = finite(self.maximum_font_size, 96.0).clamp(self.minimum_font_size, 400.0);
        if let PropertyMode::Manual(v) = &mut self.font_size { *v = finite(*v, 20.0).clamp(4.0, 400.0); }
        if let PropertyMode::Manual(v) = &mut self.line_height { *v = finite(*v, 1.0).clamp(0.7, 3.0); }
        if let PropertyMode::Manual(v) = &mut self.letter_spacing { *v = finite(*v, 0.0).clamp(-3.0, 20.0); }
        if let PropertyMode::Manual(p) = &mut self.padding {
            for v in [&mut p.left, &mut p.right, &mut p.top, &mut p.bottom] { *v = finite(*v, 0.0).clamp(0.0, 64.0); }
        }
        if let PropertyMode::Manual(c) = &self.text_color
            && (c.len() != 7 || !c.starts_with('#') || !c[1..].bytes().all(|b| b.is_ascii_hexdigit())) { self.text_color = PropertyMode::Auto; }
        if let PropertyMode::Manual(f) = &self.font_family && f.trim().is_empty() { self.font_family = PropertyMode::Auto; }
        self.outline_width = finite(self.outline_width, 1.0).clamp(0.0, 8.0);
        self.text_opacity = finite(self.text_opacity, 1.0).clamp(0.0, 1.0);
        self.fill_opacity = finite(self.fill_opacity, 1.0).clamp(0.0, 1.0);
        self.padding_x = finite(self.padding_x, 0.0).clamp(0.0, 64.0);
        self.padding_y = finite(self.padding_y, 0.0).clamp(0.0, 64.0);
        self.extra_margin = finite(self.extra_margin, 0.0).clamp(0.0, 64.0);
        self.corner_radius = finite(self.corner_radius, 4.0).clamp(0.0, 64.0);
        for color in [&mut self.outline_color, &mut self.fill_color] {
            if let PropertyMode::Manual(c) = color && (c.len() != 7 || !c.starts_with('#') || !c[1..].bytes().all(|b| b.is_ascii_hexdigit())) { *color = PropertyMode::Auto; }
        }
        self.preferred_fonts.retain(|f| !f.trim().is_empty());
        self.font_overrides.retain(|_, f| !f.trim().is_empty());
    }
}

/// Upper bound for any screen side in logical pixels (8K, rotated or not).
pub const MAX_SCREEN_SIDE: u32 = 16384;
