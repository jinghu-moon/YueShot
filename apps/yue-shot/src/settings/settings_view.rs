//! The settings page.
//!
//! Follows the Design Guides' "Forms and settings": a visible label per field, help text
//! beside the field it describes, a `Switch` for a setting that takes effect immediately and
//! radio buttons for a small set of choices. The result of a change is shown next to
//! the control, not in a dialog.
//!
//! Copy note (Design Guides, interface language): the label names the object, the help line
//! says what the control does, and neither repeats the other.

use gpui_kit::base::StyledExt as _;
use gpui_kit::*;
use yue_component::{YueButton, YueRadio, YueSwitch, current_theme};

use yue_shot_capture::save_format::SaveFormat;
use yue_shot_capture::units::{LabelDisplay, LengthUnit};

use super::model::{Settings, SettingsStore};

/// The settings after a change was saved; the shell forwards what capture needs.
#[derive(Debug, Clone)]
pub struct SettingsChanged(pub Settings);

/// The switches on the page. Each one writes exactly one setting, and the pairing is checked by
/// the compiler rather than by a string that has to match the widget id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Toggle {
    CloseToTray,
    DeepSelectTextRuns,
    MaskBlur,
    ShortcutGuide,
    ShowSize,
    ShowCoordinates,
    ShowUnit,
}

/// The label units in the order the radio group shows them.
///
/// The group speaks in indices and the settings speak in units; keeping one list for both means
/// the pair cannot drift apart when a third unit arrives.
const LABEL_UNITS: [LengthUnit; 2] = [LengthUnit::Px, LengthUnit::Dp];

/// The id of the unit row: the widget's id and the key its result is shown under are the same.
const LABEL_UNIT_ID: &str = "label-unit";

const SAVE_FORMATS: [(SaveFormat, &str, &str); 3] = [
    (SaveFormat::Png, "save-format-png", "PNG"),
    (SaveFormat::Jpeg, "save-format-jpeg", "JPEG"),
    (SaveFormat::Webp, "save-format-webp", "WebP"),
];

pub struct SettingsView {
    pub(crate) shortcut_editor: Option<Entity<crate::shortcut_editor::ShortcutEditor>>,
    shortcut_busy: bool,
    deferred_label: Option<LabelDisplay>,
    login_startup: Option<std::sync::Arc<dyn crate::autostart::LoginStartup>>,
    login_status: Result<crate::autostart::Status, String>,
    store: SettingsStore,
    settings: Settings,
    saved: Settings,
    load_error: Option<String>,
    /// Result of the last change, shown beside the control it describes.
    status: Option<(&'static str, String)>,
}

impl EventEmitter<SettingsChanged> for SettingsView {}

impl SettingsView {
    pub fn new(store: SettingsStore) -> Self {
        let (settings, load_error) = match store.load() {
            Ok(settings) => (settings, None),
            Err(error) => (Settings::default(), Some(error)),
        };
        Self {
            shortcut_editor: None,
            shortcut_busy: false,
            deferred_label: None,
            login_startup: None,
            login_status: Ok(crate::autostart::Status::Missing),
            store,
            saved: settings.clone(),
            settings,
            load_error,
            status: None,
        }
    }

