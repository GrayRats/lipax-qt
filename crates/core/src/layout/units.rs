//! Coordinate spaces. A rectangle carries the space it is measured in as a type, so pixels of the
//! captured frame, of an OCR crop and of the desktop cannot be mixed by accident: the compiler
//! refuses `Rect<FramePx>` where `Rect<DesktopPx>` is expected, and every move between spaces is
//! one of the explicit conversions below.
//!
//! - [`FramePx`]: pixels of the captured region frame. The default, used by detection and layout.
//! - [`CropPx`]: pixels of an image cut out of the frame, e.g. what an OCR engine was given.
//! - [`DesktopPx`]: logical (Qt) pixels on the desktop, global coordinates; placement and collisions.
//!
//! Things that are not pixels already have their own types: [`NormRect`] (fractions of the client
//! window or of the frame), [`WindowGeometry`] (the client area on the desktop, logical px) and
//! [`LogicalScale`] (how many logical pixels one frame pixel is).

use crate::capture::kwin::WindowGeometry;
use crate::settings::NormRect;
use serde::{Deserialize, Serialize};
use std::marker::PhantomData;

/// Marker for a coordinate space.
pub trait Space: Copy + std::fmt::Debug + Default + PartialEq + 'static {}

macro_rules! spaces {
    ($($(#[$doc:meta])* $name:ident),* $(,)?) => {$(
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
        pub struct $name;
        impl Space for $name {}
    )*};
}

spaces! {
    /// Pixels of the captured region frame.
    FramePx,
    /// Pixels of an image cut out of the frame.
    CropPx,
    /// Logical pixels on the desktop (global coordinates).
    DesktopPx,
}

/// A rectangle in the space `S`. Without an argument, `Rect` is a rectangle in the frame.
///
/// Mixing spaces does not compile:
///
/// ```compile_fail
/// use lipa_core::layout::{DesktopRect, Rect};
/// let in_the_frame = Rect::new(0.0, 0.0, 10.0, 10.0);
/// let _on_the_desktop: DesktopRect = in_the_frame;
/// ```
///
/// A crop becomes a frame rectangle only through its origin:
///
/// ```
/// use lipa_core::layout::{CropRect, Rect};
/// let line = CropRect::in_space(10.0, 5.0, 100.0, 20.0);
/// assert_eq!(line.in_frame((40, 300)), Rect::new(50.0, 305.0, 100.0, 20.0));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(bound = "")]
pub struct Rect<S: Space = FramePx> {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    #[serde(skip)]
    space: PhantomData<S>,
}

pub type DesktopRect = Rect<DesktopPx>;
pub type CropRect = Rect<CropPx>;

impl Rect<FramePx> {
    /// A rectangle in the frame: what detection and layout work with.
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self::in_space(x, y, w, h)
    }

    /// Fractions of a frame of `frame` pixels: the form QML receives.
    pub fn fraction_of(&self, frame: (u32, u32)) -> [f64; 4] {
        let (fw, fh) = (frame.0.max(1) as f64, frame.1.max(1) as f64);
        [self.x as f64 / fw, self.y as f64 / fh, self.w as f64 / fw, self.h as f64 / fh]
    }
}

impl Rect<FramePx> {
    /// The same rectangle in a crop cut from the frame at `origin` (the crop's top-left corner).
    pub fn in_crop(&self, origin: (u32, u32)) -> Rect<CropPx> {
        Rect::in_space(self.x - origin.0 as f32, self.y - origin.1 as f32, self.w, self.h)
    }
}

impl Rect<CropPx> {
    /// The same rectangle in the frame the crop was cut from at `origin` (its top-left corner).
    pub fn in_frame(&self, origin: (u32, u32)) -> Rect<FramePx> {
        Rect::in_space(self.x + origin.0 as f32, self.y + origin.1 as f32, self.w, self.h)
    }
}

impl DesktopRect {
    /// The client area of a window.
    pub fn of_window(window: &WindowGeometry) -> Self {
        Self::in_space(window.x as f32, window.y as f32, window.w as f32, window.h as f32)
    }
}

