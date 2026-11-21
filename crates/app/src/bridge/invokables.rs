//! The invokable methods of the controller that change state: settings, regions, window, pipeline commands.

use lipa_core::sync::LockExt;
use super::*;

impl qobject::Controller {
    pub(super) fn reduced_motion(&self) -> bool {
        let kdeglobals = dirs::config_dir().and_then(|d| std::fs::read_to_string(d.join("kdeglobals")).ok()).unwrap_or_default();
        reduced_motion_from(std::env::var("LIPAX_REDUCE_MOTION").ok().as_deref(), &kdeglobals)
    }
    pub(super) fn arm_floating_focus_restore(&self) {
        lipa_core::capture::arm_floating_focus_restore();
        tracing::debug!(target: "translation_window.focus", "armed one-time focus restoration for automatic floating show");
    }
    pub(super) fn default_settings_json(&self) -> QString {
        QString::from(serde_json::to_string(&Settings::default()).unwrap().as_str())
    }
    pub(super) fn editable_settings_paths(&self) -> QString {
        let paths: Vec<_> = lipa_core::settings::REACTIONS.iter()
            .filter(|(_, reaction)| *reaction != lipa_core::settings::Reaction::Managed).map(|(path, _)| path).collect();
        QString::from(serde_json::to_string(&paths).unwrap().as_str())
    }
    pub(super) fn copy_text(&self, text: &QString) { crate::icon::copy_text(text); }
    pub(super) fn configure_translation_window_blur(&self, enable: bool, radius: i32) { crate::icon::translation_window_blur(enable, radius); }
    /// `rects` — JSON `[[x, y, w, h], ...]`: где overlay принимает ввод, когда клики идут сквозь него.
    pub(super) fn configure_translation_window(&self, passthrough: bool, rects: &QString) {
        let rects: Vec<[i32; 4]> = serde_json::from_str(&rects.to_string()).unwrap_or_default();
        crate::icon::translation_window_input(passthrough, &rects.concat());
    }
    pub(super) fn set_translation_window_pinned(mut self: Pin<&mut Self>, pinned: bool, source: &QString) {
        // Cancel an automatic show that was superseded before KWin saw the floating surface.
        if pinned { lipa_core::capture::clear_floating_focus_restore(); }
        self.rust().shared.update(|s| {
            s.translation_window.mode = lipa_core::settings::TranslationWindowMode::from_pinned(pinned);
        });
        tracing::debug!(target: "translation_window.state", pinned, floating = !pinned,
            window_role = if pinned { "layer_shell" } else { "xdg_toplevel" }, source = %source, "translation window pin state");
        self.as_mut().publish_settings();
    }
    pub(super) fn report_floating_geometry(mut self: Pin<&mut Self>, json: &QString, reason: &QString) {
        match serde_json::from_str::<lipa_core::settings::FloatingGeometry>(&json.to_string()) {
            Ok(g) if g.is_valid() => {
                store_floating_geometry(&self.rust().shared, g, &reason.to_string());
                self.as_mut().publish_settings();
            }
            _ => tracing::warn!(target: "translation_window.geometry", json = %json, "invalid floating geometry from QML"),
        }
    }
    pub(super) fn report_main_window_geometry(mut self: Pin<&mut Self>, json: &QString) {
        if !self.rust().shared.settings.borrow().general.remember_window_geometry { return; }
        match serde_json::from_str::<lipa_core::settings::WindowRecord>(&json.to_string()) {
            Ok(g) if g.is_valid() => {
                if self.rust().shared.settings.borrow().general.main_window.as_ref() == Some(&g) { return; }
                tracing::debug!(target: "main_window.geometry", x = g.x, y = g.y, width = g.w, height = g.h, screen = %g.screen, "main window geometry");
                self.rust().shared.update(|s| s.general.main_window = Some(g));
                self.as_mut().publish_settings();
            }
            _ => tracing::warn!(target: "main_window.geometry", json = %json, "invalid main window geometry from QML"),
        }
    }
    pub(super) fn report_portal_monitor_geometry(mut self: Pin<&mut Self>, json: &QString) {
        let settings = self.rust().shared.settings.borrow().clone();
        let enabled = settings.capture.portal_fills_monitor
            && !settings.translation_window.screen.is_empty()
            && settings.capture.window.as_ref().is_some_and(is_portal_window);
        let geometry = serde_json::from_str::<[f64; 4]>(&json.to_string()).ok()
            .filter(|g| g.iter().all(|v| v.is_finite()) && g[2] > 0.0 && g[3] > 0.0);
        let value = if enabled { geometry.map(|g| serde_json::to_string(&g).unwrap()) } else { None };
        let value = value.unwrap_or_default();
        if self.game_geometry().to_string() != value {
            self.as_mut().set_game_geometry(QString::from(value.as_str()));
            self.as_mut().publish_inplace();
        }
    }
    pub(super) fn report_screens(mut self: Pin<&mut Self>, json: &QString) {
        let Ok(list) = serde_json::from_str::<Vec<[f64; 4]>>(&json.to_string()) else {
            tracing::warn!(target: "inplace.geometry", json = %json, "invalid screen geometry from QML");
            return;
        };
        let screens: Vec<_> = list.into_iter().filter(|g| g.iter().all(|v| v.is_finite()) && g[2] > 0.0 && g[3] > 0.0)
            .map(|g| lipa_core::layout::DesktopRect::in_space(g[0] as f32, g[1] as f32, g[2] as f32, g[3] as f32)).collect();
        if screens != self.rust().screens {
            tracing::debug!(target: "inplace.geometry", screens = ?screens, "screens changed");
            self.as_mut().rust_mut().screens = screens;
            self.as_mut().publish_inplace();
        }
    }
    pub(super) fn set_inplace_visibility(mut self: Pin<&mut Self>, visible: bool, source: &QString) {
        if *self.inplace_visible() != visible {
            tracing::debug!(target: "inplace.state", visible, source = %source, "inplace visibility");
            self.as_mut().set_inplace_visible(visible);
        }
    }
    pub(super) fn set_translation_window_visibility(mut self: Pin<&mut Self>, visible: bool, source: &QString) {
        if *self.translation_window_visible() != visible {
            tracing::debug!(target: "translation_window.state", visible, source = %source, "translation window visibility");
            self.as_mut().set_translation_window_visible(visible);
        }
    }
    /// Активный рендер по настройкам и бэкенду захвата; меняется — пишется в журнал.
    pub(super) fn publish_display(mut self: Pin<&mut Self>) {
        let (mode, note) = effective_display(&self.rust().shared.settings.borrow());
        if self.effective_display().to_string() != mode || self.display_note().to_string() != note {
            if note.is_empty() {
                tracing::info!(target: "inplace.state", mode, "translation display");
            } else {
                tracing::warn!(target: "inplace.state", mode, reason = %note, "translation display fallback");
            }
            self.as_mut().set_effective_display(QString::from(mode));
            self.as_mut().set_display_note(QString::from(note.as_str()));
        }
    }
    /// Уровень журнала и запись автозапуска меняются сразу, как только они изменились в настройках.
    pub(super) fn apply_general(mut self: Pin<&mut Self>) {
        let now = self.rust().shared.settings.borrow().general.clone();
        let before = self.rust().applied_general.clone();
        if now == before { return; }
        self.as_mut().rust_mut().applied_general = now.clone();
        let mut failures: Vec<String> = Vec::new();
        if now.log_level != before.log_level
            && let Err(e) = crate::logging::set_level(&now.log_level) { failures.push(e); }
        if now.autostart != before.autostart {
            let result = lipa_core::autostart::default_dir().ok_or_else(|| "не найден каталог ~/.config".to_owned()).and_then(|dir| {
                let exe = std::env::current_exe().map_err(|e| e.to_string())?;
                lipa_core::autostart::set(&dir, now.autostart, &exe.to_string_lossy()).map_err(|e| e.to_string())
            });
            if let Err(e) = result { failures.push(format!("автозапуск: {e}")); }
        }
        if !failures.is_empty() {
            tracing::error!(component = "settings", error = %failures.join("; "), "Общие настройки применены не полностью");
            self.as_mut().set_status(QString::from(format!("Не удалось применить: {}", failures.join("; ")).as_str()));
            self.as_mut().set_status_kind(QString::from("error"));
        }
    }

