//! Мост Rust ↔ Qt. Контроллер тонкий: состояние и логика живут в `lipa-core`,
//! тяжёлая работа выполняется в tokio-задачах, GUI-поток только получает события.

#[path = "model_controller.rs"]
mod model_controller;

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qproperty(QString, status)]
        #[qproperty(QString, status_kind, cxx_name = "statusKind")]
        #[qproperty(QString, settings_state, cxx_name = "settingsState")]
        #[qproperty(QString, model_state, cxx_name = "modelState")]
        #[qproperty(bool, model_busy, cxx_name = "modelBusy")]
        /// Bergamot: установленная и доступные версии модели выбранной пары (JSON `PairVersions`); см. `refreshModelVersions`.
        #[qproperty(QString, model_versions, cxx_name = "modelVersions")]
        #[qproperty(QString, history_json, cxx_name = "historyJson")]
        #[qproperty(QString, diagnostics_json, cxx_name = "diagnosticsJson")]
        #[qproperty(bool, diagnostics_busy, cxx_name = "diagnosticsBusy")]
        #[qproperty(QString, original)]
        #[qproperty(QString, translation)]
        /// Translations drawn over the original text, JSON array (one entry per active region).
        #[qproperty(QString, inplace_json, cxx_name = "inplaceJson")]
        /// Что не вышло показать поверх оригинала как задумано: JSON `{"texts": [{region, text}], "degraded": n}`.
        /// `texts` — переводы, которым не нашлось места поверх игры (их показывает окно перевода),
        /// `degraded` — сколько полей показано упрощённо (простая плашка или подпись рядом).
        #[qproperty(QString, inplace_fallback_json, cxx_name = "inplaceFallbackJson")]
        /// Что видел последний OCR каждой области (JSON-массив); заполняется, пока открыто окно просмотра.
        #[qproperty(QString, ocr_preview_json, cxx_name = "ocrPreviewJson")]
        #[qproperty(QString, window_title, cxx_name = "windowTitle")]
        #[qproperty(QString, preview_source, cxx_name = "previewSource")]
        /// Высота (px кадра) заголовка, который окно рисует само, на последнем снимке для выбора области; 0 — не найден.
        #[qproperty(i32, preview_title_bar, cxx_name = "previewTitleBar")]
        #[qproperty(QString, tesseract_json, cxx_name = "tesseractJson")]
        #[qproperty(bool, tesseract_busy, cxx_name = "tesseractBusy")]
        /// Состояние окружения PaddleOCR для выбранного Python и языка (JSON `PaddleEnv`); пусто — ещё не проверялось.
        #[qproperty(QString, paddle_json, cxx_name = "paddleJson")]
        #[qproperty(bool, paddle_busy, cxx_name = "paddleBusy")]
        /// RapidOCR for the recognition language: model, installed models, ONNX Runtime (JSON `RapidStatus`); empty — not checked yet.
        #[qproperty(QString, rapid_json, cxx_name = "rapidJson")]
        /// A RapidOCR model is being downloaded or deleted.
        #[qproperty(bool, rapid_busy, cxx_name = "rapidBusy")]
        #[qproperty(i32, rapid_progress, cxx_name = "rapidProgress")]
        /// Id of the model `rapidBusy` is about.
        #[qproperty(QString, rapid_model, cxx_name = "rapidModel")]
        #[qproperty(QString, rapid_error, cxx_name = "rapidError")]
        #[qproperty(QString, game_geometry, cxx_name = "gameGeometry")]
        #[qproperty(bool, running)]
        #[qproperty(bool, has_region, cxx_name = "hasRegion")]
        /// Видимость перевода поверх оригинала. Не связана с окном перевода.
        #[qproperty(bool, inplace_visible, cxx_name = "inplaceVisible")]
        /// Окно игры в таком состоянии, что перевод поверх оригинала можно показывать: настройка «только когда окно
        /// активно» выключена или окно видно и в фокусе (в фокусе может быть и окно самого LipaX).
        #[qproperty(bool, overlay_allowed, cxx_name = "overlayAllowed")]
        /// Итог последнего автоподбора фильтров (JSON): пресеты с оценками, лучший и применён ли он.
        #[qproperty(QString, autotune_json, cxx_name = "autotuneJson")]
        /// Видимость окна перевода («Поверх игры»). Не связана с переводом поверх оригинала.
        #[qproperty(bool, translation_window_visible, cxx_name = "translationWindowVisible")]
        /// Какой рендер работает сейчас: "inplace" или "window" (с учётом отката, см. `displayNote`).
        #[qproperty(QString, effective_display, cxx_name = "effectiveDisplay")]
        /// Почему выбранный режим заменён другим (пусто, если не заменён).
        #[qproperty(QString, display_note, cxx_name = "displayNote")]
        /// Что умеет способ захвата выбранного окна (JSON: `CaptureCapabilities` и причины отказов
        /// `inplaceBlocker`, `frameBlocker`; пустая строка — можно). Интерфейс отключает недоступное по нему.
        #[qproperty(QString, capture_capabilities, cxx_name = "captureCapabilities")]
        type Controller = super::ControllerRust;

        /// Закрепить/открепить окно перевода; закреплённое встаёт туда, где было свободное.
        #[qinvokable]
        #[cxx_name = "setTranslationWindowPinned"]
        fn set_translation_window_pinned(self: Pin<&mut Controller>, pinned: bool, source: &QString);
        /// Mark the next automatic floating-window show for one-time KWin focus restoration.
        #[qinvokable]
        #[cxx_name = "armFloatingFocusRestore"]
        fn arm_floating_focus_restore(self: &Controller);
        /// Геометрия свободного окна от QML (только X11: там Qt знает положение окна).
        #[qinvokable]
        #[cxx_name = "reportFloatingGeometry"]
        fn report_floating_geometry(self: Pin<&mut Controller>, json: &QString, reason: &QString);
        /// The desktop asks for reduced motion (KDE "Animation speed" set to instant, or `LIPAX_REDUCE_MOTION=1`):
        /// the status light then does not pulse. Qt itself has no such query.
        #[qinvokable]
        #[cxx_name = "reducedMotion"]
        fn reduced_motion(self: &Controller) -> bool;
        /// Geometry of the main window from QML (JSON `WindowRecord`); stored only while "remember window geometry" is on.
        #[qinvokable]
        #[cxx_name = "reportMainWindowGeometry"]
        fn report_main_window_geometry(self: Pin<&mut Controller>, json: &QString);
        /// Desktop geometry of the monitor explicitly selected for fullscreen portal capture.
        #[qinvokable]
        #[cxx_name = "reportPortalMonitorGeometry"]
        fn report_portal_monitor_geometry(self: Pin<&mut Controller>, json: &QString);
        /// Desktop geometry of every screen, `[[x, y, w, h], …]` in logical pixels: in-place fields are kept
        /// to the part of the game window that is on a screen.
        #[qinvokable]
        #[cxx_name = "reportScreens"]
        fn report_screens(self: Pin<&mut Controller>, json: &QString);
        #[qinvokable]
        #[cxx_name = "setInplaceVisibility"]
        fn set_inplace_visibility(self: Pin<&mut Controller>, visible: bool, source: &QString);
        #[qinvokable]
        #[cxx_name = "setTranslationWindowVisibility"]
        fn set_translation_window_visibility(self: Pin<&mut Controller>, visible: bool, source: &QString);

        #[qinvokable]
        #[cxx_name = "defaultSettingsJson"]
        fn default_settings_json(self: &Controller) -> QString;
        #[qinvokable]
        #[cxx_name = "editableSettingsPaths"]
        fn editable_settings_paths(self: &Controller) -> QString;
        #[qinvokable]
        #[cxx_name = "clearHistory"]
        fn clear_history(self: Pin<&mut Controller>);
        #[qinvokable]
        #[cxx_name = "copyText"]
        fn copy_text(self: &Controller, text: &QString);
        #[qinvokable]
        #[cxx_name = "refreshDiagnostics"]
        fn refresh_diagnostics(self: Pin<&mut Controller>);
        #[qinvokable]
        #[cxx_name = "configureTranslationWindow"]
        fn configure_translation_window(self: &Controller, passthrough: bool, rects: &QString);
        #[qinvokable]
        #[cxx_name = "configureTranslationWindowBlur"]
        fn configure_translation_window_blur(self: &Controller, enable: bool, radius: i32);
        #[qinvokable]
        #[cxx_name = "settingsJson"]
        fn settings_json(self: &Controller) -> QString;
        #[qinvokable]
        #[cxx_name = "refreshModels"]
        fn refresh_models(self: Pin<&mut Controller>);
        /// Узнать, какая версия модели выбранной пары установлена и есть ли новее (по встроенному каталогу, без сети).
        #[qinvokable]
        #[cxx_name = "refreshModelVersions"]
        fn refresh_model_versions(self: Pin<&mut Controller>);
        /// Скачать и проверить именно эту версию модели из каталога (выбор или обновление); только по кнопке.
        #[qinvokable]
        #[cxx_name = "downloadModelVersion"]
        fn download_model_version(self: Pin<&mut Controller>, pair: &QString, version: &QString);
        #[qinvokable]
        #[cxx_name = "setModelPath"]
        fn set_model_path(self: Pin<&mut Controller>, path: &QString);
        #[qinvokable]
        #[cxx_name = "applySettings"]
        fn apply_settings(self: Pin<&mut Controller>, json: &QString);
        #[qinvokable]
        #[cxx_name = "applySettingsPatch"]
        fn apply_settings_patch(self: Pin<&mut Controller>, json: &QString);
        #[qinvokable]
        #[cxx_name = "pickWindow"]
        fn pick_window(self: Pin<&mut Controller>);
        /// Забыть профиль игры (ключ — класс окна из `game_profiles`). Текущий выбор окна не меняется.
        #[qinvokable]
        #[cxx_name = "forgetGameProfile"]
        fn forget_game_profile(self: Pin<&mut Controller>, key: &QString);
        #[qinvokable]
        #[cxx_name = "requestPreview"]
        fn request_preview(self: Pin<&mut Controller>);
        #[qinvokable]
        #[cxx_name = "setRegion"]
        fn set_region(self: Pin<&mut Controller>, x: f64, y: f64, w: f64, h: f64);
        #[qinvokable]
        #[cxx_name = "resetRegion"]
        fn reset_region(self: Pin<&mut Controller>);
        #[qinvokable]
        fn start(self: Pin<&mut Controller>);
        #[qinvokable]
        fn stop(self: Pin<&mut Controller>);
        /// Запустить слежение, если есть область; иначе статус «Сначала выберите область» и `false`.
        #[qinvokable]
        #[cxx_name = "startAutoTranslate"]
        fn start_auto_translate(self: Pin<&mut Controller>) -> bool;
        #[qinvokable]
        #[cxx_name = "stopAutoTranslate"]
        fn stop_auto_translate(self: Pin<&mut Controller>);
        /// Вывести окно (по `objectName`) на передний план. Wayland: по токену xdg-activation, он может быть пуст.
        #[qinvokable]
        #[cxx_name = "activateWindow"]
        fn activate_window(self: &Controller, object_name: &QString, token: &QString);
        /// Окно просмотра входа OCR открыто/закрыто: пока закрыто, кадры не копируются и не кодируются.
        #[qinvokable]
        #[cxx_name = "setOcrPreviewEnabled"]
        fn set_ocr_preview_enabled(self: Pin<&mut Controller>, enabled: bool);
        #[qinvokable]
        #[cxx_name = "translateOnce"]
        fn translate_once(self: Pin<&mut Controller>);
        /// «Поверх исходного текста»: определить шрифты полей заново (выбор шрифта иначе зафиксирован за полем).
        /// Семейства шрифтов приложения (JSON-массив): для выбора шрифта в настройках.
        #[qinvokable]
        #[cxx_name = "bundledFonts"]
        fn bundled_fonts(self: &Controller) -> QString;
        /// Подобрать фильтры изображения для текущего кадра: результат придёт в `autotuneJson`, лучший набор
        /// сохраняется в настройках (и в профиле игры).
        #[qinvokable]
        #[cxx_name = "autoTuneFilters"]
        fn auto_tune_filters(self: Pin<&mut Controller>);
        #[qinvokable]
        #[cxx_name = "reanalyzeFonts"]
        fn reanalyze_fonts(self: Pin<&mut Controller>);
        /// Заново проверить Tesseract и список языков (без перезапуска приложения).
        #[qinvokable]
        #[cxx_name = "refreshTesseract"]
        fn refresh_tesseract(self: Pin<&mut Controller>);
        /// Версия приложения (для вкладки «О программе»).
        #[qinvokable]
        #[cxx_name = "appVersion"]
        fn app_version(self: &Controller) -> QString;
        #[qinvokable]
        #[cxx_name = "themePolicy"]
        fn theme_policy(self: &Controller, requested: &QString) -> QString;
        #[qinvokable]
        #[cxx_name = "qtVersion"]
        fn qt_version(self: &Controller) -> QString;
        /// Проверить окружение PaddleOCR (пакеты, версии, способ установки, язык, модели); результат — в `paddleJson`.
        /// Ничего не устанавливает: инструкции показывает интерфейс.
        #[qinvokable]
        #[cxx_name = "refreshPaddle"]
        fn refresh_paddle(self: Pin<&mut Controller>);
        /// Check the RapidOCR model of the recognition language and ONNX Runtime; result in `rapidJson`. No network.
        #[qinvokable]
        #[cxx_name = "refreshRapid"]
        fn refresh_rapid(self: Pin<&mut Controller>);
        /// Download a RapidOCR model from the catalog (button «Скачать»): progress in `rapidProgress`, errors in `rapidError`.
        #[qinvokable]
        #[cxx_name = "downloadRapidModel"]
        fn download_rapid_model(self: Pin<&mut Controller>, id: &QString);
        #[qinvokable]
        #[cxx_name = "deleteRapidModel"]
        fn delete_rapid_model(self: Pin<&mut Controller>, id: &QString);
        /// Установить языковой пакет. Вызывается только после подтверждения пользователя в GUI.
        #[qinvokable]
        #[cxx_name = "installPackage"]
        fn install_package(self: Pin<&mut Controller>, package: &QString);
        /// Скачать языковую модель Tesseract в каталог пользователя (там, где пакета в репозитории нет); пароль не нужен.
        /// Вызывается только после подтверждения пользователя в GUI.
        #[qinvokable]
        #[cxx_name = "installLanguage"]
        fn install_language(self: Pin<&mut Controller>, code: &QString);
        /// Сообщения о недостающих языках из строки вида `jpn+eng` (JSON-массив).
        #[qinvokable]
        #[cxx_name = "missingLanguages"]
        fn missing_languages(self: &Controller, spec: &QString) -> QString;

        #[qsignal]
        #[cxx_name = "previewReady"]
        fn preview_ready(self: Pin<&mut Controller>);
        /// Показать рамку (координаты рабочего стола): после выбора окна или области.
        #[qsignal]
        #[cxx_name = "frameRequested"]
        fn frame_requested(self: Pin<&mut Controller>, x: f64, y: f64, w: f64, h: f64);
        /// Горячая клавиша «выбрать область»: окно выбора открывает QML.
        #[qsignal]
        #[cxx_name = "selectRegionRequested"]
        fn select_region_requested(self: Pin<&mut Controller>);
        /// Горячая клавиша «закрепить / открепить перевод»: работает при любом режиме рамки.
        #[qsignal]
        #[cxx_name = "togglePinRequested"]
        fn toggle_pin_requested(self: Pin<&mut Controller>);
        /// Действие извне: командная строка, Desktop Actions, повторный запуск. Имена —
        /// `instance::Action::name`; окно и меню трея обрабатывает QML, у него же состояние окон.
        #[qsignal]
        #[cxx_name = "actionRequested"]
        fn action_requested(self: Pin<&mut Controller>, action: &QString, activation_token: &QString);
    }

    impl cxx_qt::Threading for Controller {}
    impl cxx_qt::Initialize for Controller {}
}

