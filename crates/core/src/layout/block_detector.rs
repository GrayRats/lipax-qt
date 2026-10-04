//! Поиск независимых текстовых блоков в кадре области.
//!
//! Маска «чернил» (порог Оцу, текст — меньший класс) → связные компоненты → строки (перекрытие
//! по вертикали, разрыв по горизонтали не больше полутора высот) → блоки (близкие по вертикали
//! строки одной высоты, выравнивания и цвета). Имя персонажа, реплика и кнопки под ней остаются
//! разными блоками: их разделяют цвет, зазоры и выравнивание.

use super::{InkMask, Rect, TextBlockType, luma, mean_color};
use image::RgbaImage;

#[derive(Debug, Clone, PartialEq)]
pub struct TextLine {
    pub rect: Rect,
    /// Связные компоненты (буквы или их части) строки.
    pub glyphs: Vec<Rect>,
    pub ink_color: [u8; 3],
}

#[derive(Debug, Clone, PartialEq)]
pub struct DetectedTextBlock {
    /// Граница глифов блока, без полей.
    pub rect: Rect,
    pub lines: Vec<TextLine>,
    pub block_type: TextBlockType,
    pub ink_color: [u8; 3],
}

impl DetectedTextBlock {
    pub fn line_height(&self) -> f32 {
        let mut h: Vec<f32> = self.lines.iter().map(|l| l.rect.h).collect();
        h.sort_by(|a, b| a.total_cmp(b));
        h[h.len() / 2]
    }
}

pub struct BlockDetector {
    /// Больше блоков в одной области не бывает: лишние (самые мелкие) отбрасываются.
    pub max_blocks: usize,
}

impl Default for BlockDetector {
    fn default() -> Self {
        Self { max_blocks: 16 }
    }
}

#[derive(Clone, Copy)]
struct Component {
    rect: Rect,
    pixels: u32,
}

fn otsu(hist: &[u64; 256], total: u64) -> u8 {
    let sum: f64 = hist.iter().enumerate().map(|(i, &c)| i as f64 * c as f64).sum();
    let (mut sum_b, mut w_b, mut best, mut threshold) = (0.0, 0u64, -1.0, 128u8);
    for (t, &c) in hist.iter().enumerate() {
        w_b += c;
        if w_b == 0 { continue; }
        let w_f = total - w_b;
        if w_f == 0 { break; }
        sum_b += t as f64 * c as f64;
        let (m_b, m_f) = (sum_b / w_b as f64, (sum - sum_b) / w_f as f64);
        let between = w_b as f64 * w_f as f64 * (m_b - m_f).powi(2);
        if between > best { best = between; threshold = t as u8; }
    }
    threshold
}

fn find(parent: &mut [u32], mut i: u32) -> u32 {
    while parent[i as usize] != i {
        parent[i as usize] = parent[parent[i as usize] as usize];
        i = parent[i as usize];
    }
    i
}

/// Связные компоненты маски (8-связность) через серии пикселей и union-find.
fn components(mask: &InkMask) -> Vec<Component> {
    // Серии текущей и предыдущей строки: (x0, x1, метка).
    let mut parent: Vec<u32> = Vec::new();
    let mut runs: Vec<(usize, usize, usize, u32)> = Vec::new(); // y, x0, x1, label
    let mut prev: Vec<(usize, usize, u32)> = Vec::new();
    for y in 0..mask.h {
        let row = &mask.ink[y * mask.w..(y + 1) * mask.w];
        let mut cur = Vec::new();
        let mut x = 0;
        while x < mask.w {
            if !row[x] { x += 1; continue; }
            let x0 = x;
            while x < mask.w && row[x] { x += 1; }
            let label = parent.len() as u32;
            parent.push(label);
            for &(px0, px1, pl) in &prev {
                // 8-связность: серии касаются, если перекрываются с допуском в один пиксель.
                if px0 <= x && x0 <= px1 {
                    let (a, b) = (find(&mut parent, label), find(&mut parent, pl));
                    if a != b { parent[a.max(b) as usize] = a.min(b); }
                }
            }
            cur.push((x0, x, label));
            runs.push((y, x0, x, label));
        }
        prev = cur;
    }
    let mut boxes: std::collections::HashMap<u32, (usize, usize, usize, usize, u32)> = Default::default();
    for (y, x0, x1, label) in runs {
        let root = find(&mut parent, label);
        let e = boxes.entry(root).or_insert((x0, y, x1, y + 1, 0));
        e.0 = e.0.min(x0);
        e.1 = e.1.min(y);
        e.2 = e.2.max(x1);
        e.3 = e.3.max(y + 1);
        e.4 += (x1 - x0) as u32;
    }
    boxes.into_values()
        .map(|(x0, y0, x1, y1, n)| Component { rect: Rect::new(x0 as f32, y0 as f32, (x1 - x0) as f32, (y1 - y0) as f32), pixels: n })
        .collect()
}

