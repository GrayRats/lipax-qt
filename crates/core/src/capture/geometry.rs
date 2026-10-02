//! KWin's public window-info API exposes only the decorated frame. Its scripting API
//! exposes client, frame and buffer geometry in logical desktop coordinates.
use super::kwin::{WindowFrames, WindowGeometry};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use tokio::sync::Notify;

static SCRIPTS: OnceLock<Mutex<Vec<(zbus::Connection, String)>>> = OnceLock::new();

/// Explicitly unload scripts before the application runtime is shut down.
pub async fn shutdown() {
    let scripts = std::mem::take(&mut *SCRIPTS.get_or_init(Default::default).lock().unwrap());
    for (conn, plugin) in scripts {
        let _ = conn
            .call_method(
                Some("org.kde.KWin"),
                "/Scripting",
                Some("org.kde.kwin.Scripting"),
                "unloadScript",
                &(plugin,),
            )
            .await;
    }
}

#[derive(Default)]
struct State {
    windows: Mutex<HashMap<String, WindowFrames>>,
    ready: Notify,
}

struct GeometryService(Arc<State>);

#[zbus::interface(name = "io.lipa.Geometry", spawn = false)]
impl GeometryService {
    /// `[client, frame, buffer]`, each `[x, y, w, h]`; anything else (`null` on close) forgets the window.
    fn update(&self, uuid: &str, geometry: &str) {
        let rect = |[x, y, w, h]: [f64; 4]| {
            ([x, y, w, h].iter().all(|v| v.is_finite()) && w > 0.0 && h > 0.0).then_some(WindowGeometry { x, y, w, h })
        };
        let frames = serde_json::from_str::<[[f64; 4]; 3]>(geometry).ok().and_then(|[c, f, b]| {
            let client = rect(c)?;
            Some(WindowFrames { client, frame: rect(f).unwrap_or(client), buffer: rect(b).unwrap_or(client) })
        });
        let mut windows = self.0.windows.lock().unwrap();
        match frames {
            Some(frames) => windows.insert(uuid.into(), frames),
            None => windows.remove(uuid),
        };
    }

    fn ready(&self) {
        self.0.ready.notify_one();
    }
}

pub(super) struct ClientGeometry {
    state: Arc<State>,
    conn: zbus::Connection,
    plugin: String,
}

impl ClientGeometry {
    pub async fn connect() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let state = Arc::new(State::default());
        let conn = zbus::connection::Builder::session()?
            .serve_at("/io/lipa/Geometry", GeometryService(state.clone()))?
            .build()
            .await?;
        let destination = conn
            .unique_name()
            .ok_or("D-Bus connection has no name")?
            .as_str();
        let plugin = format!("lipa-geometry-{}", destination.replace([':', '.'], "_"));
        let script = include_str!("geometry.js").replace("__DESTINATION__", destination);
        let path = dirs::runtime_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join(format!("{plugin}.js"));
        // Unique bus name, exclusive creation and owner-only access.
        {
            use std::io::Write;
            use std::os::unix::fs::OpenOptionsExt;
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&path)?;
            file.write_all(script.as_bytes())?;
        }
        SCRIPTS
            .get_or_init(Default::default)
            .lock()
            .unwrap()
            .push((conn.clone(), plugin.clone()));
        let loaded = conn
            .call_method(
                Some("org.kde.KWin"),
                "/Scripting",
                Some("org.kde.kwin.Scripting"),
                "loadScript",
                &(path.to_string_lossy().as_ref(), plugin.as_str()),
            )
            .await;
        let result = async {
            let id: i32 = loaded?.body().deserialize()?;
            if id < 0 {
                return Err("KWin could not load geometry script".into());
            }
            let tracker = Self {
                state: state.clone(),
                conn: conn.clone(),
                plugin,
            };
            let script_path = format!("/Scripting/Script{id}");
            conn.call_method(
                Some("org.kde.KWin"),
                script_path.as_str(),
                Some("org.kde.kwin.Script"),
                "run",
                &(),
            )
            .await?;
            tokio::time::timeout(std::time::Duration::from_secs(3), state.ready.notified()).await?;
            Ok::<_, Box<dyn std::error::Error + Send + Sync>>(tracker)
        }
        .await;
        let _ = std::fs::remove_file(path);
        result
    }

    pub fn get(&self, uuid: &str) -> Option<WindowFrames> {
        self.state.windows.lock().unwrap().get(uuid).copied()
    }
}

impl Drop for ClientGeometry {
    fn drop(&mut self) {
        let conn = self.conn.clone();
        let plugin = self.plugin.clone();
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn(async move {
                let _ = conn
                    .call_method(
                        Some("org.kde.KWin"),
                        "/Scripting",
                        Some("org.kde.kwin.Scripting"),
                        "unloadScript",
                        &(plugin.clone(),),
                    )
                    .await;
                SCRIPTS
                    .get_or_init(Default::default)
                    .lock()
                    .unwrap()
                    .retain(|(_, name)| name != &plugin);
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::NormRect;

    #[test]
    fn client_geometry_excludes_titlebar_and_survives_move_and_close() {
        let state = Arc::new(State::default());
        let service = GeometryService(state.clone());
        // Outer window is (100, 50, 808, 634); client origin includes 4px side + 30px title.
        service.update("game", "[[104,80,800,600],[100,50,808,634],[104,80,800,600]]");
        let frames = state.windows.lock().unwrap()["game"];
        assert_eq!(frames.frame, WindowGeometry { x: 100.0, y: 50.0, w: 808.0, h: 634.0 });
        let g = frames.client;
        assert_eq!(
            g.region(NormRect {
                x: 0.0,
                y: 0.0,
                w: 1.0,
                h: 1.0
            }),
            g
        );
        assert_eq!(
            g.region(NormRect {
                x: 0.25,
                y: 0.5,
                w: 0.5,
                h: 0.25
            }),
            WindowGeometry {
                x: 304.0,
                y: 380.0,
                w: 400.0,
                h: 150.0
            }
        );
        service.update("game", "[[-1200,100,800,600],[0,0,0,0],[-1200,100,800,600]]");
        let moved = state.windows.lock().unwrap()["game"];
        assert_eq!(moved.client.x, -1200.0);
        assert_eq!(moved.frame, moved.client, "invalid frame falls back to the client area");
        service.update("game", "null");
        assert!(!state.windows.lock().unwrap().contains_key("game"));
    }
}