use core::pin::Pin;
use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::QString;
use lipa_core::capture::kwin::KwinCapture;
use lipa_core::capture::{backend_name, capabilities_for};
use lipa_core::capture::portal::is_portal_window;
use lipa_core::capture::AnyCapture;
use lipa_core::hotkeys::{self, HotkeyAction, HotkeyEvent};
use lipa_core::ocr::AnyOcr;
use lipa_core::history::{History, Entry};
use lipa_core::layout::engine::InplaceFrame;
use lipa_core::layout::place::{PlacementCache, RegionInput, Viewport, place_regions};
use lipa_core::pipeline::{Cmd, Event, Pipeline};
use lipa_core::settings::{CaptureSource, NormRect, Settings};
use lipa_core::tesseract::{TesseractInfo, TesseractManager};
use lipa_core::translate::HttpTranslate;
use std::sync::{Arc, OnceLock};
use tokio::sync::{mpsc, watch};

mod capabilities;
mod invokables;
mod preview;
mod queries;
use capabilities::*;
use preview::*;
#[cfg(test)]
mod display_tests;


/// How often the position of the game window is read.
const GEOMETRY_POLL: std::time::Duration = std::time::Duration::from_millis(100);

pub(crate) fn rt() -> &'static tokio::runtime::Runtime {
    static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("tokio runtime")
    })
}