    pub(super) fn publish_settings(mut self: Pin<&mut Self>) {
        self.as_mut().check_models();
        self.as_mut().apply_general();
        self.as_mut().publish_display();
        let s = self.rust().shared.settings.borrow().clone();
        let capabilities = capabilities_json(&s);
        if self.capture_capabilities().to_string() != capabilities {
            tracing::debug!(target: "capture.capabilities", capabilities = %capabilities, "capture feature availability updated");
            self.as_mut().set_capture_capabilities(QString::from(capabilities.as_str()));
        }
        self.as_mut().set_settings_state(QString::from(serde_json::to_string(&s).unwrap().as_str()));
        self.as_mut().set_has_region(!s.capture_regions().is_empty());
        let active: Vec<_> = s.capture_regions().into_iter().map(|r| r.id).collect();
        self.as_mut().rust_mut().faults.retain(|id, _| active.contains(id));
        self.as_mut().publish_faults();
    }
    pub(super) fn publish_translation(mut self: Pin<&mut Self>) {
        let regions = self.rust().shared.settings.borrow().capture_regions();
        self.as_mut().rust_mut().region_text.retain(|id, _| regions.iter().any(|r| &r.id == id));
        let texts = &self.rust().region_text;
        let original = regions.iter().filter_map(|r| texts.get(&r.id).map(|t| format!("{}: {}", r.name, t.0))).collect::<Vec<_>>().join("\n");
        let translation = regions.iter().filter_map(|r| texts.get(&r.id).map(|t| if regions.len() > 1 { format!("{}: {}", r.name, t.1) } else { t.1.clone() })).collect::<Vec<_>>().join("\n\n");
        self.as_mut().set_original(QString::from(original.as_str()));
        self.as_mut().set_translation(QString::from(translation.as_str()));
        self.as_mut().publish_inplace();
    }
    /// Поля «поверх оригинала» для QML: всё уже решено (шрифт, кегль после подгонки метриками
    /// Qt, типографика, фон); QML только рисует. Неизменное состояние не переустанавливается.
    pub(super) fn publish_inplace(mut self: Pin<&mut Self>) {
        // Только нужное из настроек: полный клон `Settings` на каждую публикацию не нужен.
        let (regions, inplace) = {
            let settings = self.rust().shared.settings.borrow();
            (settings.capture_regions(), settings.appearance.inplace.clone())
        };
        self.as_mut().rust_mut().region_inplace.retain(|id, _| regions.iter().any(|r| &r.id == id));
        let window = serde_json::from_str::<[f64; 4]>(&self.game_geometry().to_string()).ok();
        // Арк-указатели: клонируются только счётчики, не блоки и не картинки.
        let frames: Vec<(String, lipa_core::settings::NormRect, Arc<InplaceFrame>)> = match window {
            // A frame captured higher than the drawn region (text its top edge cut) is placed where it was captured.
            Some(_) => regions.iter().filter_map(|r| {
                let frame = self.rust().region_inplace.get(&r.id)?.clone();
                Some((r.id.clone(), frame.capture.or(r.rect)?, frame))
            }).collect(),
            None => Vec::new(),
        };
        // Подложки пишутся на диск только при новой ревизии поля.
        let mut images = std::collections::HashMap::new();
        for (region, _, frame) in &frames {
            for b in &frame.blocks {
                let Some(img) = b.background.image.as_ref() else { continue };
                let key = (region.clone(), b.id);
                // Written when the frame arrived (`write_backdrops`); this only looks the file up.
                let Some(url) = ensure_backdrop(&self.rust().shared, region, b.id, b.revision, img) else { continue };
                images.insert(key, url);
            }
        }
        // Подложки исчезнувших полей больше не нужны.
        let gone: Vec<_> = {
            let mut written = self.rust().shared.backdrops.lock_recover();
            let gone: Vec<_> = written.keys().filter(|k| !images.contains_key(*k)).cloned().collect();
            for key in &gone { written.remove(key); }
            gone
        };
        for key in gone { let _ = std::fs::remove_file(backdrop_path(&key.0, key.1)); }
        let placed = match window {
            Some(window) => {
                let inputs: Vec<RegionInput> = frames.iter().map(|(id, rect, frame)| RegionInput { id, rect: *rect, frame }).collect();
                let mut cache = std::mem::take(&mut self.as_mut().rust_mut().placement_cache);
                let view = Viewport::new(lipa_core::capture::kwin::WindowGeometry::from(window), self.rust().screens.clone());
                let placed = place_regions(&inputs, &view, &inplace, &images, &crate::icon::QtMeasure, &mut cache);
                self.as_mut().rust_mut().placement_cache = cache;
                placed
            }
            None => Default::default(),
        };
        tracing::debug!(target: "inplace.render", geometry = window.is_some(), regions = frames.len(), placed = placed.placed.len(),
            dropped = placed.dropped.len(), degraded = placed.placed.iter().filter(|p| p.degraded.is_some()).count(), "in-place fields published");
        let fallback = fallback_json(&placed, &regions);
        for field in &placed.dropped {
            tracing::warn!(target: "inplace.render", region = %field.region_id, block_id = field.block_id, reason = field.reason, stage = "placement", "inplace field cannot be rendered");
        }
        for field in &placed.placed {
            tracing::debug!(target: "inplace.render", region = %field.region_id, block_id = field.block_id, outcome = ?field.outcome, coordinate_space = "desktop_logical", "inplace placement accepted; awaiting GUI rendering");
        }
        if window.is_none() && !self.rust().region_inplace.is_empty() {
            tracing::warn!(target: "inplace.render", reason = "client_geometry_unavailable", "inplace frames received without a usable client viewport");
        }
        if self.inplace_fallback_json().to_string() != fallback {
            self.as_mut().set_inplace_fallback_json(QString::from(fallback.as_str()));
        }
        let placed = placed.placed;
        let json = serde_json::to_string(&placed).unwrap_or_else(|e| {
            tracing::error!(target: "inplace.render", error = %e, "cannot serialize the placed fields");
            "[]".into()
        });
        if *self.inplace_json() != QString::from(json.as_str()) {
            self.as_mut().set_inplace_json(QString::from(json.as_str()));
        }
    }
    pub(super) fn publish_faults(mut self: Pin<&mut Self>) {
        if self.rust().faults.is_empty() { return; }
        let text = self.rust().faults.values().map(|(s, _)| s.as_str()).collect::<Vec<_>>().join(" · ");
        let kind = if self.rust().faults.values().any(|(_, terminal)| *terminal) { "error" } else { "warning" };
        self.as_mut().set_status(QString::from(text.as_str()));
        self.as_mut().set_status_kind(QString::from(kind));
    }
    pub(super) fn publish_history(mut self: Pin<&mut Self>) {
        let s = self.rust().shared.settings.borrow().clone();
        self.as_mut().rust_mut().history.trim(s.history_limit);
        let result = self.rust().history.save(&History::path(), s.history_persist);
        let json = serde_json::to_string(&self.rust().history.entries).unwrap();
        self.as_mut().set_history_json(QString::from(json.as_str()));
        if let Err(e) = result {
            tracing::error!(component = "history", error = %e, "История не сохранена");
            self.as_mut().set_status(QString::from(format!("История не сохранена: {e}").as_str()));
        }
    }
    pub(super) fn clear_history(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().history.entries.clear();
        self.as_mut().publish_history();
    }
    pub(super) fn refresh_diagnostics(mut self: Pin<&mut Self>) {
        if *self.diagnostics_busy() { return; }
        self.as_mut().set_diagnostics_busy(true);
        let s = self.rust().shared.settings.borrow().clone();
        let qt = self.qt_thread();
        let qt_detail = self.qt_version().to_string();
        spawn_service(async move {
            let mut result = lipa_core::diagnostics::inspect(&s).await;
            result.insert(0, lipa_core::diagnostics::Check { name: "Qt / Qt Quick Controls".into(), state: "ready".into(),
                detail: qt_detail, instruction: "Используются системные библиотеки Qt 6.12 или новее.".into() });
            let json = serde_json::to_string(&result).unwrap();
            let _ = qt.queue(move |mut o| {
                o.as_mut().set_diagnostics_json(QString::from(json.as_str()));
                o.as_mut().set_diagnostics_busy(false);
            });
        });
    }

