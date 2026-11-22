//! Sway (and other wlroots compositors that speak the i3 IPC): window list, client geometry, focus and window capture
//! (R-14 step 3). Geometry and focus come from the compositor's IPC (`GET_TREE`), the pixels from `grim`
//! (`wlr-screencopy`) cut to the client rectangle of the window.
//!
//! What it is: a backend that has been run in a headless sway (`tests/inplace/live/run_sway.sh`). What it is not: wired into the
//! window picker and the settings of the application yet, so `support_for` still calls the desktop "not verified" for the
//! application as a whole. Minimising does not exist in sway (a window is hidden by moving it to the scratchpad), so
//! `minimized` is a window that is not visible.

use super::{Capture, CaptureCapabilities, CaptureError, kwin::WindowGeometry, rect_px};
use crate::desktop::{DesktopBackend, WindowState};
use crate::settings::{NormRect, Settings, WindowKey};
use image::DynamicImage;
use serde_json::Value;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Prefix of the `uuid` of a sway window in the settings.
pub const KEY_PREFIX: &str = "sway:";
const IPC_MAGIC: &[u8; 6] = b"i3-ipc";
const GET_TREE: u32 = 4;
/// How long the compositor may take to answer the IPC or `grim`.
const TIMEOUT: Duration = Duration::from_secs(5);

fn unavailable(e: impl std::fmt::Display) -> CaptureError { CaptureError::Unavailable(e.to_string()) }

/// One application window of the tree.
#[derive(Debug, Clone, PartialEq)]
pub struct SwayWindow {
    /// The container id: the key of the window for the rest of the application (`WindowKey::uuid`).
    pub id: u64,
    /// `app_id` of a Wayland client, the class of an XWayland one.
    pub class: String,
    pub title: String,
    pub pid: u32,
    /// The client rectangle on the desktop (logical px): inside the container, without border and title bar.
    pub client: WindowGeometry,
    pub focused: bool,
    pub visible: bool,
}

impl SwayWindow {
    /// The key the settings keep for this window: `sway:<container id>`, which tells it from a KWin uuid and from the portal key.
    pub fn key(&self) -> WindowKey {
        WindowKey { uuid: format!("{KEY_PREFIX}{}", self.id), resource_class: self.class.clone(), caption: self.title.clone(),
            program: super::process::program_of_pid(self.pid).unwrap_or_default() }
    }
}

fn rect(v: &Value) -> Option<WindowGeometry> {
    Some(WindowGeometry { x: v["x"].as_f64()?, y: v["y"].as_f64()?, w: v["width"].as_f64()?, h: v["height"].as_f64()? })
}

/// The application windows of a `GET_TREE` reply: the leaves that have a process (containers, workspaces and outputs do not).
pub fn parse_tree(json: &str) -> Vec<SwayWindow> {
    fn walk(node: &Value, out: &mut Vec<SwayWindow>) {
        let children = ["nodes", "floating_nodes"].iter().filter_map(|k| node[*k].as_array()).flatten().collect::<Vec<_>>();
        for child in &children { walk(child, out); }
        if !children.is_empty() { return; }
        let (Some(id), Some(pid), Some(outer), Some(inner)) = (node["id"].as_u64(), node["pid"].as_u64(), rect(&node["rect"]), rect(&node["window_rect"])) else { return };
        let class = node["app_id"].as_str().filter(|c| !c.is_empty()).or_else(|| node["window_properties"]["class"].as_str()).unwrap_or_default();
        out.push(SwayWindow {
            id, class: class.to_owned(), title: node["name"].as_str().unwrap_or_default().to_owned(), pid: pid as u32,
            client: WindowGeometry { x: outer.x + inner.x, y: outer.y + inner.y, w: inner.w, h: inner.h },
            focused: node["focused"].as_bool().unwrap_or(false), visible: node["visible"].as_bool().unwrap_or(false),
        });
    }
    let mut out = Vec::new();
    if let Ok(tree) = serde_json::from_str::<Value>(json) { walk(&tree, &mut out); }
    out
}

