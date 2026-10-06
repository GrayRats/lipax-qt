//! Определение смены текста в кадре.
//!
//! Кадр сводится к битовой карте блоков 4×4 px: бит стоит там, где есть резкий перепад яркости
//! со светлой стороной (края букв субтитров и текста на светлых плашках). Плавная смена освещения
//! и тёмный подвижный фон в карту не попадают. Всё считается за один проход целыми числами.
//!
//! Изменение «резкое», если поменялась заметная доля текстовых блоков. Изменение в пределах
//! одного-двух пятен размером с курсор (курсор игры, подсветка под ним) пропускается; если такое
//! пятно стоит на месте `MINOR_HOLD`, оно засчитывается — так не теряются короткие слова.

use image::DynamicImage;
use std::time::{Duration, Instant};

/// Сторона блока карты, px.
const BLOCK: usize = 4;
/// Плитка для поиска пятен: 8×8 блоков = 32 px.
const TILE: usize = 8;
/// Пятно размером с курсор — не больше 4×4 плиток (128 px) по каждой стороне.
const CURSOR_TILES: usize = 4;
/// Старая и новая позиция курсора.
const CURSOR_BLOBS: usize = 2;
/// Меньше стольких изменённых блоков — шум.
const MIN_DIFF: usize = 3;
/// Доля изменённых блоков от всех текстовых блоков обоих кадров, ниже которой изменение — мерцание фона.
const NOISE_RATIO: f32 = 0.12;
/// Столько текстовых блоков нужно, чтобы считать, что в кадре вообще есть текст.
const MIN_TEXT: usize = 4;
/// Яркость светлой стороны перепада.
const BRIGHT: u8 = 140;
const MINOR_HOLD: Duration = Duration::from_millis(1000);

#[derive(PartialEq, Clone)]
struct Signature {
    w: usize,
    h: usize,
    bits: Vec<bool>,
}

impl Signature {
    fn new(img: &DynamicImage, edge: u8) -> Self {
        let owned;
        let rgba = match img.as_rgba8() {
            Some(b) => b,
            None => {
                owned = img.to_rgba8();
                &owned
            }
        };
        let (w, h) = (rgba.width() as usize, rgba.height() as usize);
        let (bw, bh) = (w.div_ceil(BLOCK), h.div_ceil(BLOCK));
        let mut bits = vec![false; bw * bh];
        // Большие области читаются через пиксель: штрихи букв всё равно дают перепады.
        let step = if w * h > 1_000_000 { 2 } else { 1 };
        let raw = rgba.as_raw();
        let cols = w.div_ceil(step);
        let (mut prev, mut cur) = (vec![0u8; cols], vec![0u8; cols]);
        let edge_at = |a: u8, b: u8| a.abs_diff(b) >= edge && a.max(b) >= BRIGHT;
        for (row_index, y) in (0..h).step_by(step).enumerate() {
            let row = &raw[y * w * 4..(y + 1) * w * 4];
            for (c, p) in cur.iter_mut().zip(row.chunks_exact(4 * step)) {
                *c = ((p[0] as u32 * 77 + p[1] as u32 * 150 + p[2] as u32 * 29) >> 8) as u8;
            }
            // Хвост строки, не попавший в chunks_exact при шаге 2.
            if cols * step > w {
                let p = &row[(cols - 1) * step * 4..];
                cur[cols - 1] = ((p[0] as u32 * 77 + p[1] as u32 * 150 + p[2] as u32 * 29) >> 8) as u8;
            }
            let base = (y / BLOCK) * bw;
            for i in 0..cols {
                let l = cur[i];
                let horizontal = i > 0 && edge_at(l, cur[i - 1]);
                let vertical = row_index > 0 && edge_at(l, prev[i]);
                if horizontal || vertical {
                    bits[base + i * step / BLOCK] = true;
                }
            }
            std::mem::swap(&mut prev, &mut cur);
        }
        Self { w: bw, h: bh, bits }
    }

    fn count(&self) -> usize {
        self.bits.iter().filter(|b| **b).count()
    }
}

#[derive(Clone)]
pub struct ChangeDetector {
    /// Последний принятый кадр: с ним сравниваются следующие.
    reference: Option<Signature>,
    /// Текстовых блоков в последнем проверенном кадре.
    text_blocks: usize,
    /// Мелкое изменение: какие плитки и с какого момента.
    minor: Option<(Vec<u32>, Instant)>,
}

