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
        /// Desktop geometry of the monitor explicitly selected for fullscreen portal capture.
        #[qinvokable]
        #[cxx_name = "reportPortalMonitorGeometry"]
        fn report_portal_monitor_geometry(self: Pin<&mut Controller>, json: &QString);
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
use lipa_core::layout::place::{PlacementCache, RegionInput, place_regions};
use lipa_core::pipeline::{Cmd, Event, Pipeline};
use lipa_core::settings::{CaptureSource, NormRect, Settings};
use lipa_core::tesseract::{TesseractInfo, TesseractManager};
use lipa_core::translate::HttpTranslate;
use std::sync::{Arc, OnceLock};
use tokio::sync::{mpsc, watch};

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
    /// PNG подложек: (область, поле) → (ревизия, URL). Файл пишется только при новой ревизии.
    inplace_images: std::collections::HashMap<(String, u64), (u64, String)>,
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
            inplace_images: Default::default(),
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
                            && qt.queue(move |mut o| o.as_mut().set_ocr_preview_json(QString::from(json.as_str()))).is_err() { break; }
                        continue;
                    }
                    other => other,
                };
                let _ = qt.queue(move |mut o| match ev {
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
                });
            }
        });
    }
}

impl qobject::Controller {
    fn arm_floating_focus_restore(&self) {
        lipa_core::capture::arm_floating_focus_restore();
        tracing::debug!(target: "translation_window.focus", "armed one-time focus restoration for automatic floating show");
    }
    fn default_settings_json(&self) -> QString {
        QString::from(serde_json::to_string(&Settings::default()).unwrap().as_str())
    }
    fn editable_settings_paths(&self) -> QString {
        let paths: Vec<_> = lipa_core::settings::REACTIONS.iter()
            .filter(|(_, reaction)| *reaction != lipa_core::settings::Reaction::Managed).map(|(path, _)| path).collect();
        QString::from(serde_json::to_string(&paths).unwrap().as_str())
    }
    fn copy_text(&self, text: &QString) { crate::icon::copy_text(text); }
    fn configure_translation_window_blur(&self, enable: bool, radius: i32) { crate::icon::translation_window_blur(enable, radius); }
    /// `rects` — JSON `[[x, y, w, h], ...]`: где overlay принимает ввод, когда клики идут сквозь него.
    fn configure_translation_window(&self, passthrough: bool, rects: &QString) {
        let rects: Vec<[i32; 4]> = serde_json::from_str(&rects.to_string()).unwrap_or_default();
        crate::icon::translation_window_input(passthrough, &rects.concat());
    }
    fn set_translation_window_pinned(mut self: Pin<&mut Self>, pinned: bool, source: &QString) {
        // Cancel an automatic show that was superseded before KWin saw the floating surface.
        if pinned { lipa_core::capture::clear_floating_focus_restore(); }
        self.rust().shared.update(|s| {
            if pinned {
                // Закреплённое окно — на месте свободного, на том же выходе.
                if let Some(g) = s.translation_window.floating_geometry.clone() {
                    s.translation_window.screen = g.output.clone();
                    s.translation_window.position = g.relative();
                    tracing::debug!(target: "translation_window.geometry", reason = "pin_transition", screen = %g.output,
                        x = s.translation_window.position.0, y = s.translation_window.position.1, "pinned placement from floating geometry");
                }
            }
            s.translation_window.mode = lipa_core::settings::TranslationWindowMode::from_pinned(pinned);
        });
        tracing::debug!(target: "translation_window.state", pinned, floating = !pinned,
            window_role = if pinned { "layer_shell" } else { "xdg_toplevel" }, source = %source, "translation window pin state");
        self.as_mut().publish_settings();
    }
    fn report_floating_geometry(mut self: Pin<&mut Self>, json: &QString, reason: &QString) {
        match serde_json::from_str::<lipa_core::settings::FloatingGeometry>(&json.to_string()) {
            Ok(g) if g.is_valid() => {
                store_floating_geometry(&self.rust().shared, g, &reason.to_string());
                self.as_mut().publish_settings();
            }
            _ => tracing::warn!(target: "translation_window.geometry", json = %json, "invalid floating geometry from QML"),
        }
    }
    fn report_portal_monitor_geometry(mut self: Pin<&mut Self>, json: &QString) {
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
    fn set_inplace_visibility(mut self: Pin<&mut Self>, visible: bool, source: &QString) {
        if *self.inplace_visible() != visible {
            tracing::debug!(target: "inplace.state", visible, source = %source, "inplace visibility");
            self.as_mut().set_inplace_visible(visible);
        }
    }
    fn set_translation_window_visibility(mut self: Pin<&mut Self>, visible: bool, source: &QString) {
        if *self.translation_window_visible() != visible {
            tracing::debug!(target: "translation_window.state", visible, source = %source, "translation window visibility");
            self.as_mut().set_translation_window_visible(visible);
        }
    }
    /// Активный рендер по настройкам и бэкенду захвата; меняется — пишется в журнал.
    fn publish_display(mut self: Pin<&mut Self>) {
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
    fn apply_general(mut self: Pin<&mut Self>) {
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

    fn publish_settings(mut self: Pin<&mut Self>) {
        self.as_mut().check_models();
        self.as_mut().apply_general();
        self.as_mut().publish_display();
        let s = self.rust().shared.settings.borrow().clone();
        let capabilities = capabilities_json(&s);
        if self.capture_capabilities().to_string() != capabilities {
            self.as_mut().set_capture_capabilities(QString::from(capabilities.as_str()));
        }
        self.as_mut().set_settings_state(QString::from(serde_json::to_string(&s).unwrap().as_str()));
        self.as_mut().set_has_region(!s.capture_regions().is_empty());
        let active: Vec<_> = s.capture_regions().into_iter().map(|r| r.id).collect();
        self.as_mut().rust_mut().faults.retain(|id, _| active.contains(id));
        self.as_mut().publish_faults();
    }
    fn publish_translation(mut self: Pin<&mut Self>) {
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
    fn publish_inplace(mut self: Pin<&mut Self>) {
        // Только нужное из настроек: полный клон `Settings` на каждую публикацию не нужен.
        let (regions, inplace) = {
            let settings = self.rust().shared.settings.borrow();
            (settings.capture_regions(), settings.appearance.inplace.clone())
        };
        self.as_mut().rust_mut().region_inplace.retain(|id, _| regions.iter().any(|r| &r.id == id));
        let window = serde_json::from_str::<[f64; 4]>(&self.game_geometry().to_string()).ok();
        // Арк-указатели: клонируются только счётчики, не блоки и не картинки.
        let frames: Vec<(String, lipa_core::settings::NormRect, Arc<InplaceFrame>)> = match window {
            Some(_) => regions.iter().filter_map(|r| Some((r.id.clone(), r.rect?, self.rust().region_inplace.get(&r.id)?.clone()))).collect(),
            None => Vec::new(),
        };
        // Подложки пишутся на диск только при новой ревизии поля.
        let mut images = std::collections::HashMap::new();
        for (region, _, frame) in &frames {
            for b in &frame.blocks {
                let Some(img) = b.background.image.as_ref() else { continue };
                let key = (region.clone(), b.id);
                let url = match self.rust().inplace_images.get(&key) {
                    Some((rev, url)) if *rev == b.revision => url.clone(),
                    _ => match save_backdrop(region, b.id, b.revision, img) {
                        Some(url) => {
                            self.as_mut().rust_mut().inplace_images.insert(key.clone(), (b.revision, url.clone()));
                            url
                        }
                        None => {
                            tracing::warn!(target: "inplace.render", region = %region, block_id = b.id, "backdrop image could not be written");
                            continue;
                        }
                    },
                };
                images.insert(key, url);
            }
        }
        // Подложки исчезнувших полей больше не нужны.
        let gone: Vec<_> = self.rust().inplace_images.keys().filter(|k| !images.contains_key(*k)).cloned().collect();
        for key in gone {
            self.as_mut().rust_mut().inplace_images.remove(&key);
            let _ = std::fs::remove_file(backdrop_path(&key.0, key.1));
        }
        let placed = match window {
            Some(window) => {
                let inputs: Vec<RegionInput> = frames.iter().map(|(id, rect, frame)| RegionInput { id, rect: *rect, frame }).collect();
                let mut cache = std::mem::take(&mut self.as_mut().rust_mut().placement_cache);
                let placed = place_regions(&inputs, &lipa_core::capture::kwin::WindowGeometry::from(window), &inplace, &images, &crate::icon::QtMeasure, &mut cache);
                self.as_mut().rust_mut().placement_cache = cache;
                placed
            }
            None => Default::default(),
        };
        tracing::debug!(target: "inplace.render", geometry = window.is_some(), regions = frames.len(), placed = placed.placed.len(),
            dropped = placed.dropped.len(), degraded = placed.placed.iter().filter(|p| p.degraded.is_some()).count(), "in-place fields published");
        let fallback = fallback_json(&placed, &regions);
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
    fn publish_faults(mut self: Pin<&mut Self>) {
        if self.rust().faults.is_empty() { return; }
        let text = self.rust().faults.values().map(|(s, _)| s.as_str()).collect::<Vec<_>>().join(" · ");
        let kind = if self.rust().faults.values().any(|(_, terminal)| *terminal) { "error" } else { "warning" };
        self.as_mut().set_status(QString::from(text.as_str()));
        self.as_mut().set_status_kind(QString::from(kind));
    }
    fn publish_history(mut self: Pin<&mut Self>) {
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
    fn clear_history(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().history.entries.clear();
        self.as_mut().publish_history();
    }
    fn refresh_diagnostics(mut self: Pin<&mut Self>) {
        if *self.diagnostics_busy() { return; }
        self.as_mut().set_diagnostics_busy(true);
        let s = self.rust().shared.settings.borrow().clone();
        let qt = self.qt_thread();
        spawn_service(async move {
            let result = lipa_core::diagnostics::inspect(&s).await;
            let json = serde_json::to_string(&result).unwrap();
            let _ = qt.queue(move |mut o| {
                o.as_mut().set_diagnostics_json(QString::from(json.as_str()));
                o.as_mut().set_diagnostics_busy(false);
            });
        });
    }

    fn settings_json(&self) -> QString {
        let s = self.rust().shared.settings.borrow().clone();
        QString::from(serde_json::to_string(&s).unwrap_or_default().as_str())
    }

    fn apply_settings(mut self: Pin<&mut Self>, json: &QString) {
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

    fn apply_settings_patch(mut self: Pin<&mut Self>, json: &QString) {
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
                self.as_mut().publish_history();
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

    fn pick_window(self: Pin<&mut Self>) {
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

    fn forget_game_profile(mut self: Pin<&mut Self>, key: &QString) {
        let key = key.to_string();
        tracing::info!(target: "settings", game = %key, "профиль игры удалён");
        // The selected game is saved again by the same update; the settings form disables its button.
        self.rust().shared.update(|s| { s.game_profiles.remove(&key); });
        self.as_mut().publish_settings();
    }

    /// Снимок окна для выбора области: показывается в нашем окне, поэтому
    /// не требует рисовать поверх чужого окна (чего Wayland не позволяет).
    fn request_preview(self: Pin<&mut Self>) {
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
                let dir = dirs::runtime_dir()
                    .unwrap_or_else(std::env::temp_dir)
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

    fn set_region(mut self: Pin<&mut Self>, x: f64, y: f64, w: f64, h: f64) {
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

    fn reset_region(mut self: Pin<&mut Self>) {
        self.rust().shared.update(|s| {
            s.capture.region = None;
            if let Some(r) = s.capture.regions.iter_mut().find(|r| r.id == s.capture.active_region) { r.rect = None; }
        });
        self.as_mut().publish_settings();
        self.as_mut().publish_translation();
        self.as_mut().set_status(QString::from("Область захвата сброшена"));
    }

    fn start(mut self: Pin<&mut Self>) {
        tracing::info!("Слежение за областями запущено");
        let _ = self.rust().shared.running.send(true);
        self.as_mut().set_running(true);
        self.as_mut().set_status(QString::from("Ожидание текста"));
        self.as_mut().set_status_kind(QString::from("info"));
    }

    fn stop(mut self: Pin<&mut Self>) {
        tracing::info!("Слежение за областями остановлено");
        let _ = self.rust().shared.running.send(false);
        self.as_mut().set_running(false);
        self.as_mut().set_status(QString::from("Остановлено"));
    }

    fn start_auto_translate(mut self: Pin<&mut Self>) -> bool {
        if *self.running() { return true; }
        if !*self.has_region() {
            self.as_mut().set_status(QString::from("Сначала выберите область"));
            return false;
        }
        self.as_mut().start();
        true
    }

    fn stop_auto_translate(mut self: Pin<&mut Self>) {
        if *self.running() { self.as_mut().stop(); }
    }

    fn activate_window(&self, object_name: &QString, token: &QString) { crate::icon::activate_window(object_name, token); }

    fn set_ocr_preview_enabled(mut self: Pin<&mut Self>, enabled: bool) {
        use std::sync::atomic::Ordering;
        if self.rust().shared.preview.swap(enabled, Ordering::Relaxed) == enabled { return; }
        tracing::debug!(target: "ocr.preview", enabled, "OCR preview window");
        if enabled {
            // A fresh frame at once instead of waiting for the next change on screen.
            if *self.has_region() { let _ = self.rust().shared.cmds.send(Cmd::TranslateOnce); }
        } else {
            self.rust().shared.preview_entries.lock().unwrap().clear();
            remove_preview_files();
            self.as_mut().set_ocr_preview_json(QString::from("[]"));
        }
    }

    fn translate_once(mut self: Pin<&mut Self>) {
        tracing::info!("Запрошен ручной перевод / повтор");
        self.as_mut().rust_mut().faults.clear();
        self.as_mut().set_status_kind(QString::from("info"));
        self.as_mut().set_status(QString::from("Распознавание окна…"));
        let _ = self.rust().shared.cmds.send(Cmd::TranslateOnce);
    }

    fn bundled_fonts(&self) -> QString {
        let db = lipa_core::layout::font_database::InstalledFontDatabase::bundled();
        let names: Vec<&str> = db.fonts().iter().map(|f| f.family.as_str()).collect();
        QString::from(serde_json::to_string(&names).unwrap_or_else(|_| "[]".into()).as_str())
    }

    fn auto_tune_filters(mut self: Pin<&mut Self>) {
        tracing::info!(component = "ocr", "Запрошен автоподбор фильтров");
        self.as_mut().set_status(QString::from("Подбор фильтров…"));
        let _ = self.rust().shared.cmds.send(Cmd::AutoTune);
    }

    fn reanalyze_fonts(mut self: Pin<&mut Self>) {
        tracing::info!(component = "fonts", "Запрошено повторное определение шрифтов");
        lipa_core::layout::font_database::InstalledFontDatabase::reload();
        let _ = self.rust().shared.cmds.send(Cmd::ReanalyzeFonts);
        self.as_mut().set_status(QString::from("Шрифты полей будут определены заново"));
    }
}

fn backdrop_path(region_id: &str, block: u64) -> std::path::PathBuf {
    let name: String = region_id.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-').collect();
    dirs::runtime_dir().unwrap_or_else(std::env::temp_dir).join("lipa").join(format!("inplace-{}-{name}-{block}.png", std::process::id()))
}

/// Размытая подложка поля рядом с превью окна; ревизия в URL заставляет QML перечитать файл.
fn save_backdrop(region_id: &str, block: u64, revision: u64, img: &image::RgbaImage) -> Option<String> {
    let path = backdrop_path(region_id, block);
    std::fs::create_dir_all(path.parent()?).inspect_err(|e| tracing::error!(component = "inplace", error = %e, "Не удалось создать каталог подложек")).ok()?;
    img.save(&path).inspect_err(|e| tracing::error!(component = "inplace", error = %e, "Не удалось сохранить подложку")).ok()?;
    Some(format!("file://{}?r={revision}", path.display()))
}

fn preview_dir() -> std::path::PathBuf { dirs::runtime_dir().unwrap_or_else(std::env::temp_dir).join("lipa") }

fn preview_prefix() -> String { format!("ocr-{}-", std::process::id()) }

fn remove_preview_files() {
    let Ok(entries) = std::fs::read_dir(preview_dir()) else { return };
    let prefix = preview_prefix();
    for entry in entries.flatten().filter(|e| e.file_name().to_string_lossy().starts_with(&prefix)) {
        let _ = std::fs::remove_file(entry.path());
    }
}

/// Запись области для окна просмотра: кадр лежит в PNG рядом с превью окна, блоки — в JSON.
fn preview_entry(region_id: &str, region_name: &str, preview: &lipa_core::pipeline::OcrPreview, url: &str, filtered_url: Option<&str>) -> serde_json::Value {
    serde_json::json!({
        "id": region_id,
        "name": region_name,
        "width": preview.image.width(),
        "height": preview.image.height(),
        "image": url,
        "filtered_image": filtered_url,
        "minimum_confidence": preview.minimum_confidence,
        "filters_auto": preview.filters_auto,
        "filters": { "binarize": preview.filters.binarize, "auto_invert": preview.filters.auto_invert, "sharpen": preview.filters.sharpen, "contrast": preview.filters.contrast, "filter_noise": preview.filter_noise },
        "original": preview.original,
        "translation": preview.translation,
        "phase": preview.phase.label(),
        "timings": preview.timings.iter().map(|t| serde_json::json!({
            "stage": t.stage, "last_ms": t.last_ms, "p50_ms": t.p50_ms, "p95_ms": t.p95_ms, "samples": t.samples,
        })).collect::<Vec<_>>(),
        "boxes": preview.boxes.iter().map(|b| serde_json::json!({
            "x": b.rect.x, "y": b.rect.y, "w": b.rect.w, "h": b.rect.h, "original": b.original, "translation": b.translation, "details": b.details, "block": b.block, "confidence": b.confidence,
        })).collect::<Vec<_>>(),
    })
}

/// Сохранить кадр и обновить JSON всех областей. `None` — окно просмотра уже закрыто или кадр не записан.
fn publish_preview(shared: &Shared, region_id: String, region_name: String, preview: lipa_core::pipeline::OcrPreview) -> Option<String> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static REVISION: AtomicU64 = AtomicU64::new(0);
    if !shared.preview.load(Ordering::Relaxed) { return None; }
    let name: String = region_id.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-').collect();
    let path = preview_dir().join(format!("{}{name}.png", preview_prefix()));
    std::fs::create_dir_all(path.parent()?).ok()?;
    preview.image.save(&path).inspect_err(|e| tracing::warn!(target: "ocr.preview", error = %e, "preview frame could not be written")).ok()?;
    let revision = REVISION.fetch_add(1, Ordering::Relaxed);
    let url = format!("file://{}?r={revision}", path.display());
    // `_` is not allowed in the region part of the name, so this cannot be another region's file.
    let filtered_path = path.with_file_name(format!("{}{name}_filtered.png", preview_prefix()));
    let filtered_url = match &preview.filtered {
        Some(img) => img.save(&filtered_path).inspect_err(|e| tracing::warn!(target: "ocr.preview", error = %e, "filtered preview frame could not be written")).ok()
            .map(|()| format!("file://{}?r={revision}", filtered_path.display())),
        None => { let _ = std::fs::remove_file(&filtered_path); None }
    };
    let active: Vec<String> = shared.settings.borrow().capture_regions().into_iter().map(|r| r.id).collect();
    let mut entries = shared.preview_entries.lock().unwrap();
    // The window may have been closed while the frame was being encoded.
    if !shared.preview.load(Ordering::Relaxed) { return None; }
    entries.insert(region_id.clone(), preview_entry(&region_id, &region_name, &preview, &url, filtered_url.as_deref()));
    entries.retain(|id, _| active.contains(id));
    // Regions keep the order of the settings.
    let ordered: Vec<&serde_json::Value> = active.iter().filter_map(|id| entries.get(id)).collect();
    serde_json::to_string(&ordered).ok()
}

/// Положение свободного окна сохраняется после перемещения — не во время него.
fn store_floating_geometry(shared: &Shared, g: lipa_core::settings::FloatingGeometry, reason: &str) {
    tracing::debug!(target: "translation_window.geometry", reason, x = g.x, y = g.y, width = g.w, height = g.h, screen = %g.output, "floating geometry");
    shared.update(|s| s.translation_window.floating_geometry = Some(g));
}

/// JSON для скрипта KWin: где поставить свободное окно при появлении. Сначала — где его
/// оставили, иначе — там, где стоит закреплённое; `null` — пусть место выберет KWin.
fn floating_placement(s: &Settings) -> String {
    match &s.translation_window.floating_geometry {
        Some(g) => {
            let (rx, ry) = g.relative();
            serde_json::json!({ "x": g.x, "y": g.y, "output": g.output, "rx": rx, "ry": ry }).to_string()
        }
        None if !s.translation_window.screen.is_empty() => serde_json::json!({ "output": s.translation_window.screen, "rx": s.translation_window.position.0, "ry": s.translation_window.position.1 }).to_string(),
        None => "null".into(),
    }
}

/// Какой рендер перевода работает: выбранный в настройках, кроме случая, когда для
/// «поверх оригинала» нет положения окна на экране (захват через portal).
fn inplace_availability(s: &Settings) -> lipa_core::capture::FeatureAvailability {
    use lipa_core::capture::FeatureAvailability;
    let Some(window) = s.capture.window.as_ref() else {
        return FeatureAvailability::unavailable("Окно захвата ещё не выбрано.", "Выберите окно в разделе «Источник изображения».");
    };
    if let Some(reason) = capabilities_for(window, s).inplace_blocker() {
        return FeatureAvailability::unavailable(format!("{}: {reason}", backend_name(window)),
            "Выберите захват KWin либо укажите экран и подтвердите, что portal захватывает полноэкранное окно на этом экране.");
    }
    let db = lipa_core::layout::font_database::InstalledFontDatabase::bundled();
    let regions = s.capture_regions();
    let langs: Vec<&str> = if regions.is_empty() { vec![s.translation.target_language.as_str()] }
        else { regions.iter().map(|r| if r.target_language.is_empty() { s.translation.target_language.as_str() } else { r.target_language.as_str() }).collect() };
    if langs.into_iter().any(|lang| !db.fonts().iter().any(|font| font.covers(lipa_core::layout::Script::from_lang(lang).fontconfig_lang()))) {
        return FeatureAvailability::unavailable("Нет встроенного шрифта для языка перевода.", "Выберите поддерживаемый язык перевода или отдельное окно.");
    }
    FeatureAvailability::available()
}

fn effective_display(s: &Settings) -> (&'static str, String) {
    if s.display_mode == lipa_core::settings::TranslationDisplayMode::Window { return ("window", String::new()); }
    let availability = inplace_availability(s);
    if availability.available { ("inplace", String::new()) } else { ("window", availability.reason) }
}

/// What did not go over the game as planned, for the note in the main window. Nothing here is shown in another window.
fn fallback_json(outcome: &lipa_core::layout::place::PlacementOutcome, regions: &[lipa_core::settings::CaptureRegion]) -> String {
    let name = |id: &str| regions.iter().find(|r| r.id == id).map_or_else(|| id.to_owned(), |r| r.name.clone());
    let texts: Vec<serde_json::Value> = outcome.dropped.iter().map(|f| serde_json::json!({ "region": name(&f.region_id), "text": f.text, "reason": f.reason })).collect();
    serde_json::json!({ "dropped": texts, "degraded": outcome.placed.iter().filter(|p| p.degraded.is_some()).count() }).to_string()
}

/// Возможности способа захвата выбранного окна для QML; без окна — как у KWin (ограничений нет).
fn capabilities_json(s: &Settings) -> String {
    let caps = s.capture.window.as_ref().map(|w| capabilities_for(w, s)).unwrap_or(lipa_core::capture::CaptureCapabilities::KWIN);
    let mut value = serde_json::to_value(caps).unwrap_or_default();
    let inplace = inplace_availability(s);
    value["inplaceBlocker"] = inplace.reason.clone().into();
    value["inplaceTranslation"] = serde_json::to_value(inplace).unwrap();
    let frames = if s.capture.window.is_none() {
        lipa_core::capture::FeatureAvailability::unavailable("Окно захвата ещё не выбрано.", "Выберите окно в разделе «Источник изображения».")
    } else if let Some(reason) = caps.frame_blocker() {
        lipa_core::capture::FeatureAvailability::unavailable(reason, "Выберите захват KWin или настройте полноэкранный portal.")
    } else { lipa_core::capture::FeatureAvailability::available() };
    value["frameBlocker"] = frames.reason.clone().into();
    value["captureRegionFrame"] = serde_json::to_value(frames).unwrap();
    value.to_string()
}

/// Снимок Tesseract в JSON; тяжёлые вызовы (tesseract, pacman/dpkg) выполняются вне GUI-потока.
fn tesseract_snapshot() -> TesseractInfo {
    TesseractManager::system().detect()
}

impl qobject::Controller {
    fn app_version(&self) -> QString { QString::from(env!("CARGO_PKG_VERSION")) }

    fn refresh_paddle(mut self: Pin<&mut Self>) {
        if *self.paddle_busy() { return; }
        self.as_mut().set_paddle_busy(true);
        let (python, language) = { let s = self.rust().shared.settings.borrow(); (s.recognition.paddle_python.clone(), s.recognition.language.clone()) };
        let python = if python.trim().is_empty() { "python3".to_owned() } else { python.trim().to_owned() };
        let qt = self.qt_thread();
        spawn_service(async move {
            let env = lipa_core::ocr::paddle_env::inspect(&python, &language).await;
            tracing::debug!(component = "paddleocr", ready = env.ready, summary = %env.summary, "PaddleOCR environment");
            let json = serde_json::to_string(&env).unwrap_or_default();
            let _ = qt.queue(move |mut o| {
                o.as_mut().set_paddle_json(QString::from(json.as_str()));
                o.as_mut().set_paddle_busy(false);
            });
        });
    }

    fn refresh_tesseract(mut self: Pin<&mut Self>) {
        if *self.tesseract_busy() {
            return;
        }
        self.as_mut().set_tesseract_busy(true);
        let qt = self.qt_thread();
        spawn_service(async move {
            let info = tokio::task::spawn_blocking(tesseract_snapshot).await;
            let _ = qt.queue(move |mut o| {
                if let Ok(info) = info {
                    o.as_mut().set_tesseract_json(QString::from(
                        serde_json::to_string(&info).unwrap_or_default().as_str(),
                    ));
                }
                o.as_mut().set_tesseract_busy(false);
            });
        });
    }

    fn install_package(mut self: Pin<&mut Self>, package: &QString) {
        let package = package.to_string();
        let manager = TesseractManager::system();
        // В командную строку попадают только пакеты Tesseract этого дистрибутива.
        let argv = match manager
            .is_allowed_package(&package)
            .then(|| manager.install_argv(&package))
            .flatten()
        {
            Some(a) => a,
            None => {
                self.as_mut()
                    .set_status(QString::from("Установка этого пакета не поддерживается"));
                return;
            }
        };
        if *self.tesseract_busy() {
            return;
        }
        self.as_mut().set_tesseract_busy(true);
        self.as_mut().set_status(QString::from(
            format!("Установка {package}: подтвердите пароль в системном окне").as_str(),
        ));
        let qt = self.qt_thread();
        spawn_service(async move {
            let res = tokio::task::spawn_blocking(move || {
                std::process::Command::new(&argv[0])
                    .args(&argv[1..])
                    .output()
                    .map_err(|e| e.to_string())
                    .and_then(|o| {
                        if o.status.success() {
                            Ok(())
                        } else {
                            let err = String::from_utf8_lossy(&o.stderr).trim().to_string();
                            // Код 126/127 от pkexec — окно закрыто или нет прав.
                            Err(if err.is_empty() {
                                format!("команда завершилась с кодом {:?}", o.status.code())
                            } else {
                                err
                            })
                        }
                    })
            })
            .await
            .unwrap_or_else(|e| Err(e.to_string()));
            if let Err(e) = &res {
                tracing::error!(component = "packages", %package, error = %e, "Установка языкового пакета не удалась");
            }
            let info = if res.is_ok() {
                tokio::task::spawn_blocking(tesseract_snapshot).await.ok()
            } else {
                None
            };
            let _ = qt.queue(move |mut o| {
                match &res {
                    Ok(()) => o.as_mut().set_status(QString::from(
                        format!("Пакет {package} установлен").as_str(),
                    )),
                    Err(e) => {
                        o.as_mut().set_status(QString::from(format!("Не удалось установить {package}: {e}").as_str()));
                    },
                }
                // Список языков обновляется автоматически после успешной установки.
                if let Some(info) = info {
                    o.as_mut().set_tesseract_json(QString::from(
                        serde_json::to_string(&info).unwrap_or_default().as_str(),
                    ));
                }
                o.as_mut().set_tesseract_busy(false);
            });
        });
    }

    fn install_language(mut self: Pin<&mut Self>, code: &QString) {
        let code = code.to_string();
        if *self.tesseract_busy() { return; }
        self.as_mut().set_tesseract_busy(true);
        self.as_mut().set_status(QString::from(format!("Загрузка языковой модели {code}…").as_str()));
        let qt = self.qt_thread();
        spawn_service(async move {
            let res = lipa_core::tesseract::download_language(&code).await;
            match &res {
                Ok(path) => tracing::info!(component = "tesseract", %code, path = %path.display(), "Языковая модель скачана"),
                Err(e) => tracing::error!(component = "tesseract", %code, error = %e, "Не удалось скачать языковую модель"),
            }
            // The list of languages is read again either way: it is what the user sees.
            let info = tokio::task::spawn_blocking(tesseract_snapshot).await.ok();
            let _ = qt.queue(move |mut o| {
                let status = match &res {
                    Ok(_) => format!("Языковая модель {code} скачана"),
                    Err(e) => format!("Не удалось скачать языковую модель {code}: {e}"),
                };
                o.as_mut().set_status(QString::from(status.as_str()));
                o.as_mut().set_status_kind(QString::from(if res.is_ok() { "info" } else { "error" }));
                if let Some(info) = info {
                    o.as_mut().set_tesseract_json(QString::from(serde_json::to_string(&info).unwrap_or_default().as_str()));
                }
                o.as_mut().set_tesseract_busy(false);
            });
        });
    }

    fn missing_languages(&self, spec: &QString) -> QString {
        let info: Result<TesseractInfo, _> =
            serde_json::from_str(&self.tesseract_json().to_string());
        let list = match info {
            Ok(i) if i.installed => {
                TesseractManager::system().missing(&spec.to_string(), &i.languages)
            }
            _ => vec![],
        };
        QString::from(serde_json::to_string(&list).unwrap_or_default().as_str())
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

#[cfg(test)]
mod display_tests {
    use super::*;
    use lipa_core::settings::{FloatingGeometry, TranslationDisplayMode, WindowKey};

    #[test]
    fn appearance_patch_preserves_live_capture_settings() {
        let mut current = Settings::default();
        current.capture.window = Some(WindowKey { uuid: "game-now".into(), resource_class: "Game".into(), caption: "Game".into() });
        current.capture.source = CaptureSource::Portal;
        let before = current.processing_key();
        let patch = serde_json::json!({ "appearance.window.font_size": 30, "translation_window.pinned_corner_radius": 18 });
        let updated = merge_settings_patch(&current, patch.as_object().unwrap()).unwrap();
        assert_eq!(updated.capture.window, current.capture.window);
        assert_eq!(updated.capture.source, CaptureSource::Portal);
        assert_eq!(updated.processing_key(), before, "appearance must not reset the capture pipeline");
        assert_eq!((updated.appearance.window.font_size, updated.translation_window.pinned_corner_radius), (30, 18));
        // Anything the backend manages is refused, found by the declaration and not by a list kept here.
        for key in ["translation_window.floating_geometry", "schema_version", "capture.portal_token"] {
            let managed = serde_json::json!({ key: null });
            assert!(merge_settings_patch(&current, managed.as_object().unwrap()).is_err(), "{key}");
        }
        let forbidden = serde_json::json!({ "capture.window": null });
        assert!(merge_settings_patch(&current, forbidden.as_object().unwrap()).is_err());
        // Game profiles are written by the backend only; a stale form must not overwrite them.
        let stale = serde_json::json!({ "game_profiles": {} });
        assert!(merge_settings_patch(&current, stale.as_object().unwrap()).is_err());
        let switch = serde_json::json!({ "game_profiles_enabled": false });
        assert!(!merge_settings_patch(&current, switch.as_object().unwrap()).unwrap().game_profiles_enabled);
        current.appearance.inplace.shadow = true;
        let patch = serde_json::json!({ "appearance.inplace.fill_opacity": 0.4 });
        let updated = merge_settings_patch(&current, patch.as_object().unwrap()).unwrap();
        assert!(updated.appearance.inplace.shadow, "a delayed edit keeps a newly changed sibling");
        assert_eq!(updated.appearance.inplace.fill_opacity, 0.4);
        for patch in [serde_json::json!({"capture": {"window": null}}), serde_json::json!({"appearance.inplace": {}})] {
            assert!(merge_settings_patch(&current, patch.as_object().unwrap()).is_err(), "only registered leaves may be patched");
        }
    }

    #[test]
    fn the_ui_gets_the_backend_capabilities_and_the_reasons() {
        let mut s = Settings::default();
        let caps = |s: &Settings| serde_json::from_str::<serde_json::Value>(&capabilities_json(s)).unwrap();
        let none = caps(&s);
        assert_eq!(none["inplaceTranslation"]["available"], false, "select a capture window first");
        s.capture.window = Some(WindowKey { uuid: "portal:window".into(), resource_class: String::new(), caption: String::new() });
        let portal = caps(&s);
        assert_eq!((portal["window_geometry"].as_bool(), portal["inplace_translation"].as_bool()), (Some(false), Some(false)));
        assert!(portal["inplaceBlocker"].as_str().unwrap().contains("глобальные координаты"));
        assert!(portal["frameBlocker"].as_str().unwrap().contains("рамка"));
        s.capture.portal_fills_monitor = true;
        s.translation_window.screen = "DP-1".into();
        let fullscreen = caps(&s);
        assert_eq!((fullscreen["window_geometry"].as_bool(), fullscreen["inplace_translation"].as_bool()), (Some(true), Some(true)));
        assert_eq!((fullscreen["inplaceBlocker"].as_str(), fullscreen["frameBlocker"].as_str()), (Some(""), Some("")));
    }

    #[test]
    fn fields_left_out_are_reported_for_the_note_only() {
        use lipa_core::layout::place::{DroppedText, PlacementOutcome};
        let regions = vec![lipa_core::settings::CaptureRegion { id: "dialogue".into(), name: "Диалоги".into(), ..Default::default() }];
        let none = serde_json::from_str::<serde_json::Value>(&fallback_json(&PlacementOutcome::default(), &regions)).unwrap();
        assert_eq!((none["dropped"].as_array().map(Vec::len), none["degraded"].as_u64()), (Some(0), Some(0)), "nothing to report: the note stays hidden");
        assert!(none.get("texts").is_none(), "no texts for another window");
        let outcome = PlacementOutcome { placed: Vec::new(), dropped: vec![DroppedText { region_id: "dialogue".into(), block_id: 3, text: "Привет".into(), original: "Hello".into(), reason: "для перевода нет места поверх игры" }] };
        let shown = serde_json::from_str::<serde_json::Value>(&fallback_json(&outcome, &regions)).unwrap();
        assert_eq!((shown["dropped"][0]["region"].as_str(), shown["dropped"][0]["text"].as_str()), (Some("Диалоги"), Some("Привет")));
    }

    #[test]
    fn inplace_does_not_depend_on_the_translation_window() {
        let mut s = Settings { display_mode: TranslationDisplayMode::Inplace, ..Default::default() };
        s.capture.window = Some(WindowKey { uuid: "kwin-game".into(), resource_class: "game".into(), caption: "Game".into() });
        assert_eq!(effective_display(&s), ("inplace", String::new()));
        // Portal capture has no window position: logged fallback to the translation window.
        s.capture.window = Some(WindowKey { uuid: "portal:window".into(), resource_class: String::new(), caption: String::new() });
        let (mode, note) = effective_display(&s);
        assert_eq!(mode, "window");
        assert!(note.contains("portal"));
        s.capture.portal_fills_monitor = true;
        s.translation_window.screen = "DP-1".into();
        assert_eq!(effective_display(&s), ("inplace", String::new()));
        s.display_mode = TranslationDisplayMode::Window;
        assert_eq!(effective_display(&s), ("window", String::new()));
    }

    #[test]
    fn floating_placement_restores_where_the_user_left_it() {
        let mut s = Settings::default();
        assert_eq!(floating_placement(&s), "null", "first start: KWin chooses");
        s.translation_window.screen = "DP-1".into();
        s.translation_window.position = (40, 50);
        let p: serde_json::Value = serde_json::from_str(&floating_placement(&s)).unwrap();
        assert_eq!((p["output"].as_str(), p["rx"].as_i64(), p["ry"].as_i64()), (Some("DP-1"), Some(40), Some(50)), "where the pinned one was");
        s.translation_window.floating_geometry = Some(FloatingGeometry { x: 2140.0, y: 382.0, w: 740.0, h: 240.0, output: "DP-2".into(), output_x: 2560.0, output_y: -200.0 });
        let p: serde_json::Value = serde_json::from_str(&floating_placement(&s)).unwrap();
        assert_eq!((p["x"].as_f64(), p["y"].as_f64(), p["output"].as_str()), (Some(2140.0), Some(382.0), Some("DP-2")));
        assert_eq!((p["rx"].as_i64(), p["ry"].as_i64()), (Some(-420), Some(582)), "negative layout offsets are kept");
    }
}