    pub(crate) fn begin_shortcuts(
        &mut self,
        shortcuts: crate::shortcuts::Shortcuts,
    ) -> Result<Settings, String> {
        if self.shortcut_busy {
            return Err("已有快捷键设置正在提交".into());
        }
        if let Some(error) = &self.load_error {
            return Err(error.clone());
        }
        let mut next = self.settings.clone();
        next.shortcuts = shortcuts;
        next.shortcuts.validate(
            &yue_shot_capture::keymap::Keymap::new(&next.keymap).map_err(|e| e.to_string())?,
        )?;
        self.shortcut_busy = true;
        Ok(next)
    }
    pub(crate) fn finish_shortcuts(&mut self, next: Option<Settings>, cx: &mut Context<Self>) {
        self.shortcut_busy = false;
        if let Some(next) = next {
            self.settings = next;
            self.saved = self.settings.clone();
            cx.emit(SettingsChanged(self.settings.clone()));
        }
        if let Some(label) = self.deferred_label.take() {
            self.adopt_label(label, cx);
        }
        cx.notify();
    }
    pub(crate) fn shortcuts_busy(&self) -> bool {
        self.shortcut_busy
    }
    pub fn settings(&self) -> &Settings {
        &self.settings
    }
    pub(crate) fn with_login_startup(
        mut self,
        startup: Result<std::sync::Arc<dyn crate::autostart::LoginStartup>, String>,
    ) -> Self {
        match startup {
            Ok(startup) => {
                self.login_status = startup.status();
                self.login_startup = Some(startup);
            }
            Err(error) => self.login_status = Err(error),
        }
        self
    }
    pub(crate) fn refresh_login_startup(&mut self, cx: &mut Context<Self>) {
        if let Some(startup) = &self.login_startup {
            self.login_status = startup.status();
            if self
                .status
                .as_ref()
                .is_some_and(|(id, _)| *id == "login-startup")
            {
                self.status = None;
            }
            cx.notify();
        }
    }
    fn set_login_startup(&mut self, registered: bool, cx: &mut Context<Self>) {
        if let Some(startup) = &self.login_startup {
            let result = startup.set_registered(registered);
            // On failure, re-read the registry rather than showing a speculative checkmark.
            self.login_status = startup.status();
            self.status = Some((
                "login-startup",
                match result {
                    Ok(status) => status.message().into(),
                    Err(error) => format!("更新登录启动失败：{error}"),
                },
            ));
            cx.notify();
        }
    }

    /// Apply one switch and persist it.
    ///
    /// `Switch` is the control for a setting that takes effect immediately, so this writes
    /// straight through; a failed write is reported next to the control instead of being
    /// swallowed, because a setting the user believes is saved but is not is worse than an
    /// error message.
    fn set_switch(
        &mut self,
        toggle: Toggle,
        value: bool,
        id: &'static str,
        cx: &mut Context<Self>,
    ) {
        match toggle {
            Toggle::CloseToTray => self.settings.close_to_tray = value,
            Toggle::DeepSelectTextRuns => self.settings.deep_select_text_runs = value,
            Toggle::MaskBlur => self.settings.mask_blur = value,
            Toggle::ShortcutGuide => self.settings.shortcut_guide = value,
            Toggle::ShowSize => self.settings.show_size = value,
            Toggle::ShowCoordinates => self.settings.show_coordinates = value,
            Toggle::ShowUnit => self.settings.show_unit = value,
        }
        let result = if value { "已开启" } else { "已关闭" }.to_string();
        self.persist(id, result, cx);
    }

    /// Apply the label unit, which is a choice rather than a switch.
    fn set_length_unit(&mut self, unit: LengthUnit, cx: &mut Context<Self>) {
        self.settings.length_unit = unit;
        self.persist(LABEL_UNIT_ID, format!("已改为 {}", unit.suffix()), cx);
    }

    fn set_save_format(&mut self, format: SaveFormat, cx: &mut Context<Self>) {
        if self.settings.save_format == format {
            return;
        }
        self.settings.save_format = format;
        let label = SAVE_FORMATS
            .iter()
            .find_map(|(candidate, _, label)| (*candidate == format).then_some(*label))
            .unwrap_or("PNG");
        self.persist("save-format", format!("默认格式：{label}"), cx);
    }

    /// Adopt the display settings a capture ended with (docs/23 §14.53).
    ///
    /// The overlay's badge menu edits the same three values the page shows, and the shell hands the
    /// result back here when the session is over. Writing through this view — rather than to the
    /// file directly — is what keeps the page and the file from disagreeing, and what makes the
    /// existing `SettingsChanged` subscriber push the change back to capture for the next session.
    ///
    /// Returns whether anything changed, so the caller can stay quiet about an unchanged capture.
    pub fn adopt_label(&mut self, label: LabelDisplay, cx: &mut Context<Self>) -> bool {
        if self.shortcut_busy {
            self.deferred_label = Some(label);
            return false;
        }
        if !self.settings.adopt_label(label) {
            return false;
        }
        // The page's status line names the control the user touched; this change came from the
        // overlay, so it is reported under the unit row that the two changes have in common.
        self.persist(LABEL_UNIT_ID, "已从截图浮层更新".to_string(), cx);
        true
    }

