//! Запасной бэкенд: xdg-desktop-portal ScreenCast + PipeWire. Работает на любом композиторе с порталом
//! (KDE, GNOME, Hyprland и т. д.), в том числе без доступа к KWin ScreenShot2.
//!
//! Порядок: `CreateSession` → `SelectSources` → `Start` (системный диалог выбора окна, согласие пользователя)
//! → `OpenPipeWireRemote`. Кадры читает долгоживущий `gst-launch-1.0` (`pipewiresrc` → PNG в stdout):
//! он сам договаривается о формате буферов с PipeWire, а нам остаётся нарезать поток на PNG и хранить последний.
//! Поток portal отдаёт кадры только при изменении содержимого, поэтому последний кадр всегда актуален.
//!
//! `restore_token` сохраняется в настройках: при следующем запуске окно выбирается без диалога.

use super::{Capture, CaptureError, rect_px};
use crate::settings::{NormRect, WindowKey};
use futures_util::StreamExt;
use image::{DynamicImage, ImageFormat};
use std::collections::HashMap;
use std::os::fd::{AsRawFd, OwnedFd};
use std::process::Stdio;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::process::{Child, Command};
use tokio::sync::{Mutex, Notify, watch};
use zbus::zvariant::{ObjectPath, OwnedObjectPath, OwnedValue, Value};

/// Ключ окна у этого бэкенда: само окно выбирается в диалоге портала, а не по uuid KWin.
pub const PORTAL_UUID: &str = "portal:window";

const DEST: &str = "org.freedesktop.portal.Desktop";
const PATH: &str = "/org/freedesktop/portal/desktop";
const SCREENCAST: &str = "org.freedesktop.portal.ScreenCast";
/// Диалог выбора ждёт пользователя.
const DIALOG_TIMEOUT: Duration = Duration::from_secs(180);
const FIRST_FRAME_TIMEOUT: Duration = Duration::from_secs(5);
/// Не чаще стольких кадров в секунду кодируем в PNG (иначе анимированное окно съест процессор).
const MAX_FPS: u32 = 4;

fn err(e: impl std::fmt::Display) -> CaptureError {
    CaptureError::Unavailable(e.to_string())
}

pub fn is_portal_window(w: &WindowKey) -> bool {
    w.uuid == PORTAL_UUID
}

pub fn portal_window_key() -> WindowKey {
    WindowKey { uuid: PORTAL_UUID.into(), resource_class: "xdg-desktop-portal".into(), caption: "Окно (portal)".into() }
}

// ───────────────────────── нарезка потока PNG ─────────────────────────

const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
/// Защита от разрастания буфера при мусоре в потоке.
const MAX_BUFFER: usize = 128 << 20;

/// Режет непрерывный поток склеенных PNG на отдельные файлы (по чанкам до `IEND`).
#[derive(Default)]
pub struct PngSplitter {
    buf: Vec<u8>,
}

impl PngSplitter {
    pub fn push(&mut self, data: &[u8]) -> Vec<Vec<u8>> {
        self.buf.extend_from_slice(data);
        let mut out = Vec::new();
        loop {
            if self.buf.len() < PNG_SIGNATURE.len() {
                break;
            }
            if self.buf[..8] != PNG_SIGNATURE {
                // Рассинхронизация: ищем следующую сигнатуру.
                match self.buf.windows(8).position(|w| w == PNG_SIGNATURE) {
                    Some(i) => {
                        self.buf.drain(..i);
                    }
                    None => {
                        let keep = self.buf.len() - 7;
                        self.buf.drain(..keep);
                        break;
                    }
                }
                continue;
            }
            match self.end_of_png() {
                Some(end) => out.push(self.buf.drain(..end).collect()),
                None => break,
            }
        }
        if self.buf.len() > MAX_BUFFER {
            self.buf.clear();
        }
        out
    }

    /// Длина первого полного PNG в буфере.
    fn end_of_png(&self) -> Option<usize> {
        let mut pos = 8;
        loop {
            let header = self.buf.get(pos..pos + 8)?;
            let len = u32::from_be_bytes(header[..4].try_into().ok()?) as usize;
            let next = pos.checked_add(12)?.checked_add(len)?; // длина + тип + данные + CRC
            if next > self.buf.len() {
                return None;
            }
            if &header[4..8] == b"IEND" {
                return Some(next);
            }
            pos = next;
        }
    }
}

