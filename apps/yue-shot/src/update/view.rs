use super::{Offer, Result, Staged};
use gpui_kit::base::StyledExt as _;
use gpui_kit::*;
use std::path::{Path, PathBuf};
use yue_component::YueButton;

#[derive(Clone)]
pub(crate) struct InstallUpdate(pub(crate) Staged);
pub(crate) struct UpdateView {
    channel: String,
    data: PathBuf,
    status: String,
    offer: Option<Offer>,
    staged: Option<Staged>,
    busy: bool,
    task: Option<Task<()>>,
}
impl EventEmitter<InstallUpdate> for UpdateView {}
impl UpdateView {
    pub(crate) fn new(channel: &str, data: &Path) -> Self {
        Self {
            channel: channel.into(),
            data: data.into(),
            status: super::handoff::last_result(data)
                .unwrap_or_else(|| format!("当前版本 {}（{channel}）", env!("CARGO_PKG_VERSION"))),
            offer: None,
            staged: None,
            busy: false,
            task: None,
        }
    }
    pub(crate) fn finish_handoff(&mut self, result: Result<()>, cx: &mut Context<Self>) {
        self.busy = false;
        self.status = match result {
            Ok(()) => "即将退出 YueShot 并安装更新".into(),
            Err(error) => error,
        };
        cx.notify();
    }
    fn check(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.offer = None;
        self.staged = None;
        self.status = "正在检查更新".into();
        let channel = self.channel.clone();
        self.task = Some(cx.spawn(async move |view, cx| {
            let result = cx
                .background_spawn(async move { super::check(&channel) })
                .await;
            let _ = view.update(cx, |view, cx| {
                view.busy = false;
                match result {
                    Ok(Some(offer)) => {
                        view.status = format!("发现新版本 {}", offer.version());
                        view.offer = Some(offer);
                    }
                    Ok(None) => view.status = "当前已是最新版本".into(),
                    Err(error) => view.status = error,
                }
                cx.notify();
            });
        }));
        cx.notify();
    }
    fn download(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let Some(offer) = self.offer.clone() else {
            return;
        };
        self.busy = true;
        self.status = "正在下载并验证更新，YueShot 可以继续使用".into();
        let data = self.data.clone();
        self.task = Some(cx.spawn(async move |view, cx| {
            let result = cx
                .background_spawn(async move { super::download(offer, &data) })
                .await;
            let _ = view.update(cx, |view, cx| {
                view.busy = false;
                match result {
                    Ok(staged) => {
                        view.status = format!("版本 {} 已下载并通过验证", staged.offer.version());
                        view.staged = Some(staged);
                    }
                    Err(error) => view.status = error,
                }
                cx.notify();
            });
        }));
        cx.notify();
    }
}
impl Render for UpdateView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut actions = div().h_flex().gap_2().child(
            YueButton::new("check-updates", "检查更新")
                .disabled(self.busy)
                .on_click(cx.listener(|view, _, _, cx| view.check(cx))),
        );
        if self.staged.is_some() {
            actions = actions.child(
                YueButton::new("install-update", "退出并安装更新")
                    .disabled(self.busy)
                    .on_click(cx.listener(|view, _, _, cx| {
                        if let Some(staged) = view.staged.clone() {
                            view.busy = true;
                            view.status = "正在准备更新".into();
                            cx.emit(InstallUpdate(staged));
                            cx.notify();
                        }
                    })),
            );
        } else if self.offer.is_some() {
            actions = actions.child(
                YueButton::new("download-update", "下载更新")
                    .disabled(self.busy)
                    .on_click(cx.listener(|view, _, _, cx| view.download(cx))),
            );
        }
        div()
            .v_flex()
            .gap_2()
            .child(div().text_sm().child("软件更新"))
            .child(div().text_sm().child(self.status.clone()))
            .child(actions)
    }
}

#[cfg(test)]
mod tests {
    use super::UpdateView;
    use gpui_kit::{AppContext, TestAppContext, px, size};
    #[gpui_kit::test]
    fn check_button_reports_unconfigured_keys_and_remains_retryable(cx: &mut TestAppContext) {
        use gpui_kit::{base::Root, test::TestWindowExt};
        // This scenario uses an intentionally unconfigured channel, so it never accesses the network.
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
        let mut view = None;
        let window = cx.open_window(size(px(600.), px(220.)), |window, cx| {
            let entity = cx.new(|_| UpdateView::new("invalid", &std::env::temp_dir()));
            view = Some(entity.clone());
            Root::new(entity, window, cx)
        });
        let view = view.unwrap();
        for _ in 0..2 {
            cx.update_window(window.into(), |_, window, cx| {
                window.render_frame(cx);
                window.click("check-updates", cx);
                assert!(view.read(cx).busy);
            })
            .unwrap();
            cx.run_until_parked();
            cx.update(|cx| {
                let view = view.read(cx);
                assert!(!view.busy);
                assert!(view.status.contains("尚未配置更新公钥"));
                assert!(view.offer.is_none());
                assert!(view.staged.is_none());
            });
        }
    }
}