fn median(mut v: Vec<f32>) -> f32 {
    if v.is_empty() { return 0.0; }
    v.sort_by(|a, b| a.total_cmp(b));
    v[v.len() / 2]
}

fn color_distance(a: [u8; 3], b: [u8; 3]) -> f32 {
    a.iter().zip(b).map(|(x, y)| (*x as f32 - y as f32).powi(2)).sum::<f32>().sqrt()
}

impl BlockDetector {
    /// Маска «чернил» для всего кадра. Считается один раз и используется всеми стадиями.
    pub fn ink_mask(img: &RgbaImage) -> InkMask {
        let (w, h) = (img.width() as usize, img.height() as usize);
        // Яркость считается один раз в плоскость u8 (цикл без ветвлений векторизуется),
        // гистограмма — четырьмя независимыми счётчиками: один массив с `+= 1` по случайному
        // индексу упирается в зависимость чтения после записи на однотонных кадрах.
        let luma_plane: Vec<u8> = img.as_raw().as_chunks::<4>().0.iter().map(|p| luma(p)).collect();
        let mut lanes = [[0u32; 256]; 4];
        let (quads, rest) = luma_plane.as_chunks::<4>();
        for q in quads {
            for (lane, &v) in lanes.iter_mut().zip(q) { lane[v as usize] += 1; }
        }
        for &v in rest { lanes[0][v as usize] += 1; }
        let mut hist = [0u64; 256];
        for (i, slot) in hist.iter_mut().enumerate() { *slot = lanes.iter().map(|l| l[i] as u64).sum(); }
        let total = (w * h).max(1) as u64;
        let t = otsu(&hist, total);
        let dark: u64 = hist[..=t as usize].iter().sum();
        let ink_is_dark = dark * 2 < total;
        let ink = luma_plane.iter().map(|&l| (l <= t) == ink_is_dark).collect();
        InkMask { w, h, ink, ink_is_dark }
    }

