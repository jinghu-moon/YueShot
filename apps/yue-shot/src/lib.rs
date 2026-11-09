#![cfg(windows)]
use gpui_kit::base::{Root, StyledExt as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::{path::PathBuf, sync::Arc};
use yue_component::{YueButton, YueSegmented, YueSegmentedOption};
use yue_shot_storage::capture_history::CaptureHistoryStore;
mod autostart;
pub mod capture;
mod capture_host;
mod commands;
mod shortcut_editor;
pub mod shortcuts;
use commands::Command;
mod clipboard;
mod diagnostics;
mod history_source;
mod history_view;
mod instance;
mod logging;
mod maintenance;
mod pin;
mod pin_source;
mod product;
pub mod save;
mod save_panel;
pub mod settings;
mod startup;
const WINDOW_WIDTH: f32 = 560.0;
const WINDOW_HEIGHT: f32 = 560.0;
#[derive(Clone, Copy)]
enum Page {
    History,
    Settings,
}
struct ProductOwner(Entity<YueShotApp>);
impl Global for ProductOwner {}
/// Services belong to the process, independently of every GPUI window's Root/view.
struct YueShotApp {
    data: PathBuf,
    capture: Arc<yue_shot_capture::CaptureRuntime>,
    pins: Option<Entity<yue_pin::PinManager>>,
    pin_source: Arc<pin_source::PinSource>,
    settings: Entity<settings::SettingsView>,
    save_panels: save_panel::PanelWindows,
    history: Option<CaptureHistoryStore>,
    views: Vec<WeakEntity<history_view::CaptureHistoryView>>,
    main: Option<(WindowHandle<Root>, WeakEntity<MainWindow>)>,
    tray: Option<yue_shell::Tray>,
    tray_options: yue_shell::TrayOptions,
    tray_events: async_channel::Sender<yue_shell::TrayEvent>,
    tray_status: String,
    capture_status: Option<String>,
    capture_events: capture_host::Events,
    instance: Option<instance::Instance>,
    startup_status: Vec<String>,
    status: Vec<String>,
    tasks: Vec<Task<()>>,
    _settings_sync: Subscription,
}
impl YueShotApp {
    fn new(paths: startup::ProductPaths, cx: &mut Context<Self>) -> Self {
        let data = paths.root.clone();
        let mut startup_status = Vec::new();
        let history = match CaptureHistoryStore::open(&data) {
            Ok(store) => Some(store),
            Err(error) => {
                startup_status.push(format!("截图历史不可用：{error}"));
                None
            }
        };
        let login_startup = autostart::Registration::current(&paths)
            .map(|registration| Arc::new(registration) as Arc<dyn autostart::LoginStartup>);
        let settings = cx.new(|_| {
            settings::SettingsView::new(settings::SettingsStore::new(&data))
                .with_login_startup(login_startup)
        });
        let initial = settings.read(cx).settings().clone();
        let (refresh_sender, refresh_receiver) = async_channel::unbounded();
        let (save_sender, save_receiver) = async_channel::unbounded();
        let (pin_sender, pin_receiver) = async_channel::unbounded();
        let (failed_sender, failed_receiver) = async_channel::bounded(8);
        let capture_events = capture_host::Events::new(
            refresh_sender.clone(),
            save_sender,
            pin_sender,
            failed_sender,
        );
        let mut capture_status = None;
        let capture = capture_host::start(
            &data,
            history.clone(),
            capture_events.clone(),
            initial.detection_options(),
            initial.overlay_preferences(),
        )
        .unwrap_or_else(|error| {
            capture_status = Some(format!("截图服务不可用：{error}"));
            Arc::new(yue_shot_capture::CaptureRuntime::new(Box::new(
                capture_host::UnavailableOverlay,
            )))
        });
        let source = Arc::new(pin_source::PinSource::new());
        source.set_home(capture.clone());
        let pins = match yue_pin::PinManager::new() {
            Ok((manager, commands)) => {
                let owner = cx.new(|_| manager);
                pin::drain_copies(&owner, commands, cx);
                Some(owner)
            }
            Err(error) => {
                startup_status.push(format!("贴图服务不可用：{error}"));
                None
            }
        };
        let _settings_sync = cx.subscribe(
            &settings,
            |app: &mut Self, _, event: &settings::SettingsChanged, _| {
                app.capture.set_preferences(event.0.overlay_preferences())
            },
        );
        let mut tasks = Vec::new();
        tasks.push(cx.spawn(async move |app, cx| {
            while pin_receiver.recv().await.is_ok() {
                if app.update(cx, |app, cx| app.pin((0, 0), cx)).is_err() {
                    break;
                }
            }
        }));
        tasks.push(cx.spawn(async move |app, cx| {
            while refresh_receiver.recv().await.is_ok() {
                if app
                    .update(cx, |app, cx| {
                        if let Some(preferences) = app.capture.preferences() {
                            app.settings
                                .update(cx, |view, cx| view.adopt_label(preferences.label, cx));
                        }
                        app.views.retain(|view| view.upgrade().is_some());
                        for view in &app.views {
                            let _ = view.update(cx, |view, cx| view.reload_history(cx));
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        }));
        let archive_history = history.clone();
        tasks.push(cx.spawn(async move |app, cx| {
            while let Ok(request) = save_receiver.recv().await {
                let prepared = match request {
                    capture_host::SaveRequest::Selection(request) => {
                        let picture = save_panel::Picture::from_shared_pixels(
                            request.selection.pixels.clone(),
                        );
                        let [width, height] = request.selection.pixels.size();
                        let metadata = yue_shot_model::CaptureMetadata {
                            session_id: request.session_id,
                            width,
                            height,
                            dpi: 0,
                            source: request.selection.source(),
                            captured_at_unix_ms: request.selection.frame.captured_at_unix_ms,
                            monitor_device_name: None,
                        };
                        Ok((picture, metadata))
                    }
                    capture_host::SaveRequest::Bands {
                        session_id,
                        spec,
                        bands,
                        dpi,
                    } => {
                        let metadata = yue_shot_model::CaptureMetadata {
                            session_id,
                            width: spec.region.width() as u32,
                            height: spec.region.height() as u32,
                            dpi,
                            source: spec.source,
                            captured_at_unix_ms: spec.captured_at_unix_ms,
                            monitor_device_name: None,
                        };
                        save_panel::Picture::from_bands(spec, bands)
                            .map(|picture| (picture, metadata))
                    }
                };
                let (mut picture, metadata) = match prepared {
                    Ok(value) => value,
                    Err(error) => {
                        _ = app.update(cx, |app, cx| {
                            app.report(format!("保存面板无法读取截图：{error}"), cx)
                        });
                        continue;
                    }
                };
                if let Some(store) = &archive_history {
                    picture = picture.with_archive(
                        history_source::CaptureArchive::new(store.clone(), metadata)
                            .with_refresh(refresh_sender.clone()),
                    );
                }
                if app
                    .update(cx, |app, cx| {
                        let default_format = app.settings.read(cx).settings().save_format;
                        if let Err(error) = app.save_panels.open(picture, default_format, cx) {
                            app.report(format!("保存面板无法打开：{error}"), cx);
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        }));
        let (tray_sender, tray_receiver) = async_channel::unbounded();
        let options =
            yue_shell::TrayOptions::new(paths.instance_id, "YueShot", initial.shortcuts.menu())
                .with_hotkeys(initial.shortcuts.hotkeys().expect("validated settings"));
        let (tray, tray_status) = match yue_shell::Tray::start(options.clone(), tray_sender.clone())
        {
            Ok(tray) => {
                let status = Self::describe_tray(&tray.status(), &initial.shortcuts);
                (Some(tray), status)
            }
            Err(error) => (None, format!("托盘不可用：{error}；关闭主窗口将退出应用")),
        };
        tasks.push(cx.spawn(async move |app, cx| {
            while let Ok(event) = tray_receiver.recv().await {
                if app
                    .update(cx, |app, cx| {
                        let event = match event {
                            yue_shell::TrayEvent::Command(event) => event,
                            yue_shell::TrayEvent::Status(status) => {
                                let next = Self::describe_tray(
                                    &status,
                                    &app.settings.read(cx).settings().shortcuts,
                                );
                                let failed = !next.is_empty();
                                app.tray_status = next;
                                if failed {
                                    app.open_main(Page::History, cx);
                                } else {
                                    app.refresh_status(cx);
                                }
                                return;
                            }
                        };
                        if let Some(command) = Command::from_id(event.command()) {
                            app.dispatch(command, event.position(), cx);
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        }));
        let mut app = Self {
            data,
            capture,
            pins,
            pin_source: source,
            settings,
            save_panels: save_panel::PanelWindows::default(),
            history,
            views: Vec::new(),
            main: None,
            tray,
            tray_options: options,
            tray_events: tray_sender,
            tray_status,
            capture_status,
            capture_events,
            instance: None,
            startup_status,
            status: Vec::new(),
            tasks,
            _settings_sync,
        };
        app.receive_capture_failures(failed_receiver, cx);
        app
    }
    fn receive_capture_failures(
        &mut self,
        errors: async_channel::Receiver<String>,
        cx: &mut Context<Self>,
    ) {
        self.tasks.push(cx.spawn(async move |app, cx| {
            while let Ok(error) = errors.recv().await {
                if app.update(cx, |app, cx| app.report(error, cx)).is_err() {
                    break;
                }
            }
        }));
    }
    fn attach_instance(
        &mut self,
        owner: instance::Instance,
        commands: async_channel::Receiver<instance::Event>,
        cx: &mut Context<Self>,
    ) {
        self.instance = Some(owner);
        self.receive_instance_commands(commands, cx);
    }
    fn receive_instance_commands(
        &mut self,
        commands: async_channel::Receiver<instance::Event>,
        cx: &mut Context<Self>,
    ) {
        self.tasks.push(cx.spawn(async move |app, cx| {
            while let Ok(event) = commands.recv().await {
                if app
                    .update(cx, |app, cx| match event {
                        instance::Event::Command(command) => app.launch(command, cx),
                        instance::Event::Failed(error) => app.report(error, cx),
                    })
                    .is_err()
                {
                    break;
                }
            }
        }));
    }
    fn dispatch(&mut self, command: Command, position: (i32, i32), cx: &mut Context<Self>) {
        let started = std::time::Instant::now();
        match command {
            Command::Show => self.open_main(Page::History, cx),
            Command::Settings => self.open_main(Page::Settings, cx),
            Command::Capture => self.start_capture(cx),
            Command::Scroll => {
                if let Err(error) = self.capture.start_scroll() {
                    self.report(format!("长截图启动失败：{error}"), cx);
                }
            }
            Command::Pin => self.pin(position, cx),
            Command::ShowPins | Command::HidePins => {
                if let Some(pins) = &self.pins {
                    pins.update(cx, |pins, cx| {
                        pins.set_visible(command == Command::ShowPins, cx)
                    });
                }
            }
            Command::ClosePins => {
                if let Some(pins) = &self.pins {
                    pins.update(cx, |pins, cx| pins.close_all(cx));
                }
            }
            Command::UndoPin => {
                if let Some(pins) = &self.pins
                    && let Err(error) = yue_pin::PinManager::undo(pins, position, cx)
                {
                    self.report(error.to_string(), cx);
                }
            }
            Command::Quit => self.request_quit(cx),
        }
        log::info!(target:"yueshot.event", "command.{:?} elapsed_us={}", command, started.elapsed().as_micros());
    }
    fn launch(&mut self, command: instance::Command, cx: &mut Context<Self>) {
        match command {
            instance::Command::Show => self.dispatch(Command::Show, (0, 0), cx),
            instance::Command::Capture => self.dispatch(Command::Capture, (0, 0), cx),
            instance::Command::Background
                if self.tray.as_ref().is_some_and(|tray| tray.is_available())
                    && self.capture_status.is_none()
                    && self.tray_status.is_empty()
                    && self.startup_status.is_empty()
                    && self.status.is_empty() => {}
            instance::Command::Background => self.open_main(Page::History, cx),
        }
    }
    fn open_folder(&mut self, cx: &mut Context<Self>) {
        let directory = self.data.join("artifacts");
        let result = std::fs::create_dir_all(&directory).and_then(|_| {
            std::process::Command::new("explorer.exe")
                .arg(directory)
                .spawn()
                .map(|_| ())
        });
        if let Err(error) = result {
            self.report(format!("截图文件夹无法打开：{error}"), cx);
        }
    }
    fn pin(&mut self, position: (i32, i32), cx: &mut Context<Self>) {
        if let Some(pins) = &self.pins {
            pin::open_selection(&self.pin_source, pins, position, cx);
        } else {
            self.report("贴图服务不可用，请重启后重试".into(), cx);
        }
    }
    fn start_capture(&mut self, cx: &mut Context<Self>) {
        if let Err(error) = self.capture.start_capture() {
            self.report(format!("截图启动失败：{error}"), cx);
        }
    }
    fn report(&mut self, error: String, cx: &mut Context<Self>) {
        log::warn!(target:"product.operation_failed", "{error}");
        if !self.status.contains(&error) {
            if self.status.len() == 16 {
                self.status.remove(0);
            }
            self.status.push(error);
        }
        self.open_main(Page::History, cx);
    }
    fn describe_tray(status: &yue_shell::TrayStatus, keys: &shortcuts::Shortcuts) -> String {
        let mut errors = Vec::new();
        if !status.is_available() {
            errors.push(format!(
                "托盘不可用：{}；{}，关闭主窗口将退出应用",
                status.error().unwrap_or("图标尚未恢复"),
                if status.is_running() {
                    "正在重试"
                } else {
                    "可点击重试"
                }
            ));
        }
        for failure in status.hotkey_failures() {
            errors.push(shortcuts::failure_message(
                failure.command(),
                failure.error(),
                keys,
            ));
        }
        errors.join("；")
    }
    fn status_text(&self) -> String {
        self.startup_status
            .iter()
            .chain(&self.status)
            .map(String::as_str)
            .chain(self.capture_status.as_deref())
            .chain((!self.tray_status.is_empty()).then_some(self.tray_status.as_str()))
            .collect::<Vec<_>>()
            .join("；")
    }
    fn refresh_status(&self, cx: &mut Context<Self>) {
        if let Some((_, view)) = &self.main {
            let status = self.status_text();
            let retry_services = self.capture_status.is_some() || !self.tray_status.is_empty();
            let _ = view.update(cx, |view, cx| {
                view.status = status;
                view.retry_services = retry_services;
                cx.notify();
            });
        }
    }
    fn retry_services(&mut self, cx: &mut Context<Self>) {
        // An active capture is never torn down merely because its latest operation failed.
        if self.capture_status.is_some() {
            let settings = self.settings.read(cx).settings();
            match capture_host::start(
                &self.data,
                self.history.clone(),
                self.capture_events.clone(),
                settings.detection_options(),
                settings.overlay_preferences(),
            ) {
                Ok(capture) => {
                    self.capture.shutdown();
                    self.pin_source.set_home(capture.clone());
                    self.capture = capture;
                    self.capture_status = None;
                }
                Err(error) => self.capture_status = Some(format!("截图服务不可用：{error}")),
            }
        }
        if self.tray.as_ref().is_none_or(|tray| tray.retry().is_err()) {
            self.tray.take();
            match yue_shell::Tray::start(self.tray_options.clone(), self.tray_events.clone()) {
                Ok(tray) => {
                    self.tray_status = Self::describe_tray(
                        &tray.status(),
                        &self.settings.read(cx).settings().shortcuts,
                    );
                    self.tray = Some(tray);
                }
                Err(error) => {
                    self.tray_status = format!("托盘不可用：{error}；关闭主窗口将退出应用")
                }
            }
        }
        self.refresh_status(cx);
    }
    fn apply_shortcuts(
        &mut self,
        shortcuts: shortcuts::Shortcuts,
        editor: WeakEntity<shortcut_editor::ShortcutEditor>,
        cx: &mut Context<Self>,
    ) {
        let next = match self
            .settings
            .update(cx, |settings, _| settings.begin_shortcuts(shortcuts))
        {
            Ok(next) => next,
            Err(error) => {
                let _ = editor.update(cx, |editor, cx| editor.finish(Err(error), cx));
                return;
            }
        };
        let prepared = (|| {
            let tray = self.tray.as_ref().ok_or("托盘线程不可用，请先重试服务")?;
            let menu = next.shortcuts.menu();
            let hotkeys = next.shortcuts.hotkeys()?;
            let data = self.data.clone();
            let commit = next.clone();
            tray.configure(menu, hotkeys, move || {
                settings::SettingsStore::new(data).save(&commit)
            })
        })();
        let result = match prepared {
            Ok(result) => result,
            Err(error) => {
                self.settings
                    .update(cx, |settings, cx| settings.finish_shortcuts(None, cx));
                let _ = editor.update(cx, |editor, cx| editor.finish(Err(error), cx));
                return;
            }
        };
        self.tasks.push(cx.spawn(async move |app, cx| {
            let outcome = result
                .recv()
                .await
                .unwrap_or_else(|_| Err("快捷键服务已停止；请重启后核对设置".into()));
            let _ = app.update(cx, |app, cx| {
                if outcome.is_ok() {
                    app.tray_options = app
                        .tray_options
                        .clone()
                        .with_hotkeys(next.shortcuts.hotkeys().expect("validated settings"));
                    // Include the current labels if the service has to be recreated later.
                    app.tray_options = app.tray_options.clone().with_menu(next.shortcuts.menu());
                }
                app.settings.update(cx, |settings, cx| {
                    settings.finish_shortcuts(outcome.is_ok().then_some(next), cx)
                });
                let _ = editor.update(cx, |editor, cx| editor.finish(outcome, cx));
            });
        }));
    }
    fn close_main(&mut self, cx: &mut Context<Self>) {
        let Some((handle, view)) = &self.main else {
            return;
        };
        if view
            .upgrade()
            .is_some_and(|view| view.read(cx).history.read(cx).has_pending_operation())
        {
            self.report(
                "正在保存或复制历史图片，请等待操作完成后再关闭窗口".into(),
                cx,
            );
            return;
        }
        if self.tray.as_ref().is_some_and(|tray| tray.is_available())
            && self.settings.read(cx).settings().close_to_tray
        {
            let handle = *handle;
            let _ = handle.update(cx, |_, window, _| window.remove_window());
            self.main = None;
        } else {
            self.request_quit(cx);
        }
    }
    fn open_main(&mut self, page: Page, cx: &mut Context<Self>) {
        if matches!(page, Page::Settings) {
            self.settings
                .update(cx, |view, cx| view.refresh_login_startup(cx));
        }
        if let Some((handle, view)) = &self.main {
            let view = view.clone();
            let status = self.status_text();
            let retry_services = self.capture_status.is_some() || !self.tray_status.is_empty();
            if handle
                .update(cx, |_, window, cx| {
                    let _ = view.update(cx, |view, cx| {
                        view.page = page;
                        view.status = status;
                        view.retry_services = retry_services;
                        cx.notify();
                    });
                    window.activate_window();
                })
                .is_ok()
            {
                return;
            }
        }
        let app = cx.entity().downgrade();
        let settings = self.settings.clone();
        let history = self.history.clone();
        let pins = self.pins.as_ref().map(Entity::downgrade);
        let mut created = None;
        let mut history_view = None;
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None,
                size(px(WINDOW_WIDTH), px(WINDOW_HEIGHT)),
                cx,
            ))),
            window_min_size: Some(size(px(360.), px(360.))),
            titlebar: Some(TitlebarOptions {
                title: Some("YueShot".into()),
                ..Default::default()
            }),
            ..Default::default()
        };

        let close_owner = cx.entity().downgrade();
        let status = self.status_text();
        let retry_services = self.capture_status.is_some() || !self.tray_status.is_empty();
        match cx.open_window(options, |window, cx| {
            let shortcuts = settings.read(cx).settings().shortcuts.clone();
            let editor = cx.new(|cx| {
                shortcut_editor::ShortcutEditor::new(app.clone(), &shortcuts, window, cx)
            });
            settings.update(cx, |settings, _| settings.shortcut_editor = Some(editor));
            let history = cx.new(|cx| history_view::CaptureHistoryView::new(history, pins, cx));
            history_view = Some(history.downgrade());
            let view = cx.new(|_| MainWindow {
                app,
                settings,
                history,
                page,
                status,
                retry_services,
            });
            created = Some(view.downgrade());
            window.on_window_should_close(cx, move |_, cx| {
                // Defer all policy to the owner: read live tray health, never a window-creation snapshot.
                let owner = close_owner.clone();
                cx.defer(move |cx| {
                    let _ = owner.update(cx, |app, cx| app.close_main(cx));
                });
                false
            });
            cx.new(|cx| Root::new(view, window, cx))
        }) {
            Ok(handle) => {
                self.main = Some((handle, created.unwrap()));
                self.views.retain(|view| view.upgrade().is_some());
                self.views.push(history_view.unwrap());
            }
            Err(error) => eprintln!("[yue-shot] main window could not open: {error}"),
        }
    }
    fn request_quit(&mut self, cx: &mut Context<Self>) {
        let saving = self.settings.read(cx).shortcuts_busy()
            || self.save_panels.has_pending_operation(cx)
            || self.views.iter().any(|view| {
                view.upgrade()
                    .is_some_and(|view| view.read(cx).has_pending_operation())
            });
        if saving {
            self.report(
                "正在保存图片或设置，请等待操作完成后再退出 YueShot".into(),
                cx,
            );
        } else {
            cx.quit();
        }
    }
    fn shutdown(&mut self, cx: &mut Context<Self>) {
        log::info!(target:"yueshot.event","shutdown.begin");
        self.instance.take();
        self.tray.take();
        self.tasks.clear();
        self.capture.shutdown();
        if let Some(pins) = self.pins.take() {
            pins.update(cx, |pins, cx| pins.close_all(cx));
        }
        log::info!(target:"yueshot.event","shutdown.services_stopped");
    }
}
struct MainWindow {
    app: WeakEntity<YueShotApp>,
    settings: Entity<settings::SettingsView>,
    history: Entity<history_view::CaptureHistoryView>,
    page: Page,
    status: String,
    retry_services: bool,
}
impl Render for MainWindow {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = yue_component::current_theme(cx);
        let app = self.app.clone();
        let status = (!self.status.is_empty()).then(|| self.status.clone());
        div()
            .v_flex()
            .size_full()
            .gap_3()
            .p_4()
            .bg(gpui_kit::Hsla::from(gpui_kit::rgb(theme.page())))
            .text_color(gpui_kit::Hsla::from(gpui_kit::rgb(theme.text_primary())))
            .child(
                div()
                    .h_flex()
                    .flex_wrap()
                    .gap_2()
                    .child(YueButton::new("capture-region", "开始截图").on_click(
                        move |_, _, cx| {
                            let _ = app
                                .update(cx, |app, cx| app.dispatch(Command::Capture, (0, 0), cx));
                        },
                    ))
                    .child(
                        YueSegmented::new(
                            "main-page",
                            theme,
                            if matches!(self.page, Page::History) {
                                "history"
                            } else {
                                "settings"
                            },
                        )
                        .accessibility_label("YueShot 页面")
                        .options([
                            YueSegmentedOption::new("history", "截图历史"),
                            YueSegmentedOption::new("settings", "设置"),
                        ])
                        .on_change({
                            let view = cx.entity().downgrade();
                            move |next, _, cx| {
                                let _ = view.update(cx, |view, cx| {
                                    view.page = if next.as_ref() == "settings" {
                                        Page::Settings
                                    } else {
                                        Page::History
                                    };
                                    cx.notify();
                                });
                            }
                        }),
                    )
                    .child(YueButton::new("open-captures", "截图文件夹…").on_click({
                        let app = self.app.clone();
                        move |_, _, cx| {
                            let _ = app.update(cx, |app, cx| app.open_folder(cx));
                        }
                    }))
                    .child(YueButton::new("quit", "退出").on_click(|_, _, cx| {
                        if cx.has_global::<ProductOwner>() {
                            let owner = cx.global::<ProductOwner>().0.clone();
                            owner.update(cx, |app, cx| app.dispatch(Command::Quit, (0, 0), cx));
                        }
                    })),
            )
            .when_some(status, |view, status| {
                view.child(
                    div()
                        .v_flex()
                        .gap_2()
                        .child(div().text_sm().child(status))
                        .when(self.retry_services, |view| {
                            view.child(
                                YueButton::new("retry-services", "重试托盘、热键与截图服务")
                                    .on_click({
                                        let app = self.app.clone();
                                        move |_, _, cx| {
                                            let _ =
                                                app.update(cx, |app, cx| app.retry_services(cx));
                                        }
                                    }),
                            )
                        }),
                )
            })
            .child(div().flex_1().min_h_0().child(match self.page {
                Page::History => self.history.clone().into_any_element(),
                Page::Settings => self.settings.clone().into_any_element(),
            }))
    }
}
pub fn run() -> std::process::ExitCode {
    match launch() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("[yue-shot][startup] {error}");
            std::process::ExitCode::FAILURE
        }
    }
}
fn launch() -> Result<(), String> {
    let options = startup::StartupOptions::parse(std::env::args_os().skip(1))?;
    let channel = startup::Channel::build()?;
    match options.launch {
        startup::Launch::ProductInfo => {
            println!("{}", product::info(channel));
            return Ok(());
        }
        startup::Launch::UninstallCleanup => {
            autostart::remove_installation_startup(channel)?;
            return Ok(());
        }
        startup::Launch::Help => {
            println!("{}", startup::HELP);
            return Ok(());
        }
        startup::Launch::Version => {
            println!(
                "YueShot {} ({}, windows-x64)",
                env!("CARGO_PKG_VERSION"),
                channel.name()
            );
            return Ok(());
        }
        _ => {}
    }
    let local = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
    let paths = startup::ProductPaths::resolve(&options, channel, local.as_deref())?;
    if options.launch == startup::Launch::RemoveStartup {
        autostart::Registration::remove_for(&paths)?;
        println!("YueShot per-user login startup removed for this channel/data root");
        return Ok(());
    }
    // Keep the data lock on this stack through GPUI shutdown and all service destruction.
    // Help/version above never touch data. Smoke must also respect a running writer.
    let Some(_data_lock) = instance::DataLock::acquire(&paths)? else {
        if matches!(
            options.launch,
            startup::Launch::Smoke
                | startup::Launch::Backup
                | startup::Launch::Restore
                | startup::Launch::ExportDiagnostics
        ) {
            return Err("maintenance refused: data directory already has an active writer".into());
        }
        let command = instance::Command::from_launch(options.launch)
            .ok_or("unsupported instance launch command")?;
        instance::forward(&paths, command)?;
        println!("YueShot command accepted by the existing instance");
        return Ok(());
    };
    if options.launch == startup::Launch::ExportDiagnostics {
        diagnostics::export(
            &paths.root,
            options
                .maintenance_path
                .as_deref()
                .ok_or("missing diagnostics destination")?,
            channel.name(),
        )?;
        println!(
            "YueShot diagnostics exported; no message bodies, paths, images or clipboard content; no upload"
        );
        return Ok(());
    }
    if options.launch == startup::Launch::Backup {
        maintenance::backup(
            &paths.root,
            options
                .maintenance_path
                .as_deref()
                .ok_or("missing backup path")?,
        )?;
        println!("YueShot backup completed and verified");
        return Ok(());
    }
    if options.launch == startup::Launch::Restore {
        maintenance::restore(
            &paths.root,
            options
                .maintenance_path
                .as_deref()
                .ok_or("missing restore path")?,
        )?;
        println!("YueShot restore completed and verified");
        return Ok(());
    }
    maintenance::ensure_ready(&paths.root)?;
    if options.launch == startup::Launch::Smoke {
        let store = CaptureHistoryStore::open(&paths.root).map_err(|e| e.to_string())?;
        store
            .history_page(None, Some(1))
            .map_err(|e| e.to_string())?;
        let settings = settings::SettingsStore::new(&paths.root);
        let value = settings.load()?;
        settings.save(&value)?;
        println!(
            "YueShot smoke-test passed: configuration and screenshot storage; no UI/GPU/hotkeys"
        );
        return Ok(());
    }
    let _logging = logging::init(&paths.root, channel.name())?;
    if let Err(message) = yue_shot_capture::windows::monitor::set_per_monitor_v2_awareness() {
        return Err(format!("DPI awareness: {message}"));
    }
    let aumid: Vec<u16> = paths.aumid().encode_utf16().chain(Some(0)).collect();
    let result = unsafe {
        windows_sys::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID(aumid.as_ptr())
    };
    if result < 0 {
        return Err(format!("set application identity failed: 0x{result:08x}"));
    }
    let (instance, commands) = instance::Instance::start(&paths)?;
    let command =
        instance::Command::from_launch(options.launch).ok_or("unsupported launch command")?;
    gpui_kit::application()
        // The product owns process lifetime: removing the last view must keep
        // capture, IPC and tray services alive until request_quit allows exit.
        .with_quit_mode(QuitMode::Explicit)
        .with_assets(gpui_kit::assets::Assets)
        .run(move |cx| {
            gpui_kit::init(cx);
            let dark = matches!(
                cx.window_appearance(),
                WindowAppearance::Dark | WindowAppearance::VibrantDark
            );
            save_panel::init(cx, dark, &paths.root);
            yue_pin::init(cx);
            let app = cx.new(|cx| YueShotApp::new(paths, cx));
            app.update(cx, |app, cx| app.attach_instance(instance, commands, cx));
            cx.set_global(ProductOwner(app.clone()));
            cx.on_app_quit(|cx| {
                if cx.has_global::<ProductOwner>() {
                    let owner = cx.remove_global::<ProductOwner>().0;
                    owner.update(cx, |app, cx| app.shutdown(cx));
                }
                async {}
            })
            .detach();
            app.update(cx, |app, cx| app.launch(command, cx));
        });
    Ok(())
}
#[cfg(test)]
mod tests {
    #[test]
    fn binary_is_a_thin_composition_entry() {
        let source = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/main.rs"))
            .expect("binary source is readable");
        assert!(source.contains("yue_shot::run()"));
        assert!(!source.contains("snapclip_app"));
    }
    pub(super) fn owner(cx: &mut gpui_kit::TestAppContext) -> gpui_kit::Entity<super::YueShotApp> {
        use gpui_kit::AppContext as _;
        let data = std::env::temp_dir().join(format!("yueshot-owner-ui-{}", std::process::id()));
        cx.update(|cx| super::save_panel::init(cx, false, &data));
        let settings = cx.new(|_| {
            super::settings::SettingsView::new(super::settings::SettingsStore::new(&data))
        });
        cx.new(|cx| {
            let sync = cx.subscribe(
                &settings,
                |_: &mut super::YueShotApp, _, _: &super::settings::SettingsChanged, _| {},
            );
            super::YueShotApp {
                data: data.clone(),
                capture: std::sync::Arc::new(yue_shot_capture::CaptureRuntime::new(Box::new(
                    super::capture_host::UnavailableOverlay,
                ))),
                pins: None,
                pin_source: std::sync::Arc::new(super::pin_source::PinSource::new()),
                settings,
                save_panels: Default::default(),
                history: None,
                views: Vec::new(),
                main: None,
                tray: None,
                tray_options: yue_shell::TrayOptions::new("YueShot.Test", "Test", vec![]),
                tray_events: async_channel::unbounded().0,
                tray_status: String::new(),
                capture_status: None,
                capture_events: super::capture_host::Events::new(
                    async_channel::unbounded().0,
                    async_channel::unbounded().0,
                    async_channel::unbounded().0,
                    async_channel::bounded(8).0,
                ),
                instance: None,
                startup_status: vec!["persistent startup failure".into()],
                status: vec!["test failure remains visible".into()],
                tasks: Vec::new(),
                _settings_sync: sync,
            }
        })
    }
    #[gpui_kit::test]
    fn opening_the_main_window_does_not_reenter_its_application_owner(
        cx: &mut gpui_kit::TestAppContext,
    ) {
        let owner = owner(cx);
        cx.update(|cx| owner.update(cx, |app, cx| app.open_main(super::Page::History, cx)));
        cx.run_until_parked();
        cx.update(|cx| {
            let app = owner.read(cx);
            assert!(app.main.is_some());
            assert_eq!(app.views.len(), 1);
        });
        // Launch handoff drains on the app context and reuses the existing window.
        // Real OS pipe/thread handoff is covered by instance::tests. The deterministic GPUI
        // scheduler requires this side's wakeups to originate on its own test thread.
        let (sender, receiver) = async_channel::bounded(32);
        cx.update(|cx| owner.update(cx, |app, cx| app.receive_instance_commands(receiver, cx)));
        for _ in 0..2 {
            sender
                .try_send(super::instance::Event::Command(
                    super::instance::Command::Show,
                ))
                .unwrap();
            cx.run_until_parked();
        }
        cx.update(|cx| {
            assert_eq!(owner.read(cx).views.len(), 1);
            owner.update(cx, |app, cx| app.shutdown(cx));
        });
    }
    #[gpui_kit::test]
    fn capture_errors_reach_the_existing_window_without_unbounded_duplicates(
        cx: &mut gpui_kit::TestAppContext,
    ) {
        use yue_shot_capture::ports::CaptureEventSink as _;
        let owner = owner(cx);
        let (sender, receiver) = async_channel::bounded(8);
        let sink = super::capture_host::Events::new(
            async_channel::unbounded().0,
            async_channel::unbounded().0,
            async_channel::unbounded().0,
            sender,
        );
        cx.update(|cx| owner.update(cx, |app, cx| app.receive_capture_failures(receiver, cx)));
        for _ in 0..2 {
            sink.on_failed(
                Some("test"),
                &yue_shot_capture::CaptureError::Unsupported,
                "test-provider",
            );
            cx.run_until_parked();
        }
        cx.update(|cx| {
            let app = owner.read(cx);
            assert_eq!(
                app.status.len(),
                2,
                "same runtime failure is not appended twice"
            );
            assert_eq!(app.views.len(), 1, "failure updates the same window");
            let view = app.main.as_ref().unwrap().1.upgrade().unwrap();
            assert!(view.read(cx).status.contains("test-provider"));
            assert!(view.read(cx).status.contains("可重新发起截图"));
            assert!(
                app.capture_status.is_none(),
                "an operation failure never replaces a live service"
            );
        });
        cx.update(|cx| {
            owner.update(cx, |app, cx| {
                for index in 0..30 {
                    app.report(format!("failure {index}"), cx);
                }
                assert_eq!(app.status.len(), 16);
                assert!(app.status_text().contains("persistent startup failure"));
                app.shutdown(cx);
            })
        });
    }
    #[gpui_kit::test]
    #[ignore = "creates an isolated native tray; headless GPUI receives no OS-thread wakeups"]
    fn closing_and_reopening_the_window_keeps_services_and_reads_live_tray_health(
        cx: &mut gpui_kit::TestAppContext,
    ) {
        let owner = owner(cx);
        let tray = yue_shell::Tray::start_with_handler(
            yue_shell::TrayOptions::new(
                format!("YueShot.WindowCloseTest.{}", std::process::id()),
                "Close test",
                vec![],
            ),
            std::sync::Arc::new(|_| {}),
        )
        .unwrap();
        assert!(tray.is_available());
        let capture = cx.update(|cx| owner.read(cx).capture.clone());
        cx.update(|cx| {
            owner.update(cx, |app, cx| {
                app.tray = Some(tray);
                app.open_main(super::Page::History, cx);
                app.close_main(cx);
                assert!(app.main.is_none());
                assert!(std::sync::Arc::ptr_eq(&capture, &app.capture));
                assert!(app.tray.as_ref().unwrap().is_available());
            })
        });
        cx.run_until_parked();
        cx.update(|cx| {
            owner.update(cx, |app, cx| {
                app.open_main(super::Page::History, cx);
                assert_eq!(
                    app.views.len(),
                    1,
                    "reopening does not retain destroyed history views"
                );
                app.tray.as_mut().unwrap().shutdown();
                assert!(!app.tray.as_ref().unwrap().is_available());
                app.close_main(cx);
                // TestPlatform::quit is deliberately a no-op. A missing tray must choose that
                // path instead of destroying the last reachable window and leaving a process.
                assert!(
                    app.main.is_some(),
                    "unavailable tray must not hide the main window"
                );
                assert!(std::sync::Arc::ptr_eq(&capture, &app.capture));
                app.shutdown(cx);
            })
        });
    }
    #[gpui_kit::test]
    #[ignore = "temporarily reserves F5; creates hidden capture window and a test tray"]
    fn a_conflicting_product_hotkey_does_not_disable_the_capture_runtime(
        cx: &mut gpui_kit::TestAppContext,
    ) {
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::{RegisterHotKey, UnregisterHotKey};
        struct Reservation;
        impl Drop for Reservation {
            fn drop(&mut self) {
                unsafe {
                    UnregisterHotKey(std::ptr::null_mut(), 0x1234);
                }
            }
        }
        assert_ne!(
            unsafe { RegisterHotKey(std::ptr::null_mut(), 0x1234, 0, 0x74) },
            0
        );
        let reservation = Reservation;
        let owner = owner(cx);
        cx.update(|cx| {
            owner.update(cx, |app, cx| {
                let keys = super::shortcuts::Shortcuts::default();
                app.tray_options = yue_shell::TrayOptions::new(
                    format!("YueShot.ProductHotkeyTest.{}", std::process::id()),
                    "Hotkey test",
                    keys.menu(),
                )
                .with_hotkeys(keys.hotkeys().unwrap());
                app.capture_status = Some("not initialized".into());
                app.retry_services(cx);
                assert!(
                    app.capture_status.is_none(),
                    "capture initialization no longer depends on product hotkey availability"
                );
                assert!(app.capture.preferences().is_some());
                let status = app.tray.as_ref().unwrap().status();
                assert_eq!(status.hotkey_failures().len(), 1);
                assert_eq!(
                    status.hotkey_failures()[0].command(),
                    super::Command::Capture.id()
                );
                assert!(app.tray_status.contains("开始截图"));
                let capture = app.capture.clone();
                app.retry_services(cx);
                assert!(std::sync::Arc::ptr_eq(&capture, &app.capture));
            })
        });
        drop(reservation);
        cx.update(|cx| {
            owner.update(cx, |app, _| {
                app.tray.as_ref().unwrap().retry().unwrap();
            })
        });
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            if cx.update(|cx| {
                owner
                    .read(cx)
                    .tray
                    .as_ref()
                    .unwrap()
                    .status()
                    .hotkey_failures()
                    .is_empty()
            }) {
                break;
            }
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        cx.update(|cx| owner.update(cx, |app, cx| app.shutdown(cx)));
    }
}
