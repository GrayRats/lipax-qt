//! Бэкенд KWin: `org.kde.KWin.ScreenShot2` для захвата содержимого одного окна
//! (чужие окна поверх и наш overlay в кадр не попадают) и `org.kde.KWin.queryWindowInfo`
//! для интерактивного выбора окна кликом.
//!
//! Требование безопасности KWin: ScreenShot2 доступен только приложению, чей
//! `.desktop` перечисляет интерфейс в `X-KDE-DBUS-Restricted-Interfaces`
//! (см. packaging/io.lipa.Translator.desktop). Обходов модели безопасности нет.

use super::{Capture, CaptureError, rect_px};
use crate::settings::{NormRect, Settings, WindowKey};
use image::{DynamicImage, RgbaImage};
use std::collections::HashMap;
use std::io::Read;
use std::os::fd::OwnedFd;
use zbus::zvariant::{Fd, OwnedValue, Value};

pub struct KwinCapture {
    conn: zbus::Connection,
    geometry: tokio::sync::OnceCell<super::geometry::ClientGeometry>,
    /// Failed script loads are retried at most every `GEOMETRY_RETRY`, not on every frame.
    geometry_failed: std::sync::Mutex<Option<std::time::Instant>>,
}

const GEOMETRY_RETRY: std::time::Duration = std::time::Duration::from_secs(30);

fn unavailable(e: impl std::fmt::Display) -> CaptureError {
    CaptureError::Unavailable(e.to_string())
}

fn capture_error(error: zbus::Error) -> CaptureError {
    match &error {
        zbus::Error::MethodError(name, _, _) if name.as_str() == "org.kde.KWin.ScreenShot2.Error.NoAuthorized" => {
            let executable = std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_else(|_| "неизвестен".into());
            unavailable(format!("KWin не разрешил захват для {executable}. Разрешение связано с путём запуска: Exec в desktop-файле должен указывать на этот бинарник и содержать X-KDE-DBUS-Restricted-Interfaces=org.kde.KWin.ScreenShot2. Для установленной версии запустите /usr/bin/lipax; для локальной сборки используйте packaging/run-local.sh. После обновления пакета полностью перезапустите LipaX; при необходимости обновите кэш командой kbuildsycoca6."))
        }
        zbus::Error::MethodError(name, _, _) if matches!(name.as_str(),
            "org.kde.KWin.ScreenShot2.Error.InvalidWindow" | "org.freedesktop.DBus.Error.InvalidArgs") => CaptureError::WindowGone,
        _ => unavailable(error),
    }
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

impl From<[f64; 4]> for WindowGeometry {
    /// x, y, ширина, высота: так геометрия окна приходит в QML и обратно.
    fn from([x, y, w, h]: [f64; 4]) -> Self { Self { x, y, w, h } }
}

impl WindowGeometry {
    /// Прямоугольник области, заданной долями окна.
    pub fn region(&self, r: NormRect) -> WindowGeometry {
        WindowGeometry { x: self.x + r.x * self.w, y: self.y + r.y * self.h, w: r.w * self.w, h: r.h * self.h }
    }

    fn contains(&self, o: &WindowGeometry) -> bool {
        const EPS: f64 = 0.5;
        o.x >= self.x - EPS && o.y >= self.y - EPS && o.x + o.w <= self.x + self.w + EPS && o.y + o.h <= self.y + self.h + EPS
    }

    fn union(&self, o: &WindowGeometry) -> WindowGeometry {
        let (x, y) = (self.x.min(o.x), self.y.min(o.y));
        WindowGeometry { x, y, w: (self.x + self.w).max(o.x + o.w) - x, h: (self.y + self.h).max(o.y + o.h) - y }
    }
}

/// Прямоугольники окна в KWin: клиентская область (без рамки и заголовка KWin), рамка
/// с серверной декорацией и буфер (у окон с CSD — вместе с тенью).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WindowFrames {
    pub client: WindowGeometry,
    pub frame: WindowGeometry,
    pub buffer: WindowGeometry,
}

/// Где в кадре KWin лежит клиентская область, в пикселях кадра. Кадр соответствует одному
/// из прямоугольников окна (клиент, рамка, буфер или их объединение) в масштабе вывода:
/// находим его по размеру и вырезаем клиентскую часть. `None` — кадр уже равен клиентской
/// области или не совпал ни с одним прямоугольником (тогда он остаётся как есть).
pub fn client_crop(width: u32, height: u32, f: &WindowFrames) -> Option<(u32, u32, u32, u32)> {
    if width == 0 || height == 0 {
        return None;
    }
    let (iw, ih) = (width as f64, height as f64);
    for c in [f.client, f.frame, f.buffer, f.frame.union(&f.buffer)] {
        if !c.contains(&f.client) {
            continue;
        }
        let scale = iw / c.w;
        // Округление размеров при дробном масштабе — до 2 px.
        if !(0.5..=4.0).contains(&scale) || (ih - c.h * scale).abs() > 2.0 {
            continue;
        }
        let px = |v: f64, max: u32| (v * scale).round().clamp(0.0, max as f64) as u32;
        let (x, y) = (px(f.client.x - c.x, width - 1), px(f.client.y - c.y, height - 1));
        let (w, h) = (px(f.client.w, width - x).max(1), px(f.client.h, height - y).max(1));
        // Кадр уже совпадает с клиентской областью с точностью до округления.
        if x <= 1 && y <= 1 && width - w <= 2 && height - h <= 2 {
            return None;
        }
        return Some((x, y, w, h));
    }
    None
}