impl<S: Space> Rect<S> {
    /// A rectangle in any space; the space is stated by the caller (`Rect::<CropPx>::in_space`)
    /// or follows from where the value is used.
    pub fn in_space(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h, space: PhantomData }
    }
    pub fn right(&self) -> f32 {
        self.x + self.w
    }
    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }
    pub fn center(&self) -> (f32, f32) {
        (self.x + self.w / 2.0, self.y + self.h / 2.0)
    }
    pub fn area(&self) -> f32 {
        self.w.max(0.0) * self.h.max(0.0)
    }
    pub fn union(&self, o: &Self) -> Self {
        let (x, y) = (self.x.min(o.x), self.y.min(o.y));
        Self::in_space(x, y, self.right().max(o.right()) - x, self.bottom().max(o.bottom()) - y)
    }
    pub fn intersection(&self, o: &Self) -> f32 {
        let w = self.right().min(o.right()) - self.x.max(o.x);
        let h = self.bottom().min(o.bottom()) - self.y.max(o.y);
        if w > 0.0 && h > 0.0 { w * h } else { 0.0 }
    }
    pub fn iou(&self, o: &Self) -> f32 {
        let i = self.intersection(o);
        let u = self.area() + o.area() - i;
        if u > 0.0 { i / u } else { 0.0 }
    }
    /// The part of the rectangle inside `bounds`; empty (zero size, at the nearest edge) if they do not meet.
    pub fn clipped_to(&self, bounds: &Self) -> Self {
        let (x0, y0) = (self.x.max(bounds.x), self.y.max(bounds.y));
        Self::in_space(x0, y0, (self.right().min(bounds.right()) - x0).max(0.0), (self.bottom().min(bounds.bottom()) - y0).max(0.0))
    }
    /// Whether `inner` lies inside, within half a pixel (sums of fractions are not exact).
    pub fn contains(&self, inner: &Self) -> bool {
        inner.x >= self.x - 0.5 && inner.y >= self.y - 0.5 && inner.right() <= self.right() + 0.5 && inner.bottom() <= self.bottom() + 0.5
    }
    /// Расширить на `pad` со всех сторон, не выходя за `w`×`h`.
    pub fn expand(&self, pad: f32, w: f32, h: f32) -> Self {
        let (x0, y0) = ((self.x - pad).max(0.0), (self.y - pad).max(0.0));
        let (x1, y1) = ((self.right() + pad).min(w), (self.bottom() + pad).min(h));
        Self::in_space(x0, y0, (x1 - x0).max(0.0), (y1 - y0).max(0.0))
    }
    /// Целочисленный прямоугольник внутри `w`×`h`: x, y, ширина, высота (не меньше 1).
    pub fn pixels(&self, w: u32, h: u32) -> (u32, u32, u32, u32) {
        let x0 = (self.x.floor().max(0.0) as u32).min(w.saturating_sub(1));
        let y0 = (self.y.floor().max(0.0) as u32).min(h.saturating_sub(1));
        let x1 = (self.right().ceil().max(0.0) as u32).clamp(x0 + 1, w);
        let y1 = (self.bottom().ceil().max(0.0) as u32).clamp(y0 + 1, h);
        (x0, y0, x1 - x0, y1 - y0)
    }
}

/// How many logical pixels one pixel of the frame is. Lengths measured in the frame are turned into
/// lengths on screen only through this, so a frame length is never added to a screen length.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LogicalScale(f32);

impl LogicalScale {
    pub fn new(logical_per_frame_px: f32) -> Self {
        Self(logical_per_frame_px)
    }
    /// A length of the frame, in logical pixels.
    pub fn px(self, frame_length: f32) -> f32 {
        frame_length * self.0
    }
    /// A length in logical pixels, in frame pixels (never divides by a degenerate scale).
    pub fn frame_px(self, logical_length: f32) -> f32 {
        logical_length / self.0.max(0.01)
    }
    /// Bits for use in a cache key: equal scales give equal keys.
    pub fn key(self) -> u32 {
        self.0.to_bits()
    }
}

/// Where a region frame lies on the desktop and how it is scaled: the one place that converts
/// frame pixels to desktop coordinates and back to the fractions QML takes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FrameToDesktop {
    origin: (f64, f64),
    scale: (f64, f64),
    frame: (u32, u32),
}

impl FrameToDesktop {
    /// `window` is the client area on the desktop, `region` the part of it that is captured (fractions),
    /// `frame` the size in pixels of what was captured.
    pub fn new(window: &WindowGeometry, region: NormRect, frame: (u32, u32)) -> Self {
        let (fw, fh) = (frame.0.max(1) as f64, frame.1.max(1) as f64);
        Self {
            origin: (window.x + region.x * window.w, window.y + region.y * window.h),
            scale: (region.w * window.w / fw, region.h * window.h / fh),
            frame,
        }
    }

    /// The scale of horizontal lengths (widths, letter spacing).
    pub fn scale(&self) -> LogicalScale {
        LogicalScale::new(self.scale.0 as f32)
    }

    /// The scale of vertical lengths (glyph and line heights). It differs from [`Self::scale`] when the
    /// frame is not shaped like the place it is shown at, e.g. while the game is changing its resolution.
    pub fn scale_y(&self) -> LogicalScale {
        LogicalScale::new(self.scale.1 as f32)
    }

    pub fn rect(&self, r: &Rect<FramePx>) -> DesktopRect {
        DesktopRect::in_space(
            (self.origin.0 + r.x as f64 * self.scale.0) as f32,
            (self.origin.1 + r.y as f64 * self.scale.1) as f32,
            (r.w as f64 * self.scale.0) as f32,
            (r.h as f64 * self.scale.1) as f32,
        )
    }

