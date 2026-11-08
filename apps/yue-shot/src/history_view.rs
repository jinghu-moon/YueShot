//! Screenshot history owns browsing and replay; application services outlive this view.
use gpui_kit::base::StyledExt as _;
use gpui_kit::base::virtualization::{
    VirtualizerChange, VirtualizerSizing, VirtualizerSource, VirtualizerState,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::{collections::HashMap, rc::Rc, sync::Arc};
use yue_clipboard::SystemClipboard;
use yue_component::{ButtonSize, YueButton, YueVirtualList};
use yue_shot_capture::save_format::SaveFormat;
use yue_shot_storage::capture_history::{
    CaptureHistoryEntry, CaptureHistoryPage, CaptureHistoryStore,
};
const HISTORY_PAGE_SIZE: u32 = 50;
// This geometry is shared by the row and virtualizer, invalidated as one unit.
const HISTORY_ROW_HEIGHT: Pixels = px(72.0);
#[derive(Clone, Copy)]
enum CaptureAction {
    Open,
    CopySdr,
    CopyHdr,
}
pub(crate) struct CaptureHistoryView {
    history: Option<CaptureHistoryStore>,
    history_rows: Rc<Vec<CaptureHistoryEntry>>,
    history_cursor: Option<String>,
    history_error: Option<String>,
    history_virtualizer: Entity<VirtualizerState>,
    history_revision: u64,
    selected: Option<String>,
    pins: Option<WeakEntity<yue_pin::PinManager>>,
    capture_busy: bool,
    capture_operation: Option<Task<()>>,
    status: Option<String>,
}
impl CaptureHistoryView {
    pub(crate) fn has_pending_operation(&self) -> bool {
        self.capture_busy
    }
    pub(crate) fn new(
        history: Option<CaptureHistoryStore>,
        pins: Option<WeakEntity<yue_pin::PinManager>>,
        cx: &mut Context<Self>,
    ) -> Self {
        let rows = Rc::new(Vec::new());
        let history_virtualizer = cx.new(|_| {
            VirtualizerState::new(
                Axis::Vertical,
                Rc::new(CaptureListSource::new(rows.clone())),
                VirtualizerSizing::known(Rc::<[Pixels]>::from([])),
            )
            .expect("empty capture history")
        });
        let mut view = Self {
            history,
            history_rows: rows,
            history_cursor: None,
            history_error: None,
            history_virtualizer,
            history_revision: 0,
            selected: None,
            pins,
            capture_busy: false,
            capture_operation: None,
            status: None,
        };
        view.reload_history(cx);
        view
    }
    fn selected_source(&self) -> Option<&yue_shot_model::CaptureSource> {
        self.history_rows
            .iter()
            .find(|row| Some(&row.id) == self.selected.as_ref())
            .map(|row| &row.source)
    }
    fn delete_selected(&mut self, cx: &mut Context<Self>) {
        if self.capture_busy {
            return;
        }
        let Some(id) = self.selected.clone() else {
            return;
        };
        match self
            .history
            .as_ref()
            .ok_or_else(|| "截图历史不可用".to_string())
            .and_then(|store| store.delete_capture(&id).map_err(|e| e.to_string()))
        {
            Ok(_) => self.reload_history(cx),
            Err(error) => self.status = Some(format!("删除失败：{error}")),
        }
        cx.notify();
    }
    fn export_capture(
        &mut self,
        format: yue_shot_capture::save_format::SaveFormat,
        cx: &mut Context<Self>,
    ) {
        if self.capture_busy {
            return;
        }
        let Some(source) = self.selected_source().cloned() else {
            return;
        };
        let selection = cx.prompt_for_new_path(
            std::path::Path::new(&crate::save::dirs::desktop()),
            Some(
                if format == yue_shot_capture::save_format::SaveFormat::Avif {
                    "YueShot-original.avif"
                } else {
                    "YueShot-original.png"
                },
            ),
        );
        self.capture_busy = true;
        self.capture_operation = Some(cx.spawn(async move |this, cx| {
            let result = match selection.await {
                Ok(Ok(Some(path))) => {
                    cx.background_spawn(async move {
                        crate::history_source::export_source(&source, &path).map(Some)
                    })
                    .await
                }
                Ok(Ok(None)) => Ok(None),
                error => Err(format!("无法选择保存路径：{error:?}")),
            };
            _ = this.update(cx, |view, cx| {
                view.capture_busy = false;
                view.status = match result {
                    Ok(Some(saved)) => Some(format!("已导出 {}", saved.path.display())),
                    Ok(None) => None,
                    Err(error) => Some(format!("原图导出失败：{error}")),
                };
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn capture_action(&mut self, action: CaptureAction, cx: &mut Context<Self>) {
        if self.capture_busy {
            return;
        }
        let Some(source) = self.selected_source().cloned() else {
            return;
        };
        let pins = self.pins.clone();
        let mut cursor = windows_sys::Win32::Foundation::POINT { x: 0, y: 0 };
        unsafe {
            windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos(&mut cursor);
        }
        let cursor = (cursor.x, cursor.y);
        self.capture_busy = true;
        self.status = Some("正在读取原始图片…".into());
        self.capture_operation = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    let image = crate::history_source::read_source(&source)?;
                    match action {
                        CaptureAction::Open => Ok(Some(image)),
                        CaptureAction::CopySdr => {
                            SystemClipboard.copy_pixels(image.pixels()).map(|_| None)
                        }
                        CaptureAction::CopyHdr => {
                            let bytes = crate::capture::pixel_export::encode_pixels(
                                image.pixels(),
                                crate::capture::pixel_export::ColorExport::PreserveHdr,
                            )?;
                            SystemClipboard.copy_hdr_png(&bytes).map(|_| None)
                        }
                    }
                })
                .await;
            _ = this.update(cx, |view, cx| {
                view.capture_busy = false;
                let result = result.and_then(|image| {
                    if let Some(image) = image {
                        let pins = pins
                            .and_then(|weak| weak.upgrade())
                            .ok_or("图片窗口服务不可用")?;
                        yue_pin::PinManager::open(
                            Arc::new(yue_pin::PinSnapshot::new(yue_pin::PinContent::Image(image))),
                            yue_pin::PinPlacement::Pointer(cursor),
                            &pins,
                            cx,
                        )
                        .map_err(|e| e.to_string())?;
                    }
                    Ok(())
                });
                view.status = Some(match result {
                    Ok(()) => match action {
                        CaptureAction::Open => "已打开原图".into(),
                        CaptureAction::CopySdr => "已复制 SDR 图片".into(),
                        CaptureAction::CopyHdr => "已复制 HDR PNG".into(),
                    },
                    Err(e) => format!("原图操作失败：{e}"),
                });
                cx.notify();
            });
        }));
        cx.notify();
    }

    pub(crate) fn reload_history(&mut self, cx: &mut Context<Self>) {
        self.load_history_page(None, true, cx);
    }

    fn load_history_page(&mut self, cursor: Option<String>, replace: bool, cx: &mut Context<Self>) {
        let Some(history) = &self.history else {
            self.history_error = Some("截图历史数据库不可用".into());
            return;
        };
        match history.history_page(cursor, Some(HISTORY_PAGE_SIZE)) {
            Ok(page) => self.adopt_history_page(page, replace, cx),
            Err(error) => self.history_error = Some(format!("读取截图历史失败：{error}")),
        }
    }

    fn adopt_history_page(
        &mut self,
        page: CaptureHistoryPage,
        replace: bool,
        cx: &mut Context<Self>,
    ) {
        let mut rows = if replace {
            Vec::new()
        } else {
            self.history_rows.as_ref().clone()
        };
        rows.extend(page.items);
        self.history_rows = Rc::new(rows);
        if !self
            .history_rows
            .iter()
            .any(|row| Some(&row.id) == self.selected.as_ref())
        {
            self.selected = self.history_rows.first().map(|row| row.id.clone());
        }
        self.history_cursor = page.next_cursor;
        self.history_error = None;
        self.history_revision = self.history_revision.wrapping_add(1);
        let source = Rc::new(CaptureListSource::new(self.history_rows.clone()));
        let sizing = VirtualizerSizing::known(Rc::<[Pixels]>::from(vec![
            HISTORY_ROW_HEIGHT;
            self.history_rows.len()
        ]));
        self.history_virtualizer.update(cx, |virtualizer, _| {
            virtualizer
                .replace_source(
                    source,
                    sizing,
                    VirtualizerChange::Replace {
                        preserve_anchor: !replace,
                    },
                )
                .expect("valid screenshot history geometry");
        });
    }
}

struct CaptureListSource {
    rows: Rc<Vec<CaptureHistoryEntry>>,
    positions: HashMap<ElementId, usize>,
}

impl CaptureListSource {
    fn new(rows: Rc<Vec<CaptureHistoryEntry>>) -> Self {
        let positions = rows
            .iter()
            .enumerate()
            .map(|(index, row)| (ElementId::from(row.id.clone()), index))
            .collect();
        Self { rows, positions }
    }
}

impl VirtualizerSource for CaptureListSource {
    fn len(&self) -> usize {
        self.rows.len()
    }
    fn key_at(&self, index: usize) -> ElementId {
        ElementId::from(self.rows[index].id.clone())
    }
    fn index_of(&self, key: &ElementId) -> Option<usize> {
        self.positions.get(key).copied()
    }
}

impl Render for CaptureHistoryView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = yue_component::current_theme(cx);
        let source = self.selected_source();
        let hdr = source.is_some_and(|s| {
            s.metadata.source.descriptor().encoding() == yue_media::pixels::PixelEncoding::ScRgb
        });
        let button = |id, label| {
            YueButton::text(id, theme, label)
                .size(ButtonSize::Small)
                .disabled(self.capture_busy || source.is_none())
        };
        let toolbar = div()
            .h_flex()
            .flex_wrap()
            .gap_2()
            .child(button("history-open-original", "查看原图…").on_click(
                cx.listener(|view, _, _, cx| view.capture_action(CaptureAction::Open, cx)),
            ))
            .child(
                button(
                    "history-export-original",
                    if hdr {
                        "导出 HDR PNG…"
                    } else {
                        "导出 PNG…"
                    },
                )
                .on_click(cx.listener(|view, _, _, cx| view.export_capture(SaveFormat::Png, cx))),
            )
            .child(button("history-copy-sdr", "复制 SDR").on_click(
                cx.listener(|view, _, _, cx| view.capture_action(CaptureAction::CopySdr, cx)),
            ))
            .when(hdr, |bar| {
                bar.child(button("history-copy-hdr", "复制 HDR PNG").on_click(
                    cx.listener(|view, _, _, cx| view.capture_action(CaptureAction::CopyHdr, cx)),
                ))
            })
            .when(hdr && cfg!(feature = "avif"), |bar| {
                bar.child(button("history-export-avif", "导出 AVIF…").on_click(
                    cx.listener(|view, _, _, cx| view.export_capture(SaveFormat::Avif, cx)),
                ))
            })
            .child(
                button("history-delete", "删除记录")
                    .on_click(cx.listener(|view, _, _, cx| view.delete_selected(cx))),
            );
        let rows = self.history_rows.clone();
        let owner = cx.entity().downgrade();
        let selected = self.selected.clone();
        let list = YueVirtualList::new(
            "capture-history-list",
            &self.history_virtualizer,
            move |item, _, _| {
                let Some(entry) = rows.get(item.index()) else {
                    return div().into_any_element();
                };
                let id = entry.id.clone();
                let name = entry
                    .source
                    .artifact
                    .absolute_path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "截图".into());
                let label = format!(
                    "{} × {} · {}",
                    entry.source.metadata.width, entry.source.metadata.height, name
                );
                let owner = owner.clone();
                div()
                    .h(HISTORY_ROW_HEIGHT)
                    .h_flex()
                    .w_full()
                    .px_3()
                    .when(selected.as_ref() == Some(&id), |row| {
                        row.bg(theme.selection())
                    })
                    .child(
                        YueButton::text(format!("select-{id}"), theme, label)
                            .accessibility_description(if selected.as_ref() == Some(&id) {
                                "已选中"
                            } else {
                                "选择截图"
                            })
                            .on_click(move |_, _, cx| {
                                let _ = owner.update(cx, |view, cx| {
                                    view.selected = Some(id.clone());
                                    cx.notify();
                                });
                            }),
                    )
                    .into_any_element()
            },
        )
        .content_revision(self.history_revision)
        .accessibility_label("截图历史");
        div()
            .v_flex()
            .size_full()
            .gap_3()
            .child(
                div()
                    .h_flex()
                    .justify_between()
                    .child(div().child("截图历史"))
                    .child(
                        YueButton::new("refresh-captures", "刷新")
                            .on_click(cx.listener(|view, _, _, cx| view.reload_history(cx))),
                    ),
            )
            .child(toolbar)
            .when_some(
                self.status.clone().or(self.history_error.clone()),
                |view, status| view.child(div().id("history-status").text_sm().child(status)),
            )
            .when(self.history_rows.is_empty(), |view| {
                view.child(div().text_sm().child("还没有截图，按 F5 开始截图"))
            })
            .child(div().flex_1().min_h_0().child(list))
            .when_some(self.history_cursor.clone(), |view, cursor| {
                view.child(
                    YueButton::new("load-more-captures", "加载更多").on_click(cx.listener(
                        move |view, _, _, cx| {
                            view.load_history_page(Some(cursor.clone()), false, cx)
                        },
                    )),
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use super::{CaptureHistoryStore, CaptureHistoryView};
    use gpui_kit::{AppContext as _, TestAppContext};
    #[gpui_kit::test]
    fn screenshot_pages_and_selection_remain_distinct_after_refresh_and_delete(
        cx: &mut TestAppContext,
    ) {
        let root =
            std::env::temp_dir().join(format!("yueshot-history-pages-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let store = CaptureHistoryStore::open(&root).unwrap();
        let pixels = yue_media::pixels::PixelBuffer::from_tight(
            1,
            1,
            yue_media::pixels::PixelDescriptor::bgra8(),
            vec![1, 2, 3, 255],
        )
        .unwrap();
        let writer = crate::capture::HistoryArtifactWriter::new(store.artifact_root())
            .with_history(store.clone());
        writer
            .write_pixels(
                &yue_shot_model::CaptureMetadata {
                    session_id: "paging".into(),
                    width: 1,
                    height: 1,
                    dpi: 96,
                    source: yue_media::pixels::PixelSource::new(pixels.descriptor(), 1),
                    captured_at_unix_ms: 42,
                    monitor_device_name: None,
                },
                &pixels,
            )
            .unwrap();
        let first = store.history_page(None, None).unwrap().items.remove(0);
        for ix in 0..54 {
            store
                .save_capture(format!("extra-{ix:02}"), first.source.clone())
                .unwrap();
        }
        let view = cx.new(|cx| CaptureHistoryView::new(Some(store.clone()), None, cx));
        cx.update(|cx| {
            view.update(cx, |view, cx| {
                assert_eq!(view.history_rows.len(), 50);
                let selected = view.selected.clone();
                let cursor = view.history_cursor.clone().unwrap();
                view.load_history_page(Some(cursor), false, cx);
                assert_eq!(view.history_rows.len(), 55);
                assert!(view.history_cursor.is_none());
                assert_eq!(view.selected, selected);
                assert_eq!(
                    view.history_rows
                        .iter()
                        .map(|row| &row.id)
                        .collect::<std::collections::HashSet<_>>()
                        .len(),
                    55
                );
                view.delete_selected(cx);
                assert!(
                    first.source.artifact.absolute_path.exists(),
                    "remaining references retain the source"
                );
                assert_ne!(view.selected, selected);
                assert_eq!(store.history_page(None, Some(100)).unwrap().items.len(), 54);
            })
        });
        drop(view);
        drop(store);
        std::fs::remove_dir_all(root).unwrap();
    }
}
