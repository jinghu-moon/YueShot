//! Product-level global shortcut form. Inputs remain editable after a failed registration.
use crate::{YueShotApp, shortcuts::Shortcuts};
use gpui_kit::{
    base::{StyledExt as _, input::InputState},
    prelude::FluentBuilder as _,
    *,
};
use yue_component::{YueButton, YueInput};
pub(super) struct ShortcutEditor {
    app: WeakEntity<YueShotApp>,
    fields: [Entity<InputState>; 3],
    busy: bool,
    status: Option<String>,
}
impl ShortcutEditor {
    pub(super) fn new(
        app: WeakEntity<YueShotApp>,
        value: &Shortcuts,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let fields = [&value.capture, &value.scroll, &value.pin]
            .map(|value| cx.new(|cx| InputState::new(window, cx).default_value(value.clone())));
        Self {
            app,
            fields,
            busy: false,
            status: None,
        }
    }
    fn apply(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let [capture, scroll, pin] = self
            .fields
            .each_ref()
            .map(|field| field.read(cx).value().trim().to_string());
        let next = Shortcuts {
            capture,
            scroll,
            pin,
        };
        let app = self.app.clone();
        let editor = cx.entity().downgrade();
        self.busy = true;
        self.status = None;
        cx.notify();
        cx.defer(move |cx| {
            let _ = app.update(cx, |app, cx| app.apply_shortcuts(next, editor, cx));
        });
    }
    pub(super) fn finish(&mut self, result: Result<(), String>, cx: &mut Context<Self>) {
        self.busy = false;
        self.status = Some(match result {
            Ok(()) => "已保存并生效".into(),
            Err(error) => format!("未应用：{error}"),
        });
        cx.notify();
    }
}
impl Render for ShortcutEditor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = yue_component::current_theme(cx);
        div().v_flex().gap_3().min_w_0()
            .child(div().text_sm().child("全局快捷键"))
            .child(div().text_xs().child("输入 F5 或 Ctrl+Shift+P 等组合；按应用后生效。绑定失败保留原组合。按键按 Windows 虚拟键识别，截图内的 Ctrl+S、Esc 单独生效。"))
            .children(self.fields.iter().zip([("global-capture","开始截图"),("global-scroll","开始长截图"),("global-pin","贴出最近截图")]).map(|(field,(id,label))| {
                div().v_flex().gap_1().min_w_0().child(div().text_sm().child(label))
                    .child(YueInput::new(id,theme,field).accessibility_label(label).disabled(self.busy).block())
            }))
            .child(YueButton::new("apply-global-shortcuts",if self.busy {"正在应用…"} else {"应用快捷键"}).disabled(self.busy).on_click(cx.listener(|this,_,_,cx| this.apply(cx))))
            .when_some(self.status.clone(),|view,status| view.child(div().text_sm().child(status)))
    }
}

#[cfg(test)]
mod tests {
    use super::{ShortcutEditor, Shortcuts};
    use gpui_kit::{AppContext as _, TestAppContext, px, size};
    use gpui_kit::{base::Root, test::TestWindowExt as _};
    #[gpui_kit::test]
    fn the_apply_button_reports_failure_without_changing_the_saved_binding(
        cx: &mut TestAppContext,
    ) {
        let owner = crate::tests::owner(cx);
        let mut editor = None;
        let handle = cx.open_window(size(px(480.), px(640.)), |window, cx| {
            let view = cx.new(|cx| {
                ShortcutEditor::new(owner.downgrade(), &Shortcuts::default(), window, cx)
            });
            editor = Some(view.clone());
            Root::new(view, window, cx)
        });
        let editor = editor.unwrap();
        cx.update_window(handle.into(), |_, window, cx| {
            editor.read(cx).fields[0]
                .clone()
                .update(cx, |state, cx| state.set_value("F6", window, cx));
            window.render_frame(cx);
            window.click("apply-global-shortcuts", cx);
        })
        .unwrap();
        cx.run_until_parked();
        cx.update(|cx| {
            assert!(!editor.read(cx).busy);
            assert!(editor.read(cx).status.as_deref().unwrap().contains("重复"));
            assert_eq!(
                owner
                    .read(cx)
                    .settings
                    .read(cx)
                    .settings()
                    .shortcuts
                    .capture,
                "F5"
            );
            assert!(!owner.read(cx).settings.read(cx).shortcuts_busy());
        });
        cx.update(|cx| owner.update(cx, |app, cx| app.shutdown(cx)));
    }
}