    pub fn detect_text_blocks(&self, img: &RgbaImage, mask: &InkMask) -> Vec<DetectedTextBlock> {
        let (fw, fh) = (mask.w as f32, mask.h as f32);
        if mask.w < 8 || mask.h < 8 { return Vec::new(); }
        // Шум и фоновые элементы (рамки панелей, крупные фигуры) — не текст.
        let comps: Vec<Component> = components(mask).into_iter()
            .filter(|c| c.pixels >= 3)
            .filter(|c| !(c.rect.w > 0.6 * fw && c.rect.h > 0.4 * fh) && c.rect.h < 0.8 * fh)
            .collect();
        if comps.is_empty() { return Vec::new(); }
        let typical = median(comps.iter().filter(|c| c.rect.h >= 4.0).map(|c| c.rect.h).collect());
        let main_min = (typical * 0.4).max(4.0);
        let (mut main, small): (Vec<_>, Vec<_>) = comps.into_iter().partition(|c| c.rect.h >= main_min);
        // Слишком широкие при малой высоте — линии-разделители интерфейса.
        main.retain(|c| !(c.rect.w > 8.0 * c.rect.h && c.rect.w > 0.3 * fw && c.pixels as f32 > 0.8 * c.rect.area()));

        // Строки: сначала полосы по перекрытию по вертикали, затем разрывы по горизонтали.
        main.sort_by(|a, b| a.rect.center().1.total_cmp(&b.rect.center().1));
        let mut bands: Vec<(f32, f32, Vec<Rect>)> = Vec::new(); // top, bottom (средние), члены
        for c in &main {
            let r = c.rect;
            let fit = bands.iter_mut().find(|(t, b, _)| {
                let overlap = r.bottom().min(*b) - r.y.max(*t);
                overlap >= 0.5 * r.h.min(*b - *t)
            });
            match fit {
                Some((t, b, members)) => {
                    let n = members.len() as f32;
                    *t = (*t * n + r.y) / (n + 1.0);
                    *b = (*b * n + r.bottom()) / (n + 1.0);
                    members.push(r);
                }
                None => bands.push((r.y, r.bottom(), vec![r])),
            }
        }
        let mut lines: Vec<TextLine> = Vec::new();
        for (_, _, mut members) in bands {
            members.sort_by(|a, b| a.x.total_cmp(&b.x));
            let h = median(members.iter().map(|r| r.h).collect());
            let mut current: Vec<Rect> = Vec::new();
            let mut right = f32::MIN;
            for r in members {
                if !current.is_empty() && r.x - right > 1.5 * h {
                    lines.push(TextLine { rect: Rect::default(), glyphs: std::mem::take(&mut current), ink_color: [0; 3] });
                }
                right = right.max(r.right());
                current.push(r);
            }
            if !current.is_empty() {
                lines.push(TextLine { rect: Rect::default(), glyphs: current, ink_color: [0; 3] });
            }
        }
        // Точки, акценты, запятые — к ближайшей строке, которая их накрывает по горизонтали.
        for c in small {
            let r = c.rect;
            let (cx, cy) = r.center();
            let host = lines.iter_mut()
                .map(|l| { let lr = l.glyphs.iter().fold(l.glyphs[0], |a, g| a.union(g)); (l, lr) })
                .filter(|(_, lr)| cx >= lr.x - 2.0 && cx <= lr.right() + 2.0)
                .map(|(l, lr)| (l, (cy - lr.center().1).abs() - lr.h / 2.0))
                .filter(|(l, d)| *d <= 0.6 * l.glyphs.iter().map(|g| g.h).fold(0.0, f32::max))
                .min_by(|a, b| a.1.total_cmp(&b.1));
            if let Some((l, _)) = host { l.glyphs.push(r); }
        }
        for l in &mut lines {
            l.rect = l.glyphs.iter().fold(l.glyphs[0], |a, g| a.union(g));
            l.ink_color = mean_color(img, &l.rect, |x, y| mask.at(x, y)).unwrap_or([255; 3]);
        }
        // Одиночная крошечная «строка» — шум.
        lines.retain(|l| l.glyphs.len() >= 2 || l.rect.w >= 2.0 * l.rect.h);

        // Блоки: строка присоединяется к блоку, если она прямо под его последней строкой,
        // той же высоты, того же цвета и выровнена с ним.
        lines.sort_by(|a, b| a.rect.y.total_cmp(&b.rect.y));
        let mut blocks: Vec<Vec<TextLine>> = Vec::new();
        for line in lines {
            let r = line.rect;
            let host = blocks.iter_mut().find(|b| {
                let last = b.last().unwrap().rect;
                let h = last.h.max(r.h);
                let gap = r.y - last.bottom();
                let same_height = last.h.max(r.h) / last.h.min(r.h).max(1.0) <= 1.45;
                let aligned = r.intersection(&Rect::new(last.x, r.y, last.w, r.h)) > 0.0
                    || (r.x - last.x).abs() <= 2.0 * h
                    || (r.center().0 - last.center().0).abs() <= 2.0 * h;
                let same_color = color_distance(b.last().unwrap().ink_color, line.ink_color) <= 70.0;
                gap >= -0.3 * h && gap <= 1.0 * h && same_height && aligned && same_color
            });
            match host {
                Some(b) => b.push(line),
                None => blocks.push(vec![line]),
            }
        }
        let mut out: Vec<DetectedTextBlock> = blocks.into_iter().map(|lines| {
            let rect = lines.iter().skip(1).fold(lines[0].rect, |a, l| a.union(&l.rect));
            let ink_color = lines[0].ink_color;
            DetectedTextBlock { rect, lines, block_type: TextBlockType::Unknown, ink_color }
        }).collect();
        out.sort_by(|a, b| b.rect.area().total_cmp(&a.rect.area()));
        out.truncate(self.max_blocks);
        out.sort_by(|a, b| (a.rect.y, a.rect.x).partial_cmp(&(b.rect.y, b.rect.x)).unwrap());
        classify(&mut out, fw, fh);
        out
    }
}