/// Whether the key names a sway window (see [`SwayWindow::key`]).
pub fn is_sway_window(window: &WindowKey) -> bool { window.uuid.starts_with(KEY_PREFIX) }

/// How long [`SwayCapture::pick_window`] waits for the user to click the game.
pub const PICK_TIMEOUT: Duration = Duration::from_secs(20);

/// Sway, reached through `$SWAYSOCK`.
pub struct SwayCapture {
    socket: std::path::PathBuf,
}

impl SwayCapture {
    /// The sway of this session; `None` when `SWAYSOCK` is not set (another compositor).
    pub fn from_env() -> Option<Self> {
        std::env::var_os("SWAYSOCK").filter(|s| !s.is_empty()).map(|s| Self { socket: s.into() })
    }

    /// One request of the i3 IPC and its reply text.
    async fn ipc(&self, kind: u32) -> Result<String, CaptureError> {
        let exchange = async {
            let mut stream = tokio::net::UnixStream::connect(&self.socket).await?;
            let mut header = Vec::with_capacity(14);
            header.extend_from_slice(IPC_MAGIC);
            header.extend_from_slice(&0u32.to_ne_bytes());
            header.extend_from_slice(&kind.to_ne_bytes());
            stream.write_all(&header).await?;
            let mut reply = [0u8; 14];
            stream.read_exact(&mut reply).await?;
            if &reply[..6] != IPC_MAGIC { return Err(std::io::Error::other("not an i3 IPC reply")); }
            let mut payload = vec![0u8; u32::from_ne_bytes(reply[6..10].try_into().unwrap()) as usize];
            stream.read_exact(&mut payload).await?;
            Ok(String::from_utf8_lossy(&payload).into_owned())
        };
        tokio::time::timeout(TIMEOUT, exchange).await.map_err(|_| unavailable("sway не ответил"))?.map_err(unavailable)
    }

    /// The application windows now open.
    pub async fn windows(&self) -> Result<Vec<SwayWindow>, CaptureError> {
        Ok(parse_tree(&self.ipc(GET_TREE).await?))
    }

    async fn window(&self, uuid: &str) -> Result<SwayWindow, CaptureError> {
        let id = uuid.strip_prefix(KEY_PREFIX).unwrap_or(uuid);
        self.windows().await?.into_iter().find(|w| w.id.to_string() == id).ok_or(CaptureError::WindowGone)
    }

    /// Whether the window `uuid` still exists.
    pub async fn window_exists(&self, uuid: &str) -> bool { self.window(uuid).await.is_ok() }

    /// The user picks the game by clicking it: sway has no interactive window picker, so the first window that is not one of
    /// LipaX's own (`own_pid`) and takes the focus within [`PICK_TIMEOUT`] is the one. `Ok(None)`: nothing was clicked.
    pub async fn pick_window(&self, own_pid: u32) -> Result<Option<WindowKey>, CaptureError> {
        let deadline = tokio::time::Instant::now() + PICK_TIMEOUT;
        while tokio::time::Instant::now() < deadline {
            if let Some(window) = self.windows().await?.into_iter().find(|w| w.focused && w.pid != own_pid) { return Ok(Some(window.key())); }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        Ok(None)
    }

    /// The pixels of the client rectangle of `window` as `grim` sees them (output scale included).
    async fn grab_window(&self, window: &SwayWindow) -> Result<DynamicImage, CaptureError> {
        let g = window.client;
        // grim takes whole logical px; round outwards so that nothing of the client is cut.
        let (x, y) = (g.x.floor() as i64, g.y.floor() as i64);
        let (w, h) = ((g.x + g.w).ceil() as i64 - x, (g.y + g.h).ceil() as i64 - y);
        let output = tokio::time::timeout(TIMEOUT * 2, tokio::process::Command::new("grim")
            .args(["-l", "0", "-g", &format!("{x},{y} {w}x{h}"), "-"]).kill_on_drop(true).output())
            .await.map_err(|_| unavailable("grim не ответил"))?
            .map_err(|e| unavailable(format!("grim не запущен: {e} (нужен пакет grim)")))?;
        if !output.status.success() {
            return Err(unavailable(format!("grim: {}", String::from_utf8_lossy(&output.stderr).trim())));
        }
        image::load_from_memory_with_format(&output.stdout, image::ImageFormat::Png).map_err(unavailable)
    }
}

impl DesktopBackend for SwayCapture {
    fn name(&self) -> &'static str { "Sway" }