    pub(super) fn settings_json(&self) -> QString {
        let s = self.rust().shared.settings.borrow().clone();
        QString::from(serde_json::to_string(&s).unwrap_or_default().as_str())
    }

    pub(super) fn apply_settings(mut self: Pin<&mut Self>, json: &QString) {
        match serde_json::from_str::<Settings>(&json.to_string()) {
            Ok(new) => {
                let before = self.rust().shared.settings.borrow().processing_key();
                // Окно и область меняются отдельными действиями — не затираем их из формы.
                self.rust().shared.update(|s| {
                    let (w, r, t, p) = (s.capture.window.take(), s.capture.region.take(), std::mem::take(&mut s.capture.portal_token), std::mem::take(&mut s.game_profiles));
                    *s = { let mut value = new; value.capture.window = w; value.capture.region = r; value.capture.portal_token = t; value.game_profiles = p; value };
                });
                if self.rust().shared.settings.borrow().processing_key() != before {
                    // Поля, найденные в прежней области, не рисуются поверх новой.
                    self.as_mut().rust_mut().region_inplace.clear();
                }
                self.as_mut().publish_settings();
                self.as_mut().publish_translation();
                self.as_mut().publish_history();
            }
            Err(e) => {
                tracing::error!(component = "settings", error = %e, "Настройки из QML не приняты");
                self.as_mut().set_status(QString::from(format!("Настройки не сохранены: {e}").as_str()));
                self.as_mut().set_status_kind(QString::from("error"));
            },
        }
    }