#[derive(Default)]
struct RuntimeServices {
    tasks: Vec<tokio::task::JoinHandle<()>>,
    capture: Option<Arc<AnyCapture>>,
}

fn services() -> &'static std::sync::Mutex<RuntimeServices> {
    static SERVICES: OnceLock<std::sync::Mutex<RuntimeServices>> = OnceLock::new();
    SERVICES.get_or_init(Default::default)
}

fn spawn_service(future: impl std::future::Future<Output = ()> + Send + 'static) {
    let task = rt().spawn(future);
    let mut services = services().lock().unwrap();
    services.tasks.retain(|task| !task.is_finished());
    services.tasks.push(task);
}

pub fn shutdown() {
    crate::model_worker::shutdown();
    remove_preview_files();
    let services = std::mem::take(&mut *services().lock().unwrap());
    rt().block_on(async move {
        for task in &services.tasks { task.abort(); }
        for task in services.tasks { let _ = task.await; }
        if let Some(capture) = services.capture { capture.portal.close().await; }
        lipa_core::capture::shutdown_geometry().await;
    });
}

struct Shared {
    settings: watch::Sender<Settings>,
    running: watch::Sender<bool>,
    cmds: mpsc::UnboundedSender<Cmd>,
    hotkeys: watch::Sender<lipa_core::settings::Hotkeys>,
    capture: Arc<AnyCapture>,
    /// Поколение настроек обработки: растёт при смене окна, области, языка, движка. События pipeline
    /// старого поколения (отправлены до смены, но ещё не доставлены) GUI отбрасывает.
    generation: Arc<std::sync::atomic::AtomicU64>,
    /// Окно просмотра входа OCR открыто; pipeline присылает кадры только тогда.
    preview: Arc<std::sync::atomic::AtomicBool>,
    /// Игра в фоне и перевод скрыт: автоматические циклы pipeline пропускаются.
    paused: Arc<std::sync::atomic::AtomicBool>,
    /// Последний результат каждой области в виде JSON-значения (см. `preview_entry`).
    preview_entries: std::sync::Mutex<std::collections::BTreeMap<String, serde_json::Value>>,
    /// Backdrop files of the in-place fields: (region, block) -> (revision written, URL). Written off the GUI thread when
    /// the frame arrives; the GUI thread only looks the URL up.
    backdrops: std::sync::Mutex<std::collections::HashMap<(String, u64), (u64, String)>>,
}