    async fn window_state(&self, uuid: &str, want_active: bool) -> WindowState {
        let Ok(window) = self.window(uuid).await else { return WindowState::default() };
        WindowState { geometry: Some(window.client), active: want_active.then_some(window.focused), minimized: !window.visible }
    }
}

impl Capture for SwayCapture {
    fn capabilities(&self, _: &WindowKey, _: &Settings) -> CaptureCapabilities { CaptureCapabilities::WLROOTS }

    async fn grab(&self, window: &WindowKey, rect: NormRect) -> Result<DynamicImage, CaptureError> {
        let full = self.grab_window(&self.window(&window.uuid).await?).await?;
        let (x, y, w, h) = rect_px(rect, full.width(), full.height());
        if (x, y, w, h) == (0, 0, full.width(), full.height()) { return Ok(full); }
        Ok(full.crop_imm(x, y, w, h))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shape of a real `GET_TREE` reply (trimmed): an output, a workspace, a tiled Wayland window with a border, a floating
    /// XWayland window and a container without a process.
    const TREE: &str = r#"{"id":1,"type":"root","nodes":[{"id":3,"type":"output","name":"HEADLESS-1","nodes":[
        {"id":4,"type":"workspace","name":"1","nodes":[
          {"id":10,"type":"con","name":"konsole","app_id":"org.kde.konsole","pid":4242,"focused":true,"visible":true,
           "rect":{"x":0,"y":0,"width":960,"height":1080},"window_rect":{"x":2,"y":24,"width":956,"height":1054},"nodes":[],"floating_nodes":[]},
          {"id":11,"type":"con","name":"split","nodes":[],"floating_nodes":[],"rect":{"x":960,"y":0,"width":960,"height":1080},"window_rect":{"x":0,"y":0,"width":0,"height":0}}
        ],"floating_nodes":[
          {"id":12,"type":"floating_con","name":"Game","app_id":null,"pid":77,"focused":false,"visible":false,
           "window_properties":{"class":"steam_app_123"},
           "rect":{"x":100,"y":50,"width":640,"height":480},"window_rect":{"x":0,"y":0,"width":640,"height":480},"nodes":[],"floating_nodes":[]}
        ]}],"floating_nodes":[]}],"floating_nodes":[]}"#;

    #[test]
    fn windows_are_the_leaves_with_a_process_and_the_client_rect_is_inside_the_container() {
        let windows = parse_tree(TREE);
        assert_eq!(windows.iter().map(|w| w.id).collect::<Vec<_>>(), [10, 12], "the output, the workspace and the empty container are not windows");
        let konsole = &windows[0];
        assert_eq!((konsole.class.as_str(), konsole.title.as_str(), konsole.pid), ("org.kde.konsole", "konsole", 4242));
        assert_eq!((konsole.client.x, konsole.client.y, konsole.client.w, konsole.client.h), (2.0, 24.0, 956.0, 1054.0), "border and title bar are left out");
        assert!(konsole.focused && konsole.visible);
        let game = &windows[1];
        assert_eq!(game.class, "steam_app_123", "an XWayland window has its class instead of an app id");
        assert!(!game.visible && !game.focused);
        assert_eq!(game.key().uuid, "sway:12");
        assert!(is_sway_window(&game.key()) && !is_sway_window(&WindowKey { uuid: "{abc}".into(), ..game.key() }));
    }

    #[test]
    fn a_reply_that_is_not_a_tree_has_no_windows() {
        assert!(parse_tree("").is_empty());
        assert!(parse_tree("[]").is_empty());
        assert!(parse_tree(r#"{"id":1,"nodes":[]}"#).is_empty());
    }
}