    /// Write the settings out and report the result beside the control that changed.
    fn persist(&mut self, at: &'static str, result: String, cx: &mut Context<Self>) {
        let result_of_save = if self.shortcut_busy {
            Err("正在应用快捷键，请稍后重新修改此项".to_string())
        } else {
            self.load_error
                .clone()
                .map_or_else(|| self.store.save(&self.settings), Err)
        };
        let status = match result_of_save {
            Ok(()) => {
                self.saved = self.settings.clone();
                cx.emit(SettingsChanged(self.settings.clone()));
                result
            }
            Err(error) => {
                self.settings = self.saved.clone();
                format!("保存失败：{error}")
            }
        };
        self.status = Some((at, status));
        cx.notify();
    }

    /// One switch, its help line and, when it was the last one changed, the result.
    fn switch(
        &self,
        toggle: Toggle,
        id: &'static str,
        checked: bool,
        label: &'static str,
        help: &'static str,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let muted = gpui_kit::Hsla::from(gpui_kit::rgb(
            yue_component::current_theme(cx).text_secondary(),
        ));
        div()
            .v_flex()
            .gap_1()
            .child(
                div()
                    .h_flex()
                    .items_center()
                    .gap_2()
                    .child(
                        YueSwitch::new(id, current_theme(cx), checked, label).on_change({
                            let owner = cx.entity().downgrade();
                            move |next, _, _, cx| {
                                let _ = owner
                                    .update(cx, |view, cx| view.set_switch(toggle, next, id, cx));
                            }
                        }),
                    )
                    .child(
                        div()
                            .id((ElementId::from(id), "label"))
                            .child(label)
                            .on_click(cx.listener(move |view, _, _, cx| {
                                view.set_switch(toggle, !checked, id, cx);
                            })),
                    ),
            )
            .child(div().text_xs().text_color(muted).child(help))
            .children(self.result(id, muted))
    }

    /// The label unit: one visible label, the two choices, the formula behind `dp` and, when it
    /// was the last control changed, the result.
    ///
    /// Each controlled radio keeps its unit-derived identity and writes the shared value directly.
    ///
    /// The help line carries the arithmetic because `dp` is not a word the user has to already
    /// know, and the scale is read from the display the capture happens on rather than guessed
    /// for the display this window happens to be on.
    fn unit_choice(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = gpui_kit::Hsla::from(gpui_kit::rgb(
            yue_component::current_theme(cx).text_secondary(),
        ));
        div()
            .v_flex()
            .gap_1()
            .child(div().text_sm().child("标签单位"))
            .child(
                div()
                    .h_flex()
                    .gap_3()
                    .children(LABEL_UNITS.map(|unit| self.unit_radio(unit, cx))),
            )
            .child(
                div().text_xs().text_color(muted).child(
                    "dp = px ÷ 屏幕缩放：150% 显示器上 1500 px = 1000 dp；尺寸和坐标一起换算",
                ),
            )
            .children(self.result(LABEL_UNIT_ID, muted))
    }

    fn save_format_choice(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = gpui_kit::Hsla::from(gpui_kit::rgb(
            yue_component::current_theme(cx).text_secondary(),
        ));
        div()
            .v_flex()
            .gap_1()
            .child(div().text_sm().child("默认保存格式"))
            .child(
                div()
                    .h_flex()
                    .flex_wrap()
                    .gap_3()
                    .children(SAVE_FORMATS.map(|(format, id, label)| {
                        YueRadio::new(id, current_theme(cx), format == self.settings.save_format)
                            .label(label)
                            .on_change({
                                let owner = cx.entity().downgrade();
                                move |checked, _, _, cx| {
                                    if checked {
                                        let _ = owner.update(cx, |view, cx| {
                                            view.set_save_format(format, cx)
                                        });
                                    }
                                }
                            })
                    })),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child("用于普通截图保存面板；长截图格式按尺寸限制选择，HDR 源默认保留为 PNG"),
            )
            .children(self.result("save-format", muted))
    }

    fn login_startup_row(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = current_theme(cx);
        let registered = self.login_status.as_ref().is_ok_and(|s| s.is_registered());
        let owner = cx.entity().downgrade();
        let mut row = div()
            .v_flex()
            .gap_1()
            .child(
                div()
                    .h_flex()
                    .gap_2()
                    .items_center()
                    .child(
                        YueSwitch::new(
                            "login-startup",
                            theme,
                            registered,
                            "登录 Windows 时启动 YueShot",
                        )
                        .disabled(self.login_startup.is_none() || self.login_status.is_err())
                        .on_change(move |registered, _, _, cx| {
                            let _ =
                                owner.update(cx, |view, cx| view.set_login_startup(registered, cx));
                        }),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child("登录 Windows 时启动 YueShot"),
                    )
                    .child(
                        YueButton::new("refresh-login-startup", "刷新状态")
                            .on_click(cx.listener(|view, _, _, cx| view.refresh_login_startup(cx))),
                    ),
            )
            .child(
                div()
                    .text_xs()
                    .child("仅为当前 Windows 用户注册，登录后在后台运行；默认不开启"),
            );
        row = row.child(div().text_xs().child(match &self.login_status {
            Ok(status) => status.message().to_owned(),
            Err(error) => format!("无法读取登录启动状态：{error}"),
        }));
        if matches!(
            self.login_status,
            Ok(crate::autostart::Status::DifferentCommand)
        ) {
            row = row.child(
                YueButton::new("repair-login-startup", "更新启动项")
                    .on_click(cx.listener(|view, _, _, cx| view.set_login_startup(true, cx))),
            );
        }
        if let Some(result) = self.result(
            "login-startup",
            gpui_kit::Hsla::from(gpui_kit::rgb(theme.text_secondary())),
        ) {
            row = row.child(result);
        }
        row
    }

    /// One unit choice: the unit it stands for is its id, and picking it writes that unit.
    fn unit_radio(&self, unit: LengthUnit, cx: &mut Context<Self>) -> YueRadio {
        let id = match unit {
            LengthUnit::Px => "label-unit-px",
            LengthUnit::Dp => "label-unit-dp",
        };
        YueRadio::new(id, current_theme(cx), unit == self.settings.length_unit)
            .label(unit.suffix())
            .on_change({
                let owner = cx.entity().downgrade();
                move |checked, _, _, cx| {
                    if checked {
                        let _ = owner.update(cx, |view, cx| view.set_length_unit(unit, cx));
                    }
                }
            })
    }

    /// The result of the last change, when that change was this control's.
    fn result(&self, at: &'static str, muted: Hsla) -> Option<impl IntoElement> {
        self.status
            .as_ref()
            .filter(|(changed, _)| *changed == at)
            .map(|(_, status)| div().text_xs().text_color(muted).child(status.clone()))
    }

    fn section(title: &'static str, help: &'static str, cx: &Context<Self>) -> impl IntoElement {
        div()
            .v_flex()
            .gap_1()
            .child(div().text_sm().child(title))
            .child(
                div()
                    .text_xs()
                    .text_color(gpui_kit::Hsla::from(gpui_kit::rgb(
                        yue_component::current_theme(cx).text_secondary(),
                    )))
                    .child(help),
            )
    }
}

