//! Бэкенд KWin: `org.kde.KWin.ScreenShot2` для захвата содержимого одного окна
//! (чужие окна поверх и наш overlay в кадр не попадают) и `org.kde.KWin.queryWindowInfo`
//! для интерактивного выбора окна кликом.
//!
//! Требование безопасности KWin: ScreenShot2 доступен только приложению, чей
//! `.desktop` перечисляет интерфейс в `X-KDE-DBUS-Restricted-Interfaces`
//! (см. packaging/io.lipa.Translator.desktop). Обходов модели безопасности нет.

use super::{Capture, CaptureError, rect_px};
use crate::settings::{NormRect, WindowKey};
use image::{DynamicImage, RgbaImage};
use std::collections::HashMap;
use std::io::Read;
use std::os::fd::OwnedFd;
use zbus::zvariant::{Fd, OwnedValue, Value};

pub struct KwinCapture {
    conn: zbus::Connection,
}

fn unavailable(e: impl std::fmt::Display) -> CaptureError {
    CaptureError::Unavailable(e.to_string())
}

fn string(m: &HashMap<String, OwnedValue>, k: &str) -> String {
    m.get(k).and_then(|v| <&str>::try_from(&**v).ok()).unwrap_or_default().to_string()
}

fn uint(m: &HashMap<String, OwnedValue>, k: &str) -> Option<u32> {
    let v = &**m.get(k)?;
    u32::try_from(v).ok().or_else(|| u64::try_from(v).ok().map(|x| x as u32)).or_else(|| i32::try_from(v).ok().map(|x| x as u32))
}

/// Прямоугольник окна на рабочем столе.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WindowGeometry {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl WindowGeometry {
    /// Прямоугольник области, заданной долями окна.
    pub fn region(&self, r: NormRect) -> WindowGeometry {
        WindowGeometry { x: self.x + r.x * self.w, y: self.y + r.y * self.h, w: r.w * self.w, h: r.h * self.h }
    }
}

fn number(m: &HashMap<String, OwnedValue>, k: &str) -> Option<f64> {
    let v = &**m.get(k)?;
    f64::try_from(v)
        .ok()
        .or_else(|| i32::try_from(v).ok().map(f64::from))
        .or_else(|| u32::try_from(v).ok().map(f64::from))
        .or_else(|| i64::try_from(v).ok().map(|x| x as f64))
}

fn geometry_from(m: &HashMap<String, OwnedValue>) -> Option<WindowGeometry> {
    let g = WindowGeometry { x: number(m, "x")?, y: number(m, "y")?, w: number(m, "width")?, h: number(m, "height")? };
    (g.w > 0.0 && g.h > 0.0).then_some(g)
}

impl KwinCapture {
    pub async fn connect() -> Result<Self, CaptureError> {
        let conn = zbus::Connection::session().await.map_err(unavailable)?;
        Ok(Self { conn })
    }

    /// Пользователь кликает по окну; возвращает его ключ. Отмена (Esc) даёт `None`.
    pub async fn pick_window(&self) -> Result<Option<WindowKey>, CaptureError> {
        let reply = self
            .conn
            .call_method(Some("org.kde.KWin"), "/KWin", Some("org.kde.KWin"), "queryWindowInfo", &())
            .await;
        let reply = match reply {
            Ok(r) => r,
            // Отмена выбора приходит как D-Bus ошибка UserCancel.
            Err(zbus::Error::MethodError(name, _, _)) if name.as_str().contains("Cancel") => return Ok(None),
            Err(e) => return Err(unavailable(e)),
        };
        let info: HashMap<String, OwnedValue> = reply.body().deserialize().map_err(unavailable)?;
        let uuid = string(&info, "uuid");
        if uuid.is_empty() {
            return Ok(None);
        }
        Ok(Some(WindowKey {
            uuid,
            resource_class: string(&info, "resourceClass"),
            caption: string(&info, "caption"),
        }))
    }

    /// Окно ещё существует (KWin знает его uuid)?
    pub async fn window_exists(&self, uuid: &str) -> bool {
        match self
            .conn
            .call_method(Some("org.kde.KWin"), "/KWin", Some("org.kde.KWin"), "getWindowInfo", &(uuid,))
            .await
        {
            Ok(r) => r
                .body()
                .deserialize::<HashMap<String, OwnedValue>>()
                .map(|m| !string(&m, "uuid").is_empty())
                .unwrap_or(false),
            Err(_) => false,
        }
    }

    /// Положение и размер окна на рабочем столе (логические координаты KWin), если окно существует.
    pub async fn window_geometry(&self, uuid: &str) -> Option<WindowGeometry> {
        let reply = self
            .conn
            .call_method(Some("org.kde.KWin"), "/KWin", Some("org.kde.KWin"), "getWindowInfo", &(uuid,))
            .await
            .ok()?;
        geometry_from(&reply.body().deserialize::<HashMap<String, OwnedValue>>().ok()?)
    }