impl Shared {
    async fn kwin(&self) -> Result<Arc<KwinCapture>, String> {
        self.capture.kwin().await.map_err(|e| e.to_string())
    }

    /// Изменить настройки, сохранить на диск и сбросить состояние pipeline.
    fn update(&self, f: impl FnOnce(&mut Settings)) {
        let _ = self.update_checked(|s| { f(s); Ok(()) });
    }

    fn update_checked(&self, f: impl FnOnce(&mut Settings) -> Result<(), String>) -> Result<(), String> {
        let before = self.settings.borrow().processing_key();
        let mut error = None;
        self.settings.send_if_modified(|s| match f(s) {
            Ok(()) => { s.sanitize(); s.remember_game(); true }
            Err(e) => { error = Some(e); false }
        });
        if let Some(e) = error { return Err(e); }
        let saved = self.settings.borrow().save().map_err(|e| {
            tracing::error!(component = "settings", error = %e, "Не удалось сохранить настройки");
            e.to_string()
        });
        if self.settings.borrow().processing_key() != before {
            // Before Reset: whatever the running tick still sends is already stale.
            self.generation.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let _ = self.cmds.send(Cmd::Reset);
        }
        // Смена горячих клавиш применяется сразу, без перезапуска.
        let hk = self.settings.borrow().hotkeys.clone();
        self.hotkeys.send_if_modified(|h| {
            let changed = *h != hk;
            if changed {
                *h = hk;
            }
            changed
        });
        saved
    }
}

/// Apply only registered leaf paths to the latest backend state. A delayed appearance
/// autosave must never replace a newly selected capture window, region or Portal token.
fn merge_settings_patch(current: &Settings, patch: &serde_json::Map<String, serde_json::Value>) -> Result<Settings, String> {
    let mut state = serde_json::to_value(current).map_err(|e| e.to_string())?;
    for (key, value) in patch {
        match lipa_core::settings::reaction(key) {
            Some(lipa_core::settings::Reaction::Managed) => return Err(format!("field {key} is managed by the backend")),
            None => return Err(format!("unknown settings path: {key}")),
            _ => {}
        }
        let mut node = &mut state;
        for part in key.split('.') { node = node.get_mut(part).ok_or_else(|| format!("unknown settings path: {key}"))?; }
        *node = value.clone();
    }
    serde_json::from_value(state).map_err(|e| e.to_string())
}