    pub(super) fn apply_settings_patch(mut self: Pin<&mut Self>, json: &QString) {
        let patch = match serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(&json.to_string()) {
            Ok(patch) => patch,
            Err(e) => {
                tracing::error!(component = "settings", error = %e, "Некорректное изменение настроек из QML");
                self.as_mut().set_status(QString::from(format!("Настройки не сохранены: {e}").as_str()));
                self.as_mut().set_status_kind(QString::from("error"));
                return;
            }
        };
        if patch.is_empty() { return; }
        let before = self.rust().shared.settings.borrow().processing_key();
        let previous = self.rust().shared.settings.borrow().clone();
        match self.rust().shared.update_checked(|s| {
            *s = merge_settings_patch(s, &patch)?;
            Ok(())
        }) {
            Ok(()) => {
                if self.rust().shared.settings.borrow().processing_key() != before {
                    self.as_mut().rust_mut().region_inplace.clear();
                }
                self.as_mut().publish_settings();
                self.as_mut().publish_translation();
                // The history file is rewritten whole: only a change of its own settings (limit, saving) does that, not
                // every step of a slider.
                if patch.keys().any(|k| k.starts_with("history_")) { self.as_mut().publish_history(); }
                // A setting that waits for something (a new window selection) says so instead of looking ignored.
                let pending = previous.pending_actions(&self.rust().shared.settings.borrow());
                if let Some(note) = pending.first() {
                    tracing::info!(target: "settings", note = *note, "setting waits for an action");
                    self.as_mut().set_status(QString::from(*note));
                }
            }
            Err(e) => {
                tracing::error!(component = "settings", error = %e, "Изменение настроек не принято");
                self.as_mut().set_status(QString::from(format!("Настройки не сохранены: {e}").as_str()));
                self.as_mut().set_status_kind(QString::from("error"));
            }
        }
    }

