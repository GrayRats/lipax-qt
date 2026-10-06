//! Plain data about a recognised field for whoever shows it (the inspector, tools, tests): what the
//! engine read, where, and how the field was styled. Rectangles are `CropRect`: pixels of the image
//! the OCR engine was given (a crop of the frame around the field), the space its lines come in.
//! Everything serialises to JSON, which is how the data reaches Qt.

use super::{CropRect, TextBlockType};
use serde::{Serialize, Serializer};

/// Identity of a tracked field: stable while the field is on screen and through the grace period after it vanishes.
pub type TextBlockId = u64;

/// `#rrggbb` in JSON.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color(pub [u8; 3]);

impl Serialize for Color {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&super::hex(self.0))
    }
}

/// One line of text as the engine read it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TextLine {
    pub text: String,
    pub bbox: CropRect,
    /// Height of the box, px of the crop.
    pub height: f32,
    /// 0–100.
    pub confidence: f32,
}

impl From<&crate::ocr::OcrLine> for TextLine {
    fn from(line: &crate::ocr::OcrLine) -> Self {
        Self { text: line.text.clone(), bbox: line.rect, height: line.rect.h, confidence: line.confidence }
    }
}

/// A field: its lines, its box and the style the engine settled on.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TextBlock {
    pub id: TextBlockId,
    pub lines: Vec<TextLine>,
    pub bbox: CropRect,
    pub font_family: String,
    pub color: Color,
    pub block_type: TextBlockType,
    /// Distance between the lines of the original, px of the frame; 0 for a single line.
    pub line_spacing: f32,
}

impl TextBlock {
    /// The block of a published field. `lines` are the OCR lines of its last reading and `crop_origin` the
    /// corner of the crop they are relative to; the glyph box of the field (frame pixels) is moved into the same crop.
    pub fn of_field(block: &super::engine::InplaceBlock, lines: &[crate::ocr::OcrLine], crop_origin: (u32, u32)) -> Self {
        Self {
            id: block.id,
            lines: lines.iter().map(TextLine::from).collect(),
            bbox: block.text_rect.in_crop(crop_origin),
            font_family: block.font.family.clone(),
            color: Color(block.style.text_color),
            block_type: block.block_type,
            line_spacing: block.style.line_height_px.unwrap_or(0.0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialises_the_way_qt_reads_it() {
        let block = TextBlock {
            id: 7,
            lines: vec![TextLine { text: "Hello".into(), bbox: CropRect::in_space(3.0, 3.0, 80.0, 18.0), height: 18.0, confidence: 91.5 }],
            bbox: CropRect::in_space(3.0, 3.0, 80.0, 40.0),
            font_family: "Inter".into(),
            color: Color([240, 200, 16]),
            block_type: TextBlockType::Dialogue,
            line_spacing: 24.0,
        };
        let json = serde_json::to_value(&block).unwrap();
        assert_eq!(json["id"], 7);
        assert_eq!(json["color"], "#f0c810");
        assert_eq!(json["block_type"], "dialogue");
        assert_eq!(json["bbox"], serde_json::json!({ "x": 3.0, "y": 3.0, "w": 80.0, "h": 40.0 }), "the space is not part of the data");
        assert_eq!((json["lines"][0]["text"].as_str(), json["lines"][0]["confidence"].as_f64()), (Some("Hello"), Some(91.5)));
    }
}