impl Render for SettingsView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = yue_component::current_theme(cx);
        let (background, foreground, muted) = (
            gpui_kit::Hsla::from(gpui_kit::rgb(theme.page())),
            gpui_kit::Hsla::from(gpui_kit::rgb(theme.text_primary())),
            gpui_kit::Hsla::from(gpui_kit::rgb(theme.text_secondary())),
        );
        let panel = div()
            .id("settings-scroll")
            .v_flex()
            .size_full()
            .overflow_y_scroll()
            .gap_4()
            .p_4()
            .bg(background)
            .text_color(foreground)
            .child(Self::section(
                "吸附",
                "控制截图时鼠标所在内容如何被选为捕获目标",
                cx,
            ))
            .child(self.switch(
                Toggle::DeepSelectTextRuns,
                "deep-select-text-runs",
                self.settings.deep_select_text_runs,
                "吸附到文字行",
                "关闭后只吸附到盒子，不会单独选中一行文字",
                cx,
            ))
            .children(self.load_error.as_ref().map(|error| {
                div()
                    .text_sm()
                    .child(format!("配置未加载，已保留原文件：{error}"))
            }))
            .child(Self::section("截图界面", "从下一次截图开始生效", cx))
            .child(self.switch(
                Toggle::MaskBlur,
                "mask-blur",
                self.settings.mask_blur,
                "模糊选区外的画面",
                "关闭后选区外只变暗，不模糊；保存的图片不受影响",
                cx,
            ))
            .child(self.switch(
                Toggle::ShortcutGuide,
                "shortcut-guide",
                self.settings.shortcut_guide,
                "显示快捷键面板",
                "关闭后左下角不再列出快捷键",
                cx,
            ))
            .child(self.switch(
                Toggle::ShowSize,
                "label-size",
                self.settings.show_size,
                "标签显示尺寸",
                "关闭后标签只写坐标（尺寸标上就只剩位置）",
                cx,
            ))
            .child(self.switch(
                Toggle::ShowCoordinates,
                "label-coordinates",
                self.settings.show_coordinates,
                "标签显示坐标",
                "关闭后只显示尺寸；位置是选区左上角在屏幕上的物理像素",
                cx,
            ))
            .child(self.unit_choice(cx))
            .child(self.switch(
                Toggle::ShowUnit,
                "label-unit-suffix",
                self.settings.show_unit,
                "标签显示单位",
                "关闭后标签只写数字（1024×768），单位由左边的选项决定",
                cx,
            ))
            .child(Self::section("启动与关闭", "运行方式的更改立即生效", cx))
            .child(self.switch(
                Toggle::CloseToTray,
                "close-to-tray",
                self.settings.close_to_tray,
                "关闭主窗口后在托盘运行",
                "保留截图快捷键；关闭此项后，关闭主窗口将退出 YueShot",
                cx,
            ))
            .child(self.login_startup_row(cx))
            .child(Self::section("保存", "默认格式用于后续打开的普通截图保存面板", cx))
            .child(self.save_format_choice(cx))
            .child(
                div()
                    .v_flex()
                    .gap_1()
                    .child(div().text_sm().child("HDR 捕获"))
                    .child(div().text_xs().text_color(muted).child(if cfg!(feature = "hdr-capture") {
                        "当前为 hdr-capture 开发构建；捕获行为由构建 feature 决定，不是运行时设置"
                    } else {
                        "当前构建未启用 hdr-capture；HDR 行为由构建 feature 决定，不是运行时设置"
                    })),
            )
            .children(self.shortcut_editor.clone())
            .child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child(format!("设置文件：{}", self.store.path().display())),
            );
        #[cfg(feature = "test-support")]
        let panel = panel.test_support();
        panel
    }
}