// ───────────────────────── сессия ─────────────────────────

struct Frames {
    latest: StdMutex<Option<Arc<Vec<u8>>>>,
    notify: Notify,
    closed: StdMutex<Option<String>>,
}

struct Session {
    /// Закрытие соединения закрывает и сессию портала.
    _conn: zbus::Connection,
    child: Child,
    frames: Arc<Frames>,
    reader: tokio::task::JoinHandle<()>,
    stderr_reader: tokio::task::JoinHandle<()>,
    stderr: Arc<StdMutex<Vec<u8>>>,
}

impl Drop for Session {
    fn drop(&mut self) {
        self.reader.abort();
        self.stderr_reader.abort();
        let _ = self.child.start_kill();
    }
}

fn checked_frame(child: &mut Child, frames: &Frames, stderr: &[u8]) -> Result<Option<Arc<Vec<u8>>>, CaptureError> {
    let reason = match child.try_wait() {
        Ok(Some(status)) => Some(format!("gst-launch-1.0 завершился ({status})")),
        Err(e) => Some(format!("не удалось проверить gst-launch-1.0: {e}")),
        Ok(None) => frames.closed.lock().unwrap().clone(),
    };
    if let Some(reason) = reason {
        let detail = String::from_utf8_lossy(stderr);
        return Err(err(format!("Portal-захват остановлен: {reason}. Выберите окно заново. Проверьте PipeWire и плагины GStreamer (pipewiresrc, pngenc). {}", detail.trim())));
    }
    Ok(frames.latest.lock().unwrap().clone())
}

pub struct PortalCapture {
    session: Mutex<Option<Session>>,
    token: watch::Sender<String>,
}

type Dict = HashMap<String, OwnedValue>;

fn new_token(prefix: &str) -> String {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    format!("{prefix}{}_{}", std::process::id(), N.fetch_add(1, Ordering::Relaxed))
}

/// Вызов метода портала, отвечающего через `org.freedesktop.portal.Request::Response`.
/// Подписка создаётся до вызова: ответ может прийти раньше, чем вернётся сам вызов.
async fn request<B>(conn: &zbus::Connection, iface: &str, method: &str, body: &B, token: &str) -> Result<Dict, CaptureError>
where
    B: serde::Serialize + zbus::zvariant::DynamicType,
{
    let sender = conn.unique_name().ok_or_else(|| err("нет имени соединения"))?.as_str().trim_start_matches(':').replace('.', "_");
    let path = format!("/org/freedesktop/portal/desktop/request/{sender}/{token}");
    let proxy = zbus::Proxy::new(conn, DEST, path, "org.freedesktop.portal.Request").await.map_err(err)?;
    let mut responses = proxy.receive_signal("Response").await.map_err(err)?;
    conn.call_method(Some(DEST), PATH, Some(iface), method, body).await.map_err(err)?;
    let msg = tokio::time::timeout(DIALOG_TIMEOUT, responses.next())
        .await
        .map_err(|_| err("портал не ответил вовремя"))?
        .ok_or_else(|| err("портал закрыл соединение"))?;
    let (code, results): (u32, Dict) = msg.body().deserialize().map_err(err)?;
    match code {
        0 => Ok(results),
        1 => Err(err("выбор отменён")),
        _ => Err(err(format!("портал отклонил запрос (код {code})"))),
    }
}

fn str_of(v: &OwnedValue) -> Option<String> {
    <&str>::try_from(&**v).ok().map(String::from).or_else(|| <ObjectPath>::try_from(&**v).ok().map(|p| p.to_string()))
}

/// Узел PipeWire и новый restore token после успешного `Start`.
struct Started {
    node: u32,
    fd: OwnedFd,
    token: Option<String>,
    conn: zbus::Connection,
}