impl KwinCapture {
    pub async fn connect() -> Result<Self, CaptureError> {
        tracing::debug!("Подключение к KWin через сессионную D-Bus");
        let conn = zbus::Connection::session().await.map_err(unavailable)?;
        Ok(Self { conn, geometry: tokio::sync::OnceCell::new(), geometry_failed: Default::default() })
    }

    /// Пользователь кликает по окну; возвращает его ключ. Отмена (Esc) даёт `None`.
    pub async fn pick_window(&self) -> Result<Option<WindowKey>, CaptureError> {
        // queryWindowInfo itself is unrestricted. Check the actual capture interface first,
        // so Auto can offer the portal before asking the user to select a KWin window.
        self.check_capture_permission().await?;
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

    /// Probe permission without taking an image. KWin checks authorization before looking
    /// up the window; the null UUID cannot refer to a real window. The expected result is
    /// InvalidWindow, both on versions with desktop-file checks and newer KWin versions.
    pub async fn check_capture_permission(&self) -> Result<(), CaptureError> {
        let sink = std::fs::File::options().write(true).open("/dev/null").map_err(unavailable)?;
        let options: HashMap<&str, Value> = HashMap::new();
        let result = tokio::time::timeout(std::time::Duration::from_secs(5), self.conn.call_method(
            Some("org.kde.KWin"), "/org/kde/KWin/ScreenShot2", Some("org.kde.KWin.ScreenShot2"),
            "CaptureWindow", &("{00000000-0000-0000-0000-000000000000}", options, Fd::from(&sink)),
        )).await.map_err(|_| unavailable("KWin не ответил на проверку разрешения захвата за 5 секунд"))?;
        match result {
            Err(zbus::Error::MethodError(name, _, _)) if name.as_str() == "org.kde.KWin.ScreenShot2.Error.InvalidWindow" => Ok(()),
            Err(error) => Err(capture_error(error)),
            Ok(_) => Err(unavailable("KWin неожиданно принял пустой идентификатор окна при проверке разрешения")),
        }
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

    /// Скрипт KWin (геометрия окон и свободное окно перевода); загружается один раз.
    async fn tracker(&self) -> Option<&super::geometry::ClientGeometry> {
        if let Some(tracker) = self.geometry.get() {
            return Some(tracker);
        }
        if self.geometry_failed.lock().unwrap().is_some_and(|t| t.elapsed() < GEOMETRY_RETRY) {
            return None;
        }
        match self.geometry.get_or_try_init(super::geometry::ClientGeometry::connect).await {
            Ok(tracker) => Some(tracker),
            Err(e) => {
                tracing::warn!("KWin client geometry unavailable: {e}");
                *self.geometry_failed.lock().unwrap() = Some(std::time::Instant::now());
                None
            }
        }
    }

    /// Прямоугольники окна (логические координаты KWin), если окно существует.
    pub async fn window_frames(&self, uuid: &str) -> Option<WindowFrames> {
        // Без скрипта рамку по геометрии с декорацией не рисуем: она была бы неверной.
        self.tracker().await?.get(uuid)
    }

    /// Свободное окно перевода: куда его поставить при появлении (JSON для скрипта KWin).
    /// `false` — скрипт KWin недоступен (не KDE), позицию выбирает композитор.
    pub async fn set_floating_placement(&self, json: String) -> bool {
        match self.tracker().await {
            Some(t) => { t.set_floating_placement(json); true }
            None => false,
        }
    }

    /// Геометрия свободного окна перевода после перемещения пользователем (от KWin).
    pub async fn floating_moves(&self) -> Option<tokio::sync::watch::Receiver<Option<(crate::settings::FloatingGeometry, String)>>> {
        Some(self.tracker().await?.floating_moves())
    }

    /// Клиентская область окна на рабочем столе (без рамки и заголовка KWin).
    pub async fn window_geometry(&self, uuid: &str) -> Option<WindowGeometry> {
        self.window_frames(uuid).await.map(|f| f.client)
    }

    /// Кадр клиентской области окна: без рамки, заголовка, кнопок и теней.
    pub async fn grab_window(&self, uuid: &str) -> Result<DynamicImage, CaptureError> {
        let img = self.capture_window(uuid).await?;
        tracing::trace!(width = img.width(), height = img.height(), "Получен кадр KWin ScreenShot2");
        Ok(match self.window_frames(uuid).await.and_then(|f| client_crop(img.width(), img.height(), &f)) {
            Some((x, y, w, h)) => img.crop_imm(x, y, w, h),
            None => img,
        })
    }

    async fn capture_window(&self, uuid: &str) -> Result<DynamicImage, CaptureError> {
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
        // По умолчанию KWin добавляет тень: у окон без серверной рамки (CSD) кадр был бы шире окна.
        options.insert("include-shadow", false.into());
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
                return Err(capture_error(e));
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
        for px in data[row * stride_..row * stride_ + w_ * 4].as_chunks::<4>().0 {
            let (r, g, b) = if bgr { (px[2], px[1], px[0]) } else { (px[0], px[1], px[2]) };
            out.extend_from_slice(&[r, g, b, 255]);
        }
    }
    RgbaImage::from_raw(w, h, out)
        .map(DynamicImage::ImageRgba8)
        .ok_or_else(|| unavailable("не удалось собрать изображение"))
}

impl Capture for KwinCapture {
    fn capabilities(&self, _: &WindowKey, _: &Settings) -> super::CaptureCapabilities { super::CaptureCapabilities::KWIN }

    async fn grab(&self, window: &WindowKey, rect: NormRect) -> Result<DynamicImage, CaptureError> {
        let full = self.grab_window(&window.uuid).await?;
        let (x, y, w, h) = rect_px(rect, full.width(), full.height());
        Ok(full.crop_imm(x, y, w, h))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn method_error(name: &str) -> zbus::Error {
        let message = zbus::Message::method_call("/test", "Test").unwrap().build(&()).unwrap();
        zbus::Error::MethodError(name.to_owned().try_into().unwrap(), Some("test".into()), message)
    }

    #[test]
    fn capture_permission_error_explains_executable_registration() {
        let error = capture_error(method_error("org.kde.KWin.ScreenShot2.Error.NoAuthorized"));
        let message = error.to_string();
        assert!(matches!(error, CaptureError::Unavailable(_)));
        assert!(message.contains("packaging/run-local.sh"));
        assert!(message.contains("/usr/bin/lipax"));
        assert!(message.contains("X-KDE-DBUS-Restricted-Interfaces"));
        assert!(message.contains(&std::env::current_exe().unwrap().display().to_string()));
    }

    #[test]
    fn only_invalid_windows_are_reported_as_gone() {
        assert!(matches!(capture_error(method_error("org.kde.KWin.ScreenShot2.Error.InvalidWindow")), CaptureError::WindowGone));
        assert!(matches!(capture_error(method_error("org.freedesktop.DBus.Error.ServiceUnknown")), CaptureError::Unavailable(_)));
    }
    use image::GenericImageView;

    #[test]
    fn region_inside_window() {
        let g = WindowGeometry { x: 100.0, y: 50.0, w: 800.0, h: 600.0 };
        let r = g.region(NormRect { x: 0.5, y: 0.25, w: 0.25, h: 0.5 });
        assert_eq!(r, WindowGeometry { x: 500.0, y: 200.0, w: 200.0, h: 300.0 });
    }

    fn rect(x: f64, y: f64, w: f64, h: f64) -> WindowGeometry {
        WindowGeometry { x, y, w, h }
    }

    #[test]
    fn client_crop_strips_decoration_and_shadow() {
        // Серверная рамка KWin: 4 px по бокам, заголовок 30 px.
        let ssd = WindowFrames { client: rect(104.0, 80.0, 800.0, 600.0), frame: rect(100.0, 50.0, 808.0, 634.0), buffer: rect(104.0, 80.0, 800.0, 600.0) };
        assert_eq!(client_crop(800, 600, &ssd), None, "KWin already returned the client area");
        assert_eq!(client_crop(808, 634, &ssd), Some((4, 30, 800, 600)), "decoration included");
        assert_eq!(client_crop(1212, 951, &ssd), Some((6, 45, 1200, 900)), "fractional scale 1.5");

        // CSD с тенью 20 px в буфере (GTK, libdecor, XWayland с _GTK_FRAME_EXTENTS).
        let csd = WindowFrames { client: rect(0.0, 0.0, 640.0, 480.0), frame: rect(0.0, 0.0, 640.0, 480.0), buffer: rect(-20.0, -20.0, 680.0, 520.0) };
        assert_eq!(client_crop(1360, 1040, &csd), Some((40, 40, 1280, 960)), "shadow at scale 2");
        assert_eq!(client_crop(1280, 960, &csd), None);

        // Неизвестный размер кадра: не режем вслепую.
        assert_eq!(client_crop(1000, 1000, &ssd), None);
        assert_eq!(client_crop(0, 0, &ssd), None, "empty frame during resize");
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