#[cfg(test)]
mod tests {
    use super::SettingsView;
    use crate::settings::SettingsStore;

    #[test]
    fn a_view_starts_from_the_file() {
        use std::path::PathBuf;
        let dir =
            std::env::temp_dir().join(format!("yue-shot-settings-view-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = SettingsStore::new(&dir);
        store
            .save(&crate::settings::Settings {
                deep_select_text_runs: false,
                ..Default::default()
            })
            .expect("save");
        let view = SettingsView::new(SettingsStore::new(PathBuf::from(&dir)));
        assert!(!view.settings().deep_select_text_runs);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[gpui_kit::test]
    fn save_format_choice_updates_the_view_and_persisted_defaults(
        cx: &mut gpui_kit::TestAppContext,
    ) {
        use gpui_kit::{AppContext, base::Root, px, size, test::TestWindowExt};
        use std::path::PathBuf;
        use yue_shot_capture::save_format::SaveFormat;

        let data =
            std::env::temp_dir().join(format!("yueshot-save-format-ui-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&data);
        let mut view = None;
        cx.update(|cx| {
            gpui_kit::init(cx);
            yue_component::init(
                cx,
                yue_component::YueTheme::new(
                    yue_component::ThemeMode::Light,
                    yue_component::Accent::Azure,
                ),
            )
            .unwrap();
        });
        let handle = cx.open_window(size(px(740.), px(640.)), |window, cx| {
            let settings = cx.new(|_| SettingsView::new(SettingsStore::new(&data)));
            view = Some(settings.clone());
            Root::new(settings, window, cx)
        });
        let view = view.unwrap();
        cx.update_window(handle.into(), |_, window, cx| {
            window.render_frame(cx);
            window.scroll(
                "settings-scroll",
                gpui_kit::ScrollDelta::Pixels(gpui_kit::point(px(0.), px(-2400.))),
                cx,
            );
            window.click("save-format-jpeg", cx);
            assert_eq!(view.read(cx).settings().save_format, SaveFormat::Jpeg);
            assert_eq!(
                SettingsStore::new(PathBuf::from(&data))
                    .load()
                    .unwrap()
                    .save_format,
                SaveFormat::Jpeg
            );
        })
        .unwrap();
        let _ = std::fs::remove_dir_all(data);
    }

    #[derive(Default)]
    struct FakeLoginStartup {
        registered: std::sync::Mutex<bool>,
        fail_write: std::sync::atomic::AtomicBool,
        fail_read: std::sync::atomic::AtomicBool,
    }
    impl crate::autostart::LoginStartup for FakeLoginStartup {
        fn status(&self) -> Result<crate::autostart::Status, String> {
            if self.fail_read.load(std::sync::atomic::Ordering::Relaxed) {
                return Err("test: Windows refused to read startup".into());
            }
            Ok(if *self.registered.lock().unwrap() {
                crate::autostart::Status::Registered
            } else {
                crate::autostart::Status::Missing
            })
        }
        fn set_registered(&self, value: bool) -> Result<crate::autostart::Status, String> {
            if self.fail_write.load(std::sync::atomic::Ordering::Relaxed) {
                return Err("test: Windows refused to change startup".into());
            }
            *self.registered.lock().unwrap() = value;
            self.status()
        }
    }

    #[gpui_kit::test]
    fn login_switch_reads_windows_and_failed_writes_keep_the_actual_state(
        cx: &mut gpui_kit::TestAppContext,
    ) {
        use gpui_kit::{AppContext, base::Root, px, size, test::TestWindowExt};
        use std::sync::{Arc, atomic::Ordering};
        let data = std::env::temp_dir().join(format!("yueshot-login-ui-{}", std::process::id()));
        cx.update(|cx| {
            gpui_kit::init(cx);
            yue_component::init(
                cx,
                yue_component::YueTheme::new(
                    yue_component::ThemeMode::Light,
                    yue_component::Accent::Azure,
                ),
            )
            .unwrap();
        });
        let startup = Arc::new(FakeLoginStartup::default());
        let mut view = None;
        let handle = cx.open_window(size(px(740.), px(640.)), |window, cx| {
            let settings = cx.new(|_| {
                SettingsView::new(SettingsStore::new(&data)).with_login_startup(Ok(startup.clone()))
            });
            view = Some(settings.clone());
            Root::new(settings, window, cx)
        });
        let view = view.unwrap();
        cx.update_window(handle.into(), |_, window, cx| {
            window.render_frame(cx);
            window.scroll(
                "settings-scroll",
                gpui_kit::ScrollDelta::Pixels(gpui_kit::point(px(0.), px(-2000.))),
                cx,
            );
            window.click("login-startup", cx);
            assert!(*startup.registered.lock().unwrap());
            assert!(view.read(cx).login_status.as_ref().unwrap().is_registered());
            assert!(
                !data.join("settings.json").exists(),
                "OS registration has no duplicate settings boolean"
            );

            startup.fail_write.store(true, Ordering::Relaxed);
            window.render_frame(cx);
            window.click("login-startup", cx);
            assert!(*startup.registered.lock().unwrap());
            assert!(view.read(cx).login_status.as_ref().unwrap().is_registered());
            assert!(
                view.read(cx)
                    .status
                    .as_ref()
                    .unwrap()
                    .1
                    .contains("更新登录启动失败")
            );

            // External Windows settings changes are reflected by the actual refresh button.
            *startup.registered.lock().unwrap() = false;
            window.render_frame(cx);
            window.click("refresh-login-startup", cx);
            assert!(!view.read(cx).login_status.as_ref().unwrap().is_registered());
            startup.fail_read.store(true, Ordering::Relaxed);
            window.render_frame(cx);
            window.click("refresh-login-startup", cx);
            assert!(view.read(cx).login_status.is_err());
            startup.fail_read.store(false, Ordering::Relaxed);
            window.render_frame(cx);
            window.click("refresh-login-startup", cx);
            assert!(view.read(cx).login_status.is_ok());
        })
        .unwrap();
        let _ = std::fs::remove_dir_all(data);
    }
}