impl ChangeDetector {
    pub fn new() -> Self {
        Self { reference: None, text_blocks: 0, minor: None }
    }

    pub fn reset(&mut self) {
        *self = Self::new();
    }

    /// Есть ли в последнем проверенном кадре что-то похожее на текст. Без текста OCR не нужен.
    pub fn has_text(&self) -> bool {
        self.text_blocks >= MIN_TEXT
    }

    /// Порог контраста края по настройке чувствительности (ниже — чувствительнее).
    fn edge_threshold(sensitivity: f32) -> u8 {
        (40.0 + sensitivity.clamp(0.0, 25.0) * 8.0) as u8
    }

    /// true — текст в кадре заметно изменился относительно последнего принятого кадра
    /// (первый кадр всегда считается изменённым).
    pub fn changed(&mut self, img: &DynamicImage, sensitivity: f32, now: Instant) -> bool {
        let cur = Signature::new(img, Self::edge_threshold(sensitivity));
        self.text_blocks = cur.count();
        let reference = match &self.reference {
            Some(r) if r.w == cur.w && r.h == cur.h => r,
            _ => return self.accept(cur),
        };
        let (tw, th) = (cur.w.div_ceil(TILE), cur.h.div_ceil(TILE));
        let mut tiles = vec![false; tw * th];
        let (mut diff, mut union) = (0usize, 0usize);
        for (i, (a, b)) in reference.bits.iter().zip(&cur.bits).enumerate() {
            union += (*a || *b) as usize;
            if a != b {
                diff += 1;
                tiles[(i / cur.w) / TILE * tw + (i % cur.w) / TILE] = true;
            }
        }
        if diff < MIN_DIFF {
            self.minor = None;
            return false;
        }
        // Компактное пятно: курсор, подсветка или короткое слово. Засчитывается, только если стоит на месте.
        // В области размером с курсор их от текста не отличить, там работает только правило доли.
        let small_area = tw <= CURSOR_TILES && th <= CURSOR_TILES;
        if !small_area && cursor_like(&tiles, tw, th) {
            let changed: Vec<u32> = tiles.iter().enumerate().filter(|(_, t)| **t).map(|(i, _)| i as u32).collect();
            match &self.minor {
                Some((then, since)) if *then == changed => {
                    if now.saturating_duration_since(*since) >= MINOR_HOLD {
                        return self.accept(cur);
                    }
                }
                _ => self.minor = Some((changed, now)),
            }
            return false;
        }
        // Разбросанные мелкие изменения при неизменном тексте — мерцание фона.
        self.minor = None;
        if (diff as f32) < NOISE_RATIO * union as f32 {
            return false;
        }
        self.accept(cur)
    }

    fn accept(&mut self, cur: Signature) -> bool {
        self.reference = Some(cur);
        self.minor = None;
        true
    }
}

/// Все изменённые плитки укладываются в `CURSOR_BLOBS` пятен не больше `CURSOR_TILES` по стороне.
fn cursor_like(tiles: &[bool], w: usize, h: usize) -> bool {
    if tiles.iter().filter(|t| **t).count() > CURSOR_BLOBS * CURSOR_TILES * CURSOR_TILES {
        return false;
    }
    let mut seen = vec![false; tiles.len()];
    let mut blobs = 0;
    let mut stack = Vec::new();
    for start in 0..tiles.len() {
        if !tiles[start] || seen[start] {
            continue;
        }
        blobs += 1;
        if blobs > CURSOR_BLOBS {
            return false;
        }
        let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0, 0);
        seen[start] = true;
        stack.push(start);
        while let Some(i) = stack.pop() {
            let (x, y) = (i % w, i / w);
            (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
            for ny in y.saturating_sub(1)..=(y + 1).min(h - 1) {
                for nx in x.saturating_sub(1)..=(x + 1).min(w - 1) {
                    let n = ny * w + nx;
                    if tiles[n] && !seen[n] {
                        seen[n] = true;
                        stack.push(n);
                    }
                }
            }
        }
        if x1 - x0 >= CURSOR_TILES || y1 - y0 >= CURSOR_TILES {
            return false;
        }
    }
    true
}