    pub(super) fn pick_window(self: Pin<&mut Self>) {
        let shared = self.rust().shared.clone();
        let qt = self.qt_thread();
        let _ = qt.queue(|mut o| {
            o.as_mut()
                .set_status(QString::from("Кликните по окну игры (Esc — отмена)"))
        });
        spawn_service(async move {
            let backend = shared.settings.borrow().capture.source;
            let by_kwin = async {
                match shared.kwin().await {
                    // Capturing needs KWin's permission, picking does not: ask before the user is made to
                    // click, and say why if it is refused (the probe takes no picture).
                    Ok(k) => match k.check_capture_permission().await {
                        Ok(()) => k.pick_window().await.map_err(|e| e.to_string()),
                        Err(e) => Err(e.to_string()),
                    },
                    Err(e) => Err(e),
                }
            };
            let by_portal = async { shared.capture.portal.select_window().await.map(Some).map_err(|e| e.to_string()) };
            let res = match backend {
                CaptureSource::Kwin => by_kwin.await,
                CaptureSource::Portal => by_portal.await,
                // KWin недоступен (другой композитор, нет прав на ScreenShot2) — запасной путь через portal.
                CaptureSource::Auto => match by_kwin.await {
                    Ok(r) => Ok(r),
                    Err(e) => {
                        tracing::warn!(component = "KWin", error = %e, "Переключение на Portal-захват");
                        let note = if e.contains("NoAuthorized") {
                            "KWin не разрешил захват этому процессу (локальная сборка? см. packaging/run-local.sh): выберите окно в системном диалоге (portal)"
                        } else {
                            "KWin недоступен, выберите окно в системном диалоге (portal)"
                        };
                        let _ = qt.queue(move |mut o| o.as_mut().set_status(QString::from(note)));
                        by_portal.await
                    }
                },
            };
            match res {
                Ok(Some(key)) => {
                    let title = key.caption.clone();
                    // Знакомая игра получает назад свои области и настройки; для новой смена окна
                    // делает прежнюю область недействительной.
                    let mut restored = false;
                    shared.update(|s| {
                        s.capture.window = Some(key);
                        s.capture.region = None;
                        restored = s.apply_game_profile();
                        if !restored { for r in &mut s.capture.regions { r.rect = None; } }
                    });
                    let has_region = !shared.settings.borrow().capture_regions().is_empty();
                    show_frame(shared.clone(), qt.clone());
                    let _ = qt.queue(move |mut o| {
                        o.as_mut().set_window_title(QString::from(title.as_str()));
                        o.as_mut().rust_mut().region_text.clear();
                        o.as_mut().rust_mut().region_inplace.clear();
                        o.as_mut().rust_mut().faults.clear();
                        o.as_mut().publish_settings();
                        o.as_mut().publish_translation();
                        o.as_mut().set_status(QString::from(match (restored, has_region) {
                            (true, true) => "Окно выбрано. Области захвата и настройки игры восстановлены",
                            (true, false) => "Окно выбрано. Профиль игры восстановлен, область захвата не задана",
                            _ => "Окно выбрано. Теперь выберите область захвата",
                        }));
                    });
                }
                Ok(None) => {
                    let _ = qt
                        .queue(|mut o| o.as_mut().set_status(QString::from("Выбор окна для захвата отменён")));
                }
                Err(e) => {
                    tracing::error!(component = "capture", error = %e, "Не удалось выбрать окно");
                    let _ = qt.queue(move |mut o| {
                        o.as_mut()
                            .set_status(QString::from(format!("Ошибка: {e}").as_str()))
                    });
                }
            }
        });
    }