    /// A desktop rectangle as fractions of this region frame (QML draws the fill with these).
    pub fn fraction(&self, r: &DesktopRect) -> [f64; 4] {
        let (rw, rh) = ((self.frame.0.max(1) as f64 * self.scale.0).max(1e-6), (self.frame.1.max(1) as f64 * self.scale.1).max(1e-6));
        [(r.x as f64 - self.origin.0) / rw, (r.y as f64 - self.origin.1) / rh, r.w as f64 / rw, r.h as f64 / rh]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window() -> WindowGeometry {
        WindowGeometry { x: 100.0, y: 50.0, w: 1000.0, h: 500.0 }
    }

    #[test]
    fn a_crop_moves_into_the_frame_by_its_origin() {
        let line = Rect::<CropPx>::in_space(10.0, 5.0, 100.0, 20.0);
        assert_eq!(line.in_frame((40, 300)), Rect::new(50.0, 305.0, 100.0, 20.0));
        // And back: frame -> crop -> frame is the identity.
        assert_eq!(line.in_frame((40, 300)).in_crop((40, 300)), line);
    }

    #[test]
    fn frame_pixels_become_desktop_pixels_and_fractions_through_one_mapping() {
        // The lower half of the window, captured as a 2000x500 px frame (a 2x scaled screen).
        let map = FrameToDesktop::new(&window(), NormRect { x: 0.0, y: 0.5, w: 1.0, h: 0.5 }, (2000, 500));
        assert_eq!(map.scale(), LogicalScale::new(0.5));
        let on_desktop = map.rect(&Rect::new(400.0, 100.0, 200.0, 50.0));
        assert_eq!(on_desktop, DesktopRect::in_space(300.0, 350.0, 100.0, 25.0), "origin y = 50 + 0.5 * 500, plus 100 px at half scale");
        // And back to the fractions of the frame, whatever the scale was.
        let [x, y, w, h] = map.fraction(&on_desktop);
        assert!((x - 0.2).abs() < 1e-6 && (y - 0.2).abs() < 1e-6 && (w - 0.1).abs() < 1e-6 && (h - 0.1).abs() < 1e-6, "{x} {y} {w} {h}");
        assert_eq!(Rect::new(400.0, 100.0, 200.0, 50.0).fraction_of((2000, 500)), [0.2, 0.2, 0.1, 0.1]);
    }

    #[test]
    fn each_axis_has_its_own_scale() {
        // A 1000×500 frame shown 1000×1000: lengths along y are doubled, along x they are not.
        let map = FrameToDesktop::new(&WindowGeometry { x: 0.0, y: 0.0, w: 1000.0, h: 1000.0 }, NormRect { x: 0.0, y: 0.0, w: 1.0, h: 1.0 }, (1000, 500));
        assert_eq!((map.scale(), map.scale_y()), (LogicalScale::new(1.0), LogicalScale::new(2.0)));
    }

    #[test]
    fn clipping_keeps_the_common_part_or_nothing() {
        let screen = DesktopRect::in_space(0.0, 0.0, 1920.0, 1080.0);
        assert_eq!(DesktopRect::in_space(-300.0, 100.0, 1200.0, 400.0).clipped_to(&screen), DesktopRect::in_space(0.0, 100.0, 900.0, 400.0));
        assert_eq!(DesktopRect::in_space(-300.0, 100.0, 200.0, 400.0).clipped_to(&screen).area(), 0.0);
        assert!(screen.contains(&DesktopRect::in_space(1919.6, 0.0, 0.8, 1.0)), "half a pixel of tolerance");
        assert!(!screen.contains(&DesktopRect::in_space(1919.0, 0.0, 2.0, 1.0)));
    }

    #[test]
    fn lengths_cross_between_frame_and_screen_only_through_the_scale() {
        let half = LogicalScale::new(0.5);
        assert_eq!(half.px(14.0), 7.0);
        assert_eq!(half.frame_px(7.0), 14.0);
        assert!(LogicalScale::new(0.0).frame_px(1.0).is_finite(), "a broken scale does not divide by zero");
        assert_eq!(half.key(), LogicalScale::new(0.5).key());
        assert_ne!(half.key(), LogicalScale::new(1.0).key());
    }

    #[test]
    fn rectangle_operations_stay_in_their_space() {
        let a = DesktopRect::in_space(0.0, 0.0, 10.0, 10.0);
        let b = DesktopRect::in_space(5.0, 5.0, 10.0, 10.0);
        let u: DesktopRect = a.union(&b);
        assert_eq!((u.w, u.h), (15.0, 15.0));
        assert_eq!(a.intersection(&b), 25.0);
        assert!((a.iou(&b) - 25.0 / 175.0).abs() < 1e-6);
        assert_eq!(DesktopRect::of_window(&window()), DesktopRect::in_space(100.0, 50.0, 1000.0, 500.0));
    }

    #[test]
    fn the_space_is_not_part_of_the_data() {
        // Config and test fixtures keep the plain four numbers.
        let json = serde_json::to_string(&Rect::new(1.0, 2.0, 3.0, 4.0)).unwrap();
        assert_eq!(json, r#"{"x":1.0,"y":2.0,"w":3.0,"h":4.0}"#);
        let back: CropRect = serde_json::from_str(&json).unwrap();
        assert_eq!(back, Rect::in_space(1.0, 2.0, 3.0, 4.0));
    }
}