/// Тип блока по геометрии. Только уверенные случаи, иначе `Unknown`.
fn classify(blocks: &mut [DetectedTextBlock], fw: f32, fh: f32) {
    let rects: Vec<(Rect, usize)> = blocks.iter().map(|b| (b.rect, b.lines.len())).collect();
    let short = |r: &Rect, lines: usize| lines == 1 && r.w < 0.3 * fw;
    let mut types = vec![TextBlockType::Unknown; blocks.len()];
    for (i, (r, lines)) in rects.iter().enumerate() {
        let (cx, cy) = r.center();
        if *lines >= 2 && r.w >= 0.35 * fw {
            types[i] = TextBlockType::Dialogue;
        } else if *lines == 1 && r.w >= 0.25 * fw && (cx - fw / 2.0).abs() <= 0.1 * fw && cy >= 0.55 * fh {
            types[i] = TextBlockType::Subtitle;
        }
    }
    for (i, (r, lines)) in rects.iter().enumerate() {
        if types[i] != TextBlockType::Unknown || !short(r, *lines) { continue; }
        // Имя прямо над репликой или субтитром, у её левого края или по центру.
        let names = rects.iter().enumerate().any(|(j, (o, _))| {
            j != i && matches!(types[j], TextBlockType::Dialogue | TextBlockType::Subtitle)
                && o.y >= r.bottom() && o.y - r.bottom() <= 1.5 * r.h
                && ((r.x - o.x).abs() <= 0.1 * fw || (r.center().0 - o.center().0).abs() <= 0.1 * fw)
        });
        let same_row = rects.iter().enumerate().filter(|(j, (o, l))| {
            *j != i && short(o, *l) && o.bottom().min(r.bottom()) - o.y.max(r.y) >= 0.5 * o.h.min(r.h)
        }).count();
        let same_column = rects.iter().enumerate().filter(|(j, (o, l))| {
            *j != i && short(o, *l) && (o.x - r.x).abs() <= 0.5 * r.h && (o.y - r.y).abs() <= 4.0 * r.h
        }).count();
        types[i] = if names { TextBlockType::CharacterName }
            else if same_row >= 1 { TextBlockType::Button }
            else if same_column >= 2 { TextBlockType::MenuItem }
            else { TextBlockType::Unknown };
    }
    for (b, t) in blocks.iter_mut().zip(types) { b.block_type = t; }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::testing::*;

    #[test]
    fn dialogue_scene_splits_into_name_dialogue_and_buttons() {
        let mut img = canvas(1200, 500, [25, 30, 40]);
        // Имя персонажа другим цветом, над репликой.
        draw_line(&mut img, 100, 100, 8, 14, 4, 22, 3, [255, 200, 60], false);
        // Реплика в две строки.
        draw_line(&mut img, 100, 140, 40, 14, 4, 22, 3, [240, 240, 240], false);
        draw_line(&mut img, 100, 172, 30, 14, 4, 22, 3, [240, 240, 240], false);
        // Две кнопки в одном ряду, далеко друг от друга.
        draw_line(&mut img, 300, 380, 7, 14, 4, 22, 3, [240, 240, 240], false);
        draw_line(&mut img, 700, 380, 6, 14, 4, 22, 3, [240, 240, 240], false);

        let mask = BlockDetector::ink_mask(&img);
        let blocks = BlockDetector::default().detect_text_blocks(&img, &mask);
        let summary: Vec<_> = blocks.iter().map(|b| (b.block_type, b.lines.len(), b.rect.x as u32, b.rect.y as u32)).collect();
        assert_eq!(blocks.len(), 4, "{summary:?}");
        assert_eq!(summary[0], (TextBlockType::CharacterName, 1, 100, 100));
        assert_eq!(summary[1], (TextBlockType::Dialogue, 2, 100, 140));
        assert_eq!((summary[2].0, summary[3].0), (TextBlockType::Button, TextBlockType::Button));
        assert!(blocks[1].ink_color[0] > 200 && blocks[0].ink_color[2] < 120);
    }

    #[test]
    fn centred_bottom_line_is_a_subtitle() {
        let mut img = canvas(1000, 300, [60, 70, 50]);
        draw_line(&mut img, 250, 230, 30, 13, 4, 20, 3, [255, 255, 255], false);
        let mask = BlockDetector::ink_mask(&img);
        let blocks = BlockDetector::default().detect_text_blocks(&img, &mask);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].block_type, TextBlockType::Subtitle);
        assert!((blocks[0].line_height() - 20.0).abs() < 1.0);
    }

    #[test]
    fn empty_and_tiny_frames() {
        let img = canvas(300, 100, [90, 90, 90]);
        assert!(BlockDetector::default().detect_text_blocks(&img, &BlockDetector::ink_mask(&img)).is_empty());
        let tiny = canvas(4, 4, [0, 0, 0]);
        assert!(BlockDetector::default().detect_text_blocks(&tiny, &BlockDetector::ink_mask(&tiny)).is_empty());
    }
}