async fn start_portal(restore_token: &str) -> Result<Started, CaptureError> {
    let conn = zbus::Connection::session().await.map_err(err)?;

    let t = new_token("lipa_s");
    let st = new_token("lipa_ss");
    let mut o: HashMap<&str, Value> = HashMap::new();
    o.insert("handle_token", Value::from(t.as_str()));
    o.insert("session_handle_token", Value::from(st.as_str()));
    let r = request(&conn, SCREENCAST, "CreateSession", &(o,), &t).await?;
    let session: OwnedObjectPath = r
        .get("session_handle")
        .and_then(str_of)
        .ok_or_else(|| err("портал не вернул session_handle"))?
        .try_into()
        .map_err(err)?;

    let t = new_token("lipa_sel");
    let mut o: HashMap<&str, Value> = HashMap::new();
    o.insert("handle_token", Value::from(t.as_str()));
    o.insert("types", Value::from(2u32)); // Только окно: рамки и overlay приложения не должны попадать в OCR.
    o.insert("multiple", Value::from(false));
    o.insert("cursor_mode", Value::from(1u32)); // курсор не рисовать
    o.insert("persist_mode", Value::from(2u32)); // помнить выбор до явного отзыва
    if !restore_token.is_empty() {
        o.insert("restore_token", Value::from(restore_token));
    }
    request(&conn, SCREENCAST, "SelectSources", &(&session, o), &t).await?;

    let t = new_token("lipa_start");
    let mut o: HashMap<&str, Value> = HashMap::new();
    o.insert("handle_token", Value::from(t.as_str()));
    let r = request(&conn, SCREENCAST, "Start", &(&session, "", o), &t).await?;
    let streams: Vec<(u32, Dict)> = r
        .get("streams")
        .ok_or_else(|| err("портал не вернул потоки"))?
        .try_clone()
        .map_err(err)?
        .try_into()
        .map_err(err)?;
    let node = streams.first().map(|(n, _)| *n).ok_or_else(|| err("портал не вернул ни одного потока"))?;
    let token = r.get("restore_token").and_then(str_of);

    let reply = conn
        .call_method(Some(DEST), PATH, Some(SCREENCAST), "OpenPipeWireRemote", &(&session, HashMap::<&str, Value>::new()))
        .await
        .map_err(err)?;
    let fd: zbus::zvariant::OwnedFd = reply.body().deserialize().map_err(err)?;
    Ok(Started { node, fd: fd.into(), token, conn })
}

/// Конвейер GStreamer: узел PipeWire → PNG в stdout, не чаще `MAX_FPS` кадров в секунду.
fn gst_args(fd: i32, node: u32) -> Vec<String> {
    let s = |v: &str| v.to_string();
    vec![
        s("-q"),
        s("pipewiresrc"),
        format!("fd={fd}"),
        format!("path={node}"),
        s("!"),
        s("videorate"),
        s("drop-only=true"),
        format!("max-rate={MAX_FPS}"),
        s("!"),
        s("videoconvert"),
        s("!"),
        s("video/x-raw,format=RGBA"),
        s("!"),
        s("pngenc"),
        s("compression-level=1"),
        s("!"),
        s("fdsink"),
        s("fd=1"),
    ]
}