    /// Полный кадр окна.
    pub async fn grab_window(&self, uuid: &str) -> Result<DynamicImage, CaptureError> {
        let (reader, writer) = std::io::pipe().map_err(unavailable)?;
        // Читаем параллельно с вызовом: кадр больше буфера pipe, иначе взаимная блокировка.
        let read = tokio::task::spawn_blocking(move || {
            let mut buf = Vec::new();
            let mut reader = reader;
            reader.read_to_end(&mut buf).map(|_| buf)
        });
        let fd: OwnedFd = writer.into();

        let mut options: HashMap<&str, Value> = HashMap::new();
        options.insert("include-cursor", false.into());
        options.insert("include-decoration", false.into());
        options.insert("native-resolution", true.into());

        let reply = self
            .conn
            .call_method(
                Some("org.kde.KWin"),
                "/org/kde/KWin/ScreenShot2",
                Some("org.kde.KWin.ScreenShot2"),
                "CaptureWindow",
                &(uuid, options, Fd::from(&fd)),
            )
            .await;
        // Закрываем свою копию записывающего конца, чтобы читатель получил EOF.
        drop(fd);
        let reply = match reply {
            Ok(r) => r,
            Err(e) => {
                let _ = read.await;
                return Err(match &e {
                    zbus::Error::MethodError(n, _, _) if n.as_str().contains("InvalidArgs") => CaptureError::WindowGone,
                    _ => unavailable(e),
                });
            }
        };
        let meta: HashMap<String, OwnedValue> = reply.body().deserialize().map_err(unavailable)?;
        let data = read.await.map_err(unavailable)?.map_err(unavailable)?;
        decode(&meta, data)
    }
}

/// Сырые данные KWin (QImage::Format) -> RGBA.
fn decode(meta: &HashMap<String, OwnedValue>, data: Vec<u8>) -> Result<DynamicImage, CaptureError> {
    let (w, h, stride, format) = (
        uint(meta, "width"),
        uint(meta, "height"),
        uint(meta, "stride"),
        uint(meta, "format"),
    );
    let (Some(w), Some(h), Some(stride), Some(format)) = (w, h, stride, format) else {
        return Err(unavailable("в ответе KWin нет параметров изображения"));
    };
    rgba_from_raw(w, h, stride, format, &data)
}

pub fn rgba_from_raw(w: u32, h: u32, stride: u32, format: u32, data: &[u8]) -> Result<DynamicImage, CaptureError> {
    // 4=RGB32, 5=ARGB32, 6=ARGB32_Premultiplied: в памяти B,G,R,A (little endian).
    // 16=RGBX8888, 17=RGBA8888, 18=RGBA8888_Premultiplied: в памяти R,G,B,A.
    let bgr = match format {
        4..=6 => true,
        16..=18 => false,
        f => return Err(unavailable(format!("неподдерживаемый формат пикселей {f}"))),
    };
    let (w_, h_, stride_) = (w as usize, h as usize, stride as usize);
    if w == 0 || h == 0 || stride_ < w_ * 4 || data.len() < stride_ * (h_ - 1) + w_ * 4 {
        return Err(unavailable("данные кадра короче заявленных"));
    }
    let mut out = Vec::with_capacity(w_ * h_ * 4);
    for row in 0..h_ {
        for px in data[row * stride_..row * stride_ + w_ * 4].chunks_exact(4) {
            let (r, g, b) = if bgr { (px[2], px[1], px[0]) } else { (px[0], px[1], px[2]) };
            out.extend_from_slice(&[r, g, b, 255]);
        }
    }
    RgbaImage::from_raw(w, h, out)
        .map(DynamicImage::ImageRgba8)
        .ok_or_else(|| unavailable("не удалось собрать изображение"))
}

impl Capture for KwinCapture {
    async fn grab(&self, window: &WindowKey, rect: NormRect) -> Result<DynamicImage, CaptureError> {
        let full = self.grab_window(&window.uuid).await?;
        let (x, y, w, h) = rect_px(rect, full.width(), full.height());
        Ok(full.crop_imm(x, y, w, h))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::GenericImageView;

    #[test]
    fn region_inside_window() {
        let g = WindowGeometry { x: 100.0, y: 50.0, w: 800.0, h: 600.0 };
        let r = g.region(NormRect { x: 0.5, y: 0.25, w: 0.25, h: 0.5 });
        assert_eq!(r, WindowGeometry { x: 500.0, y: 200.0, w: 200.0, h: 300.0 });
    }

    #[test]
    fn geometry_parsing_accepts_int_and_double() {
        let mut m: HashMap<String, OwnedValue> = HashMap::new();
        m.insert("x".into(), Value::from(10i32).try_into().unwrap());
        m.insert("y".into(), Value::from(20.5f64).try_into().unwrap());
        m.insert("width".into(), Value::from(300u32).try_into().unwrap());
        m.insert("height".into(), Value::from(200i32).try_into().unwrap());
        assert_eq!(geometry_from(&m), Some(WindowGeometry { x: 10.0, y: 20.5, w: 300.0, h: 200.0 }));
        m.insert("width".into(), Value::from(0i32).try_into().unwrap());
        assert_eq!(geometry_from(&m), None);
    }

    #[test]
    fn bgra_with_stride_padding() {
        // 2x2, stride 12 (4 байта паддинга в строке); пиксель (0,0) = B10,G20,R30.
        let mut d = vec![0u8; 24];
        d[0..4].copy_from_slice(&[10, 20, 30, 255]);
        let img = rgba_from_raw(2, 2, 12, 6, &d).unwrap();
        assert_eq!(img.get_pixel(0, 0).0, [30, 20, 10, 255]);
        assert_eq!(img.dimensions(), (2, 2));
    }

    #[test]
    fn rejects_short_data_and_unknown_format() {
        assert!(rgba_from_raw(2, 2, 8, 6, &[0; 8]).is_err());
        assert!(rgba_from_raw(1, 1, 4, 99, &[0; 4]).is_err());
    }
}