/// Pipeline events (stamped with the settings generation), their sender, and the commands.
type PipelineChannels = (mpsc::UnboundedReceiver<(u64, Event)>, mpsc::UnboundedSender<(u64, Event)>, mpsc::UnboundedReceiver<Cmd>);

pub struct ControllerRust {
    status: QString,
    status_kind: QString,
    settings_state: QString,
    model_state: QString,
    model_busy: bool,
    model_versions: QString,
    /// Only the answer to the latest version check is published.
    model_versions_generation: u64,
    model_checked: std::collections::BTreeMap<String, model_controller::ModelPaths>,
    model_results: std::collections::BTreeMap<String, String>,
    history_json: QString,
    diagnostics_json: QString,
    diagnostics_busy: bool,
    history: History,
    region_text: std::collections::BTreeMap<String, (String, String)>,
    /// Последние поля «поверх оригинала» каждой области: размер кадра и поля.
    region_inplace: std::collections::BTreeMap<String, Arc<InplaceFrame>>,
    /// Подгонка полей кэшируется: перемещение окна игры и повторные публикации ничего не пересчитывают.
    placement_cache: PlacementCache,
    /// Screens of the desktop as QML reports them (logical px): the in-place fields stay on them.
    screens: Vec<lipa_core::layout::DesktopRect>,
    /// PNG подложек: (область, поле) → (ревизия, URL). Файл пишется только при новой ревизии.
    inplace_json: QString,
    inplace_fallback_json: QString,
    ocr_preview_json: QString,
    faults: std::collections::BTreeMap<String, (String, bool)>,
    original: QString,
    translation: QString,
    tesseract_json: QString,
    tesseract_busy: bool,
    paddle_json: QString,
    paddle_busy: bool,
    rapid_json: QString,
    rapid_busy: bool,
    rapid_progress: i32,
    rapid_model: QString,
    rapid_error: QString,
    /// Only the answer to the latest RapidOCR check is published.
    rapid_generation: u64,
    window_title: QString,
    preview_source: QString,
    preview_title_bar: i32,
    game_geometry: QString,
    running: bool,
    has_region: bool,
    shared: Arc<Shared>,
    events: Option<PipelineChannels>,
    settings_rx: watch::Receiver<Settings>,
    running_rx: watch::Receiver<bool>,
    inplace_visible: bool,
    overlay_allowed: bool,
    autotune_json: QString,
    /// Что из «Общих» настроек уже применено (журнал, автозапуск): сравнивается при каждом изменении настроек.
    applied_general: lipa_core::settings::GeneralSettings,
    translation_window_visible: bool,
    effective_display: QString,
    display_note: QString,
    capture_capabilities: QString,
}