/// Делает дескриптор доступным дочернему процессу: по умолчанию он закрывается при `exec` (CLOEXEC).
fn inherit_fd(cmd: &mut Command, raw: i32) {
    // SAFETY: в дочернем процессе до exec вызывается только fcntl (async-signal-safe); дескриптор принадлежит нам.
    unsafe {
        cmd.pre_exec(move || {
            if libc::fcntl(raw, libc::F_SETFD, 0) == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
}

async fn spawn_session(restore_token: &str) -> Result<(Session, Option<String>), CaptureError> {
    let started = start_portal(restore_token).await?;
    let raw = started.fd.as_raw_fd();
    let mut cmd = Command::new("gst-launch-1.0");
    cmd.args(gst_args(raw, started.node)).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true);
    inherit_fd(&mut cmd, raw);
    let mut child = cmd.spawn().map_err(|e| {
        err(format!("не удалось запустить gst-launch-1.0 (нужны gstreamer и gst-plugin-pipewire): {e}"))
    })?;
    drop(started.fd); // у дочернего процесса своя копия
    let mut stdout = child.stdout.take().ok_or_else(|| err("нет stdout у gst-launch-1.0"))?;

    let mut stderr_pipe = child.stderr.take().ok_or_else(|| err("нет stderr у gst-launch-1.0"))?;
    let stderr = Arc::new(StdMutex::new(Vec::new()));
    let errors = stderr.clone();
    let stderr_reader = tokio::spawn(async move {
        let mut buf = [0u8; 2048];
        while let Ok(n) = stderr_pipe.read(&mut buf).await {
            if n == 0 { break; }
            let mut tail = errors.lock().unwrap();
            tail.extend_from_slice(&buf[..n]);
            let excess = tail.len().saturating_sub(8192);
            tail.drain(..excess);
        }
    });
    let frames = Arc::new(Frames { latest: StdMutex::new(None), notify: Notify::new(), closed: StdMutex::new(None) });
    let sink = frames.clone();
    let reader = tokio::spawn(async move {
        let (mut splitter, mut chunk) = (PngSplitter::default(), vec![0u8; 64 * 1024]);
        let reason = loop {
            let n = match stdout.read(&mut chunk).await {
                Ok(0) => break "поток кадров GStreamer закрыт".to_string(),
                Ok(n) => n,
                Err(e) => break format!("ошибка чтения кадров GStreamer: {e}"),
            };
            if let Some(last) = splitter.push(&chunk[..n]).pop() {
                *sink.latest.lock().unwrap() = Some(Arc::new(last));
                sink.notify.notify_waiters();
            }
        };
        *sink.closed.lock().unwrap() = Some(reason);
        sink.notify.notify_waiters();
    });
    Ok((Session { _conn: started.conn, child, frames, reader, stderr_reader, stderr }, started.token))
}

impl PortalCapture {
    pub fn new(restore_token: String) -> Self {
        Self { session: Mutex::new(None), token: watch::channel(restore_token).0 }
    }

    /// Stop the reader and GStreamer when the UI exits.
    pub async fn close(&self) {
        *self.session.lock().await = None;
    }

    /// Изменения restore token (для сохранения в настройках).
    pub fn token_updates(&self) -> watch::Receiver<String> {
        self.token.subscribe()
    }

    fn remember(&self, token: Option<String>) {
        if let Some(t) = token.filter(|t| !t.is_empty()) {
            self.token.send_replace(t);
        }
    }

    /// Пользователь выбирает окно в диалоге портала. Прежняя сессия закрывается.
    pub async fn select_window(&self) -> Result<WindowKey, CaptureError> {
        let mut slot = self.session.lock().await;
        *slot = None;
        let (session, token) = spawn_session("").await?;
        *slot = Some(session);
        self.remember(token);
        Ok(portal_window_key())
    }

    /// Не переоткрываем портал в цикле после сбоя: ошибка сохраняется до нового выбора окна.
    async fn latest_png(&self) -> Result<Arc<Vec<u8>>, CaptureError> {
        let mut slot = self.session.lock().await;
        if slot.is_none() {
            let saved = self.token.borrow().clone();
            let (session, token) = spawn_session(&saved).await?;
            *slot = Some(session);
            self.remember(token);
        }
        let session = slot.as_mut().expect("session created");
        let frames = session.frames.clone();
        let deadline = tokio::time::Instant::now() + FIRST_FRAME_TIMEOUT;
        loop {
            let notified = frames.notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            // Check the process before returning even a cached frame.
            if let Some(f) = checked_frame(&mut session.child, &frames, &session.stderr.lock().unwrap())? { return Ok(f); }
            if tokio::time::Instant::now() >= deadline {
                return Err(err("PipeWire не передал первый кадр за 5 секунд. Проверьте, что выбранное окно открыто и не свёрнуто."));
            }
            tokio::select! {
                _ = notified => {},
                _ = tokio::time::sleep(Duration::from_millis(50)) => {},
            }
        }
    }

    /// Полный кадр выбранного окна.
    pub async fn grab_full(&self) -> Result<DynamicImage, CaptureError> {
        let png = self.latest_png().await?;
        tokio::task::spawn_blocking(move || image::load_from_memory_with_format(&png, ImageFormat::Png))
            .await
            .map_err(err)?
            .map_err(err)
    }
}

impl Capture for PortalCapture {
    async fn grab(&self, window: &WindowKey, rect: NormRect) -> Result<DynamicImage, CaptureError> {
        if !is_portal_window(window) {
            return Err(CaptureError::WindowGone);
        }
        let full = self.grab_full().await?;
        let (x, y, w, h) = rect_px(rect, full.width(), full.height());
        Ok(full.crop_imm(x, y, w, h))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};
    use std::io::Cursor;

    fn png(w: u32, h: u32, v: u8) -> Vec<u8> {
        let mut out = Vec::new();
        // Шумовая картинка: несколько чанков IDAT не гарантированы, но размер достаточно велик для проверки границ.
        let img = RgbaImage::from_fn(w, h, |x, y| Rgba([(x as u8).wrapping_mul(v), (y as u8) ^ v, v, 255]));
        DynamicImage::ImageRgba8(img).write_to(&mut Cursor::new(&mut out), ImageFormat::Png).unwrap();
        out
    }

    #[tokio::test]
    async fn failed_process_is_reported_even_with_a_cached_frame() {
        let child = Command::new("sh").args(["-c", "exit 7"]).spawn().unwrap();
        let frames = Arc::new(Frames {
            latest: StdMutex::new(Some(Arc::new(png(4, 4, 1)))),
            notify: Notify::new(),
            closed: StdMutex::new(None),
        });
        let mut child = child;
        child.wait().await.unwrap();
        let error = checked_frame(&mut child, &frames, b"missing pipewiresrc").unwrap_err().to_string();
        assert!(error.contains("7"));
        assert!(error.contains("missing pipewiresrc"));
        assert!(error.contains("Выберите окно заново"));
    }

    #[tokio::test]
    async fn closed_stream_invalidates_a_cached_frame_before_process_exit() {
        let mut child = Command::new("sleep").arg("10").kill_on_drop(true).spawn().unwrap();
        let frames = Frames {
            latest: StdMutex::new(Some(Arc::new(png(4, 4, 1)))),
            notify: Notify::new(),
            closed: StdMutex::new(None),
        };
        assert!(checked_frame(&mut child, &frames, b"").unwrap().is_some());
        *frames.closed.lock().unwrap() = Some("поток кадров закрыт".into());
        assert!(checked_frame(&mut child, &frames, b"").unwrap_err().to_string().contains("поток кадров закрыт"));
        child.kill().await.unwrap();
    }

    #[test]
    fn splits_concatenated_pngs_fed_in_small_pieces() {
        let (a, b, c) = (png(40, 30, 3), png(200, 120, 7), png(8, 8, 9));
        let stream: Vec<u8> = [a.clone(), b.clone(), c.clone()].concat();
        let mut sp = PngSplitter::default();
        let mut got = Vec::new();
        for piece in stream.chunks(37) {
            got.extend(sp.push(piece));
        }
        assert_eq!(got, vec![a, b, c]);
    }

    #[test]
    fn incomplete_png_is_held_back() {
        let a = png(50, 50, 5);
        let mut sp = PngSplitter::default();
        assert!(sp.push(&a[..a.len() - 5]).is_empty());
        assert_eq!(sp.push(&a[a.len() - 5..]), vec![a]);
    }

    #[test]
    fn resyncs_after_garbage() {
        let a = png(10, 10, 1);
        let mut data = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12];
        data.extend(&a);
        assert_eq!(PngSplitter::default().push(&data), vec![a]);
    }

    #[test]
    fn decoded_frame_is_valid() {
        let a = png(16, 9, 2);
        let img = image::load_from_memory_with_format(&a, ImageFormat::Png).unwrap();
        assert_eq!((img.width(), img.height()), (16, 9));
    }

    #[test]
    fn pipeline_description_passes_fd_and_node() {
        let a = gst_args(7, 42);
        assert!(a.contains(&"fd=7".to_string()) && a.contains(&"path=42".to_string()));
        assert!(a.contains(&format!("max-rate={MAX_FPS}")));
    }

    #[tokio::test]
    async fn child_inherits_the_pipewire_fd() {
        use std::io::Read;
        let (mut reader, writer) = std::io::pipe().unwrap();
        let fd: OwnedFd = writer.into(); // как fd от OpenPipeWireRemote: с флагом CLOEXEC
        let raw = fd.as_raw_fd();
        let mut cmd = Command::new("sh");
        cmd.args(["-c", &format!("echo через-fd >&{raw}")]);
        inherit_fd(&mut cmd, raw);
        assert!(cmd.status().await.unwrap().success());
        drop(fd);
        let mut got = String::new();
        reader.read_to_string(&mut got).unwrap();
        assert_eq!(got.trim(), "через-fd");
    }

    #[test]
    fn portal_key_is_recognised() {
        assert!(is_portal_window(&portal_window_key()));
        assert!(!is_portal_window(&WindowKey { uuid: "abc".into(), resource_class: String::new(), caption: String::new() }));
    }
}