impl Default for ChangeDetector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};

    /// Тёмный кадр w×h со светлыми «буквами» — прямоугольниками (x, y, w, h).
    fn frame(w: u32, h: u32, glyphs: &[(u32, u32, u32, u32)]) -> DynamicImage {
        let mut img = RgbaImage::from_pixel(w, h, Rgba([20, 20, 30, 255]));
        for &(gx, gy, gw, gh) in glyphs {
            for y in gy..gy + gh {
                for x in gx..gx + gw {
                    img.put_pixel(x, y, Rgba([250, 250, 250, 255]));
                }
            }
        }
        DynamicImage::ImageRgba8(img)
    }

    /// Строка «текста»: штрихи шириной 3 px с шагом `pitch`, начиная с `from`.
    fn line(from: u32, to: u32, pitch: u32) -> Vec<(u32, u32, u32, u32)> {
        (from..to).step_by(pitch as usize).map(|x| (x, 60, 3, 24)).collect()
    }

    #[test]
    fn identical_frames_not_changed() {
        let mut d = ChangeDetector::new();
        let t = Instant::now();
        assert!(d.changed(&frame(800, 160, &line(40, 700, 9)), 2.0, t));
        assert!(!d.changed(&frame(800, 160, &line(40, 700, 9)), 2.0, t));
        assert!(d.has_text());
    }

    #[test]
    fn new_line_of_text_is_sharp_change() {
        let mut d = ChangeDetector::new();
        let t = Instant::now();
        d.changed(&frame(800, 160, &line(40, 700, 9)), 2.0, t);
        assert!(d.changed(&frame(800, 160, &line(60, 500, 13)), 2.0, t));
    }

    #[test]
    fn brightness_change_without_text_ignored() {
        let mut d = ChangeDetector::new();
        let t = Instant::now();
        let plain = |v| DynamicImage::ImageRgba8(RgbaImage::from_pixel(800, 160, Rgba([v, v, v, 255])));
        d.changed(&plain(10), 2.0, t);
        assert!(!d.changed(&plain(200), 2.0, t));
        assert!(!d.has_text());
    }

    #[test]
    fn moving_cursor_ignored_parked_cursor_counts_after_hold() {
        let mut d = ChangeDetector::new();
        let t = Instant::now();
        let text = line(40, 700, 9);
        d.changed(&frame(1200, 160, &text), 2.0, t);
        let with_cursor = |x| {
            let mut g = text.clone();
            g.push((x, 10, 24, 32));
            frame(1200, 160, &g)
        };
        for (i, x) in [800, 900, 1000, 1100].into_iter().enumerate() {
            assert!(!d.changed(&with_cursor(x), 2.0, t + Duration::from_millis(100 * i as u64)), "cursor at {x}");
        }
        // Курсор остановился: через MINOR_HOLD это засчитывается один раз.
        let rest = t + Duration::from_millis(400);
        assert!(!d.changed(&with_cursor(1100), 2.0, rest));
        assert!(d.changed(&with_cursor(1100), 2.0, rest + MINOR_HOLD));
        assert!(!d.changed(&with_cursor(1100), 2.0, rest + MINOR_HOLD * 2));
    }

    #[test]
    fn short_word_next_to_long_line_counts_after_hold() {
        let mut d = ChangeDetector::new();
        let t = Instant::now();
        let mut text = line(40, 700, 9);
        d.changed(&frame(1200, 160, &text), 2.0, t);
        text.extend(line(900, 960, 9));
        assert!(!d.changed(&frame(1200, 160, &text), 2.0, t));
        assert!(d.changed(&frame(1200, 160, &text), 2.0, t + MINOR_HOLD));
    }

    #[test]
    fn small_region_reacts_to_any_text_change() {
        let mut d = ChangeDetector::new();
        let t = Instant::now();
        d.changed(&frame(100, 50, &[(10, 10, 20, 10)]), 2.0, t);
        assert!(d.changed(&frame(100, 50, &[(40, 10, 20, 10)]), 2.0, t));
    }

    /// Форматы экранов: 16:9, 21:9, 32:9, 16:10, 4:3 и портретный.
    const FORMATS: [(u32, u32); 7] = [(1920, 1080), (2560, 1080), (3440, 1440), (5120, 1440), (1920, 1200), (1024, 768), (1080, 1920)];

    /// Экраны системы в пикселях захвата: KWin снимает в родном разрешении, масштаб на размер кадра
    /// не влияет. Текущие режимы из kscreen-doctor (с учётом поворота), иначе предпочтительные
    /// режимы подключённых DRM-выходов, иначе типовые форматы.
    fn system_screens() -> Vec<(u32, u32)> {
        let size = |w: u64, h: u64| (w as u32, h as u32);
        let mut screens: Vec<(u32, u32)> = std::process::Command::new("kscreen-doctor").arg("-j").output().ok()
            .and_then(|o| serde_json::from_slice::<serde_json::Value>(&o.stdout).ok())
            .map(|d| d["outputs"].as_array().into_iter().flatten()
                .filter(|o| o["enabled"].as_bool() == Some(true))
                .filter_map(|o| {
                    let mode = o["modes"].as_array()?.iter().find(|m| m["id"] == o["currentModeId"])?;
                    let (w, h) = (mode["size"]["width"].as_u64()?, mode["size"]["height"].as_u64()?);
                    // Поворот на 90° (2 — влево, 8 — вправо) меняет стороны местами.
                    Some(if matches!(o["rotation"].as_u64(), Some(2 | 8)) { size(h, w) } else { size(w, h) })
                })
                .collect())
            .unwrap_or_default();
        if screens.is_empty() {
            screens = std::fs::read_dir("/sys/class/drm").into_iter().flatten().flatten()
                .filter(|e| std::fs::read_to_string(e.path().join("status")).is_ok_and(|s| s.trim() == "connected"))
                .filter_map(|e| {
                    let modes = std::fs::read_to_string(e.path().join("modes")).ok()?;
                    let (w, h) = modes.lines().next()?.split_once('x')?;
                    Some((w.parse().ok()?, h.trim_end_matches(|c: char| !c.is_ascii_digit()).parse().ok()?))
                })
                .collect();
        }
        if screens.is_empty() {
            screens = FORMATS.to_vec();
        }
        screens.sort_unstable();
        screens.dedup();
        screens
    }

    /// Типовые области на экране w×h: весь экран, полоса субтитров (нижняя пятая часть)
    /// и окно диалога (60% ширины, треть высоты).
    fn areas(w: u32, h: u32) -> [(&'static str, u32, u32); 3] {
        [("экран", w, h), ("субтитры", w, (h / 5).max(140)), ("диалог", w * 3 / 5, (h / 3).max(140))]
    }

    #[test]
    fn works_for_any_screen_format() {
        for (sw, sh) in FORMATS {
            for (area, w, h) in areas(sw, sh) {
                let at = format!("{sw}x{sh}, {area} {w}x{h}");
                let text = line(40, w - 100, 9);
                let t = Instant::now();
                let mut d = ChangeDetector::new();
                assert!(d.changed(&frame(w, h, &text), 2.0, t), "{at}: первый кадр");
                assert!(d.has_text(), "{at}: текст найден");
                let mut with_cursor = text.clone();
                with_cursor.push((w - 150, 100, 24, 32));
                assert!(!d.changed(&frame(w, h, &with_cursor), 2.0, t), "{at}: курсор не считается сменой текста");
                assert!(d.changed(&frame(w, h, &line(60, w / 2, 13)), 2.0, t), "{at}: новая строка текста");
            }
        }
    }

    /// Скорость на экранах этой системы:
    /// cargo test --release -p lipa-core detect::tests::speed -- --ignored --nocapture
    #[test]
    #[ignore]
    fn speed() {
        for (sw, sh) in system_screens() {
            for (area, w, h) in areas(sw, sh) {
                let img = frame(w, h, &line(40, w - 100, 9));
                let mut d = ChangeDetector::new();
                let t = Instant::now();
                let runs = 50;
                for _ in 0..runs { d.changed(&img, 2.0, t); }
                println!("экран {sw}x{sh}, {area} {w}x{h}: {:?} на кадр", t.elapsed() / runs);
            }
        }
    }

    #[test]
    fn odd_sizes_and_large_frames() {
        let mut d = ChangeDetector::new();
        let t = Instant::now();
        assert!(d.changed(&frame(1601, 701, &line(40, 1500, 11)), 2.0, t));
        assert!(!d.changed(&frame(1601, 701, &line(40, 1500, 11)), 2.0, t));
        assert!(d.has_text());
    }
}