impl Default for ControllerRust {
    fn default() -> Self {
        let settings = Settings::load();
        let history = if settings.history_persist { History::load(&History::path(), settings.history_limit) } else { History::default() };
        let (settings_tx, settings_rx) = watch::channel(settings.clone());
        let (running_tx, running_rx) = watch::channel(false);
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
        let (ev_tx, ev_rx) = mpsc::unbounded_channel();
        Self {
            status: QString::from("Выберите окно игры"),
            status_kind: QString::from("info"),
            settings_state: QString::from(serde_json::to_string(&settings).unwrap().as_str()),
            history_json: QString::from(serde_json::to_string(&history.entries).unwrap().as_str()),
            diagnostics_json: QString::from("[]"),
            diagnostics_busy: false,
            model_state: QString::from("{}"),
            model_busy: false,
            model_versions: QString::from("{}"),
            model_versions_generation: 0,
            model_checked: Default::default(),
            model_results: Default::default(),
            history,
            region_text: Default::default(),
            region_inplace: Default::default(),
            placement_cache: PlacementCache::default(),
            screens: Vec::new(),
            inplace_json: QString::from("[]"),
            inplace_fallback_json: QString::from(r#"{"texts":[],"degraded":0}"#),
            ocr_preview_json: QString::from("[]"),
            faults: Default::default(),
            original: QString::default(),
            translation: QString::default(),
            tesseract_json: QString::default(),
            tesseract_busy: false,
            paddle_json: QString::default(),
            paddle_busy: false,
            rapid_json: QString::default(),
            rapid_busy: false,
            rapid_progress: 0,
            rapid_model: QString::default(),
            rapid_error: QString::default(),
            rapid_generation: 0,
            window_title: QString::from(
                settings
                    .capture.window
                    .as_ref()
                    .map(|w| w.caption.as_str())
                    .unwrap_or(""),
            ),
            preview_source: QString::default(),
            preview_title_bar: 0,
            game_geometry: QString::default(),
            inplace_visible: true,
            overlay_allowed: true,
            autotune_json: QString::default(),
            applied_general: settings.general.clone(),
            translation_window_visible: true,
            effective_display: QString::from(effective_display(&settings).0),
            display_note: QString::from(effective_display(&settings).1.as_str()),
            capture_capabilities: QString::from(capabilities_json(&settings).as_str()),
            running: false,
            has_region: !settings.capture_regions().is_empty(),
            shared: Arc::new(Shared {
                hotkeys: watch::channel(settings.hotkeys.clone()).0,
                settings: settings_tx,
                running: running_tx,
                cmds: cmd_tx,
                capture: Arc::new(AnyCapture::new(settings.capture.portal_token.clone())),
                generation: Arc::default(),
                preview: Arc::default(),
                paused: Arc::default(),
                preview_entries: Default::default(),
                backdrops: Default::default(),
            }),
            events: Some((ev_rx, ev_tx, cmd_rx)),
            settings_rx,
            running_rx,
        }
    }
}

impl cxx_qt::Initialize for qobject::Controller {
    fn initialize(mut self: Pin<&mut Self>) {
        self.as_mut().check_models();
        let (mut ev_rx, ev_tx, cmd_rx) = self
            .as_mut()
            .rust_mut()
            .events
            .take()
            .expect("initialize вызывается один раз");
        let shared = self.rust().shared.clone();
        let (settings_rx, running_rx) = (
            self.rust().settings_rx.clone(),
            self.rust().running_rx.clone(),
        );

        // Pipeline: захват → OCR → перевод в фоне. Бэкенд захвата выбирается по ключу окна.
        let capture = shared.capture.clone();
        let (preview, paused) = (shared.preview.clone(), shared.paused.clone());
        let generation = shared.generation.clone();
        spawn_service(async move {
            Pipeline::new(capture, AnyOcr::default(), HttpTranslate::new()).with_preview(preview).with_pause(paused).run(settings_rx, running_rx, cmd_rx, generation, ev_tx).await
        });

        {
            let mut services = services().lock().unwrap();
            services.capture = Some(shared.capture.clone());
        }

        // Токен восстановления portal сохраняется, чтобы окно выбиралось без диалога при следующем запуске.
        let sh = shared.clone();
        spawn_service(async move {
            let mut rx = sh.capture.portal.token_updates();
            while rx.changed().await.is_ok() {
                let token = rx.borrow_and_update().clone();
                sh.settings.send_modify(|s| s.capture.portal_token = token);
                if let Err(e) = sh.settings.borrow().save() {
                    tracing::error!(component = "portal", error = %e, "Не удалось сохранить настройки с токеном Portal");
                }
            }
        });

        // Глобальные горячие клавиши (KGlobalAccel). Недоступность не критична — только статус.
        let (hk_tx, mut hk_rx) = mpsc::unbounded_channel();
        let hk = shared.hotkeys.subscribe();
        let qt = self.qt_thread();
        spawn_service(async move {
            if let Err(e) = hotkeys::listen(hk, hk_tx).await {
                tracing::error!(component = "KGlobalAccel/D-Bus", error = %e, "Глобальные клавиши недоступны");
                let _ = qt.queue(move |mut o| {
                    o.as_mut()
                        .set_status(QString::from(format!("{e}").as_str()))
                });
            }
        });
        let qt = self.qt_thread();
        spawn_service(async move {
            while let Some(ev) = hk_rx.recv().await {
                let _ = qt.queue(move |mut o| match ev {
                    HotkeyEvent::Conflict(a) => {
                        tracing::warn!(component = "KGlobalAccel", action = a.title(), "Сочетание занято или недопустимо");
                        o.as_mut().set_status(QString::from(format!("Сочетание для «{}» занято или недопустимо", a.title()).as_str()));
                    },
                    HotkeyEvent::Pressed(action) => match action {
                        HotkeyAction::Toggle if *o.running() => o.as_mut().stop(),
                        HotkeyAction::Toggle => { o.as_mut().start_auto_translate(); }
                        HotkeyAction::TranslateOnce => o.as_mut().translate_once(),
                        HotkeyAction::SelectRegion => o.as_mut().select_region_requested(),
                        // Показывает/скрывает перевод активного режима — второй режим не трогается.
                        HotkeyAction::ToggleTranslation => {
                            let source = QString::from("hotkey");
                            if o.effective_display().to_string() == "inplace" {
                                let v = !*o.inplace_visible();
                                o.as_mut().set_inplace_visibility(v, &source);
                            } else {
                                let v = !*o.translation_window_visible();
                                o.as_mut().set_translation_window_visibility(v, &source);
                            }
                        }
                        // Закрепление есть только у окна перевода.
                        HotkeyAction::TogglePin => {
                            if o.effective_display().to_string() == "window" { o.as_mut().toggle_pin_requested(); }
                        }
                    },
                });
            }
        });

        // Свободное окно перевода — обычное окно: на Wayland его положение знает и задаёт только
        // композитор. Скрипт KWin восстанавливает сохранённое место и сообщает новое после
        // перемещения; без KWin место выбирает композитор.
        let sh = shared.clone();
        let qt = self.qt_thread();
        spawn_service(async move {
            let kwin = match sh.kwin().await {
                Ok(k) => k,
                Err(e) => {
                    tracing::info!(target: "translation_window.geometry", error = %e, "KWin недоступен: положение свободного окна выбирает композитор");
                    return;
                }
            };
            let mut settings = sh.settings.subscribe();
            let placement = floating_placement(&settings.borrow_and_update());
            if !kwin.set_floating_placement(placement).await {
                tracing::warn!(target: "translation_window.geometry", "скрипт KWin недоступен: положение свободного окна не восстанавливается");
                return;
            }
            let Some(mut moves) = kwin.floating_moves().await else { return };
            loop {
                tokio::select! {
                    changed = settings.changed() => {
                        if changed.is_err() { break; }
                        let placement = floating_placement(&settings.borrow_and_update());
                        kwin.set_floating_placement(placement).await;
                    }
                    changed = moves.changed() => {
                        if changed.is_err() { break; }
                        let moved = moves.borrow_and_update().clone();
                        if let Some((g, reason)) = moved {
                            store_floating_geometry(&sh, g, &reason);
                            if qt.queue(|mut o| o.as_mut().publish_settings()).is_err() { break; }
                        }
                    }
                }
            }
        });

        // Client geometry follows window moves and monitor changes, even while paused.
        let sh = shared.clone();
        let qt = self.qt_thread();
        spawn_service(async move {
            // Ten times a second: while the window is dragged the overlay follows it in steps of 100 ms rather
            // than 500 ms. A read is a lookup in the state the KWin script keeps, and only changes reach the GUI thread.
            let mut ticker = tokio::time::interval(GEOMETRY_POLL);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            let mut last: Option<String> = None;
            let mut allowed = true;
            loop {
                ticker.tick().await;
                // Only what this needs: a clone of the whole settings ten times a second would be waste.
                let (window, fills_monitor, only_active) = { let s = sh.settings.borrow(); (s.capture.window.clone(), s.capture.portal_fills_monitor, s.appearance.inplace.only_when_active) };
                if window.as_ref().is_some_and(is_portal_window) && fills_monitor {
                    // QML reports the selected Qt screen in logical desktop coordinates.
                    if !allowed { allowed = true; sh.paused.store(false, std::sync::atomic::Ordering::Relaxed); if qt.queue(|mut o| o.as_mut().set_overlay_allowed(true)).is_err() { break; } }
                    continue;
                }
                let (geometry, active) = match window.as_ref().filter(|w| !is_portal_window(w)) {
                    Some(w) => match sh.kwin().await {
                        // A window KWin does not know (or no script): nothing to hide by.
                        Ok(k) => (k.window_geometry(&w.uuid).await, if only_active { k.window_active(&w.uuid).await } else { None }),
                        Err(e) => { tracing::debug!(component = "KWin/D-Bus", error = %e, "Геометрия окна недоступна"); (None, None) },
                    },
                    None => (None, None),
                };
                let now_allowed = active.unwrap_or(true);
                if now_allowed != allowed {
                    allowed = now_allowed;
                    sh.paused.store(!allowed, std::sync::atomic::Ordering::Relaxed);
                    tracing::debug!(target: "inplace.visibility", allowed, "the game window {}", if allowed { "is active again: overlay restored" } else { "is not active: overlay hidden" });
                    if qt.queue(move |mut o| o.as_mut().set_overlay_allowed(now_allowed)).is_err() { break; }
                }
                // Ignore a result for a window that was replaced during the await.
                if sh.settings.borrow().capture.window != window { continue; }
                let json = geometry.map(|g| serde_json::json!([g.x, g.y, g.w, g.h]).to_string()).unwrap_or_default();
                if last.as_ref() == Some(&json) { continue; }
                last = Some(json.clone());
                if qt.queue(move |mut o| {
                    o.as_mut().set_game_geometry(QString::from(json.as_str()));
                    // Кегль и поля подгоняются в пикселях экрана: окно изменило размер — подогнать заново.
                    o.as_mut().publish_inplace();
                }).is_err() { break; }
            }
        });


        // Действия извне (D-Bus `org.freedesktop.Application`, командная строка) → QML.
        // Очередь живёт, пока работает процесс, а не окно: backend не зависит от GUI.
        if let Some(mut requests) = crate::instance::take_requests() {
            let qt = self.qt_thread();
            spawn_service(async move {
                while let Some(request) = requests.recv().await {
                    tracing::info!(target: "instance", action = request.action.name(), "действие извне");
                    let delivered = qt.queue(move |mut o| {
                        o.as_mut().action_requested(&QString::from(request.action.name()), &QString::from(request.activation_token.as_str()));
                    });
                    if delivered.is_err() { break; }
                }
            });
        }

        // Первичная проверка Tesseract в фоне, чтобы окно настроек открывалось сразу с данными.
        self.as_mut().refresh_tesseract();

        // Доставка событий в GUI-поток.
        let qt = self.qt_thread();
        spawn_service(async move {
            let preview_shared = shared.clone();
            while let Some((generation, ev)) = ev_rx.recv().await {
                // Sent before the window, area, language or engine changed: describes a state that is gone.
                if generation != preview_shared.generation.load(std::sync::atomic::Ordering::SeqCst) {
                    tracing::debug!(target: "pipeline.generation", generation, "stale pipeline event dropped");
                    continue;
                }
                // The frame is encoded here, off the GUI thread.
                let ev = match ev {
                    Event::OcrPreview { region_id, region_name, preview } => {
                        let shared = preview_shared.clone();
                        let json = tokio::task::spawn_blocking(move || publish_preview(&shared, region_id, region_name, *preview)).await.ok().flatten();
                        if let Some(json) = json
                            && qt.queue(move |mut o| {
                                // The window may have been closed (and its files removed) while this answer was on its way.
                                if o.rust().shared.preview.load(std::sync::atomic::Ordering::Relaxed) {
                                    o.as_mut().set_ocr_preview_json(QString::from(json.as_str()));
                                }
                            }).is_err() { break; }
                        continue;
                    }
                    // The backdrop pictures of the fields are encoded here too: the GUI thread places the fields, it does not write PNGs.
                    Event::Inplace { region_id, region_name, frame } => {
                        let shared = preview_shared.clone();
                        let encoded = tokio::task::spawn_blocking(move || { write_backdrops(&shared, &region_id, &frame); (region_id, region_name, frame) }).await;
                        match encoded {
                            Ok((region_id, region_name, frame)) => Event::Inplace { region_id, region_name, frame },
                            Err(_) => continue,
                        }
                    }
                    other => other,
                };
                let _ = qt.queue(move |mut o| {
                    // Settings can change after the worker queues an event but before Qt delivers it.
                    if generation != o.rust().shared.generation.load(std::sync::atomic::Ordering::SeqCst) { return; }
                    match ev {
                    Event::Status(s) => {
                        if o.rust().faults.is_empty() {
                            o.as_mut().set_status(QString::from(s.as_str()));
                            o.as_mut().set_status_kind(QString::from("info"));
                        }
                    },
                    Event::Cleared { region_id } => {
                        o.as_mut().rust_mut().faults.remove(&region_id);
                        if o.rust().faults.is_empty() {
                            o.as_mut().set_status(QString::from("Ожидание текста"));
                            o.as_mut().set_status_kind(QString::from("info"));
                        }
                        o.as_mut().publish_faults();
                    },
                    Event::Error { region_id, message, terminal } => {
                        let general = o.rust().shared.settings.borrow().general.clone();
                        if (if terminal { general.notify_errors } else { general.notify_retries })
                            && crate::notify::due(&format!("{region_id}\u{1}{message}"), std::time::Instant::now()) {
                            let (summary, body) = (if terminal { "LipaX: перевод остановлен" } else { "LipaX: ошибка, будет повтор" }, message.clone());
                            rt().spawn(async move {
                                if let Err(e) = crate::notify::send(summary, &body).await {
                                    tracing::debug!(component = "notify", error = %e, "уведомление не показано");
                                }
                            });
                        }
                        o.as_mut().rust_mut().faults.insert(region_id, (message, terminal));
                        o.as_mut().publish_faults();
                    },
                    Event::Inplace { region_id, region_name, frame } => {
                        tracing::debug!(target: "inplace.delivery", region = %region_id, generation, blocks = frame.blocks.len(), "Event::Inplace delivered on GUI thread");
                        let settings = o.rust().shared.settings.borrow().clone();
                        if !settings.capture_regions().iter().any(|r| r.id == region_id) { return; }
                        o.as_mut().rust_mut().faults.remove(&region_id);
                        let original = frame.blocks.iter().map(|b| b.original.as_str()).collect::<Vec<_>>().join("\n");
                        let text = frame.blocks.iter().map(|b| b.translation.as_str()).collect::<Vec<_>>().join("\n");
                        o.as_mut().rust_mut().region_text.insert(region_id.clone(), (original, text));
                        if settings.history_enabled && !frame.new_translations.is_empty() {
                            let timestamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as u64;
                            for (original, translation) in &frame.new_translations {
                                o.as_mut().rust_mut().history.push(Entry { timestamp, region: region_name.clone(), original: original.clone(), translation: translation.clone() }, settings.history_limit);
                            }
                            o.as_mut().publish_history();
                        }
                        let frame = *frame;
                        o.as_mut().rust_mut().region_inplace.insert(region_id, Arc::new(frame));
                        o.as_mut().publish_translation();
                        o.as_mut().set_status(QString::from("Перевод обновлён"));
                        o.as_mut().set_status_kind(QString::from("info"));
                        o.as_mut().publish_faults();
                    },
                    Event::OcrPreview { .. } => {}
                    Event::AutoTune { region_id, results, best } => {
                        let winner = &results[best];
                        let f = winner.filters;
                        let patch = serde_json::json!({
                            "recognition.binarize": f.binarize, "recognition.auto_invert": f.auto_invert,
                            "recognition.contrast": f.contrast, "recognition.sharpen": f.sharpen,
                        });
                        let json = serde_json::json!({
                            "region": region_id, "best": best, "applied": winner.score > 0.0,
                            // Another answer for the same frame is another value: the window waits for a change.
                            "at": std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64),
                            "results": results.iter().map(|r| serde_json::json!({ "name": r.name, "score": r.score })).collect::<Vec<_>>(),
                        });
                        // Nothing was read with any filter: the settings stay as they are.
                        if winner.score > 0.0 {
                            o.as_mut().apply_settings_patch(&QString::from(patch.to_string().as_str()));
                            o.as_mut().set_status(QString::from(format!("Фильтры подобраны: {}", winner.name).as_str()));
                        } else {
                            o.as_mut().set_status(QString::from("Подбор фильтров: текст не найден ни с одним набором"));
                        }
                        o.as_mut().set_status_kind(QString::from("info"));
                        o.as_mut().set_autotune_json(QString::from(json.to_string().as_str()));
                    }
                    Event::Translation { region_id, region_name, original, text } => {
                        let settings = o.rust().shared.settings.borrow().clone();
                        if !settings.capture_regions().iter().any(|r| r.id == region_id) { return; }
                        o.as_mut().rust_mut().faults.remove(&region_id);
                        o.as_mut().rust_mut().region_text.insert(region_id, (original.clone(), text.clone()));
                        if settings.history_enabled {
                            let timestamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as u64;
                            o.as_mut().rust_mut().history.push(Entry { timestamp, region: region_name, original, translation: text }, settings.history_limit);
                            o.as_mut().publish_history();
                        }
                        o.as_mut().publish_translation();
                        o.as_mut().set_status(QString::from("Перевод обновлён"));
                        o.as_mut().set_status_kind(QString::from("info"));
                        o.as_mut().publish_faults();
                    }
                    }
                });
            }
        });
    }
}



/// Рамка вокруг только что выбранного окна. Геометрию отдаёт KWin; рамки областей рисует QML (RegionFrame).
fn show_frame(shared: Arc<Shared>, qt: cxx_qt::CxxQtThread<qobject::Controller>) {
    spawn_service(async move {
        let Some(uuid) = shared.settings.borrow().capture.window.clone().filter(|w| !is_portal_window(w)).map(|w| w.uuid) else {
            return; // у окна без геометрии (portal) положение на рабочем столе неизвестно
        };
        let Ok(kwin) = shared.kwin().await else { return };
        let Some(g) = kwin.window_geometry(&uuid).await else { return };
        let _ = qt.queue(move |mut o| o.as_mut().frame_requested(g.x, g.y, g.w, g.h));
    });
}