    pub(super) fn forget_game_profile(mut self: Pin<&mut Self>, key: &QString) {
        let key = key.to_string();
        tracing::info!(target: "settings", game = %key, "профиль игры удалён");
        // The selected game is saved again by the same update; the settings form disables its button.
        self.rust().shared.update(|s| { s.game_profiles.remove(&key); });
        self.as_mut().publish_settings();
    }

    /// Снимок окна для выбора области: показывается в нашем окне, поэтому
    /// не требует рисовать поверх чужого окна (чего Wayland не позволяет).
    pub(super) fn request_preview(self: Pin<&mut Self>) {
        let shared = self.rust().shared.clone();
        let qt = self.qt_thread();
        spawn_service(async move {
            let Some(window) = shared.settings.borrow().capture.window.clone() else {
                let _ = qt.queue(|mut o| {
                    o.as_mut()
                        .set_status(QString::from("Сначала выберите окно"))
                });
                return;
            };
            let res = async {
                let img = shared.capture.grab_full(&window).await.map_err(|e| e.to_string())?;
                // A header bar drawn by the application itself is part of the frame: only a hint can be given.
                let title_bar = lipa_core::capture::title_bar::detect(&img).unwrap_or(0) as i32;
                let dir = lipa_core::runtime_dir()
                    .join("lipa");
                std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
                let path = dir.join(format!("preview-{}.png", std::process::id()));
                img.save(&path).map_err(|e| e.to_string())?;
                Ok::<_, String>((path, title_bar))
            }
            .await;
            if let Err(e) = &res {
                tracing::error!(component = "capture", error = %e, "Не удалось получить кадр для выбора области");
            }
            let _ = qt.queue(move |mut o| match res {
                Ok((path, title_bar)) => {
                    o.as_mut().set_preview_title_bar(title_bar);
                    // Меняющийся параметр отключает кэш изображения в QML.
                    let t = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_millis())
                        .unwrap_or(0);
                    let url = format!("file://{}?{t}", path.display());
                    o.as_mut().set_preview_source(QString::from(url.as_str()));
                    o.as_mut().preview_ready();
                }
                Err(e) => {
                    o.as_mut().set_status(QString::from(format!("Ошибка: {e}").as_str()));
                },
            });
        });
    }

    pub(super) fn set_region(mut self: Pin<&mut Self>, x: f64, y: f64, w: f64, h: f64) {
        let rect = NormRect { x, y, w, h };
        self.rust().shared.update(|s| {
            s.capture.region = None;
            if let Some(r) = s.capture.regions.iter_mut().find(|r| r.id == s.capture.active_region) { r.rect = Some(rect); r.enabled = true; }
        });
        let active = self.rust().shared.settings.borrow().capture.active_region.clone();
        self.as_mut().rust_mut().region_inplace.remove(&active);
        self.as_mut().publish_translation();
        // The region outline (RegionFrame) flashes by itself when the area changes.
        self.as_mut().publish_settings();
        self.as_mut().set_has_region(true);
        self.as_mut().set_status(QString::from("Область захвата сохранена"));
    }

    pub(super) fn reset_region(mut self: Pin<&mut Self>) {
        self.rust().shared.update(|s| {
            s.capture.region = None;
            if let Some(r) = s.capture.regions.iter_mut().find(|r| r.id == s.capture.active_region) { r.rect = None; }
        });
        self.as_mut().publish_settings();
        self.as_mut().publish_translation();
        self.as_mut().set_status(QString::from("Область захвата сброшена"));
    }

    pub(super) fn start(mut self: Pin<&mut Self>) {
        tracing::info!("Слежение за областями запущено");
        let _ = self.rust().shared.running.send(true);
        self.as_mut().set_running(true);
        self.as_mut().set_status(QString::from("Ожидание текста"));
        self.as_mut().set_status_kind(QString::from("info"));
    }

    pub(super) fn stop(mut self: Pin<&mut Self>) {
        tracing::info!("Слежение за областями остановлено");
        let _ = self.rust().shared.running.send(false);
        self.as_mut().set_running(false);
        self.as_mut().set_status(QString::from("Остановлено"));
    }

    pub(super) fn start_auto_translate(mut self: Pin<&mut Self>) -> bool {
        if *self.running() { return true; }
        if !*self.has_region() {
            self.as_mut().set_status(QString::from("Сначала выберите область"));
            return false;
        }
        self.as_mut().start();
        true
    }

    pub(super) fn stop_auto_translate(mut self: Pin<&mut Self>) {
        if *self.running() { self.as_mut().stop(); }
    }

    pub(super) fn activate_window(&self, object_name: &QString, token: &QString) { crate::icon::activate_window(object_name, token); }

    pub(super) fn set_ocr_preview_enabled(mut self: Pin<&mut Self>, enabled: bool) {
        use std::sync::atomic::Ordering;
        if self.rust().shared.preview.swap(enabled, Ordering::Relaxed) == enabled { return; }
        tracing::debug!(target: "ocr.preview", enabled, "OCR preview window");
        if enabled {
            // A fresh frame at once instead of waiting for the next change on screen.
            if *self.has_region() { let _ = self.rust().shared.cmds.send(Cmd::TranslateOnce); }
        } else {
            self.rust().shared.preview_entries.lock_recover().clear();
            remove_preview_files();
            self.as_mut().set_ocr_preview_json(QString::from("[]"));
        }
    }

    pub(super) fn translate_once(mut self: Pin<&mut Self>) {
        tracing::info!("Запрошен ручной перевод / повтор");
        self.as_mut().rust_mut().faults.clear();
        self.as_mut().set_status_kind(QString::from("info"));
        self.as_mut().set_status(QString::from("Распознавание окна…"));
        let _ = self.rust().shared.cmds.send(Cmd::TranslateOnce);
    }

    pub(super) fn bundled_fonts(&self) -> QString {
        let db = lipa_core::layout::font_database::InstalledFontDatabase::bundled();
        let names: Vec<&str> = db.fonts().iter().map(|f| f.family.as_str()).collect();
        QString::from(serde_json::to_string(&names).unwrap_or_else(|_| "[]".into()).as_str())
    }

    pub(super) fn auto_tune_filters(mut self: Pin<&mut Self>) {
        tracing::info!(component = "ocr", "Запрошен автоподбор фильтров");
        self.as_mut().set_status(QString::from("Подбор фильтров…"));
        let _ = self.rust().shared.cmds.send(Cmd::AutoTune);
    }

    pub(super) fn reanalyze_fonts(mut self: Pin<&mut Self>) {
        tracing::info!(component = "fonts", "Запрошено повторное определение шрифтов");
        lipa_core::layout::font_database::InstalledFontDatabase::reload();
        let _ = self.rust().shared.cmds.send(Cmd::ReanalyzeFonts);
        self.as_mut().set_status(QString::from("Шрифты полей будут определены заново"));
    }
}
