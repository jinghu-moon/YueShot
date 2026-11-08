//! Product orchestration over prepared screenshot sources; no clipboard classification.
use crate::pin_source::PinSource;
use gpui_kit::*;
use std::sync::Arc;
use yue_clipboard::SystemClipboard;
use yue_pin::{CopyTarget, PinCommand, PinContent, PinManager};

pub fn open_selection(
    source: &Arc<PinSource>,
    manager: &Entity<PinManager>,
    position: (i32, i32),
    cx: &mut App,
) {
    let Some(claim) = source.claim() else {
        yue_pin::show_feedback(
            if source.has_pending() {
                "正在创建贴图，请稍候"
            } else {
                "还没有可贴的截图，请先按 F5 框选"
            },
            position,
            cx,
        );
        return;
    };
    if let Err(error) =
        PinManager::open_selection(claim.selection().clone(), manager, cx, move || {
            if let Err(error) = claim.commit() {
                eprintln!("[yue-shot][pin] could not acknowledge source: {error}");
            }
        })
    {
        yue_pin::show_feedback(error.to_string(), position, cx);
    }
}

pub fn drain_copies(
    manager: &Entity<PinManager>,
    commands: async_channel::Receiver<PinCommand>,
    cx: &mut App,
) {
    let manager = manager.downgrade();
    cx.spawn(async move |cx| {
        while let Ok(PinCommand::Copy { id, target }) = commands.recv().await {
            let result = cx.background_spawn(async move { copy(target) }).await;
            if manager
                .update(cx, |manager, cx| manager.report_copy(id, result, cx))
                .is_err()
            {
                break;
            }
        }
    })
    .detach();
}

fn copy(target: CopyTarget) -> Result<(), String> {
    match target {
        CopyTarget::Text(text) => SystemClipboard.copy_text(&text),
        CopyTarget::Files(paths) => SystemClipboard.copy_files(&paths),
        CopyTarget::Pixels(image) => SystemClipboard.copy_pixels(image.pixels()),
        CopyTarget::HdrPixels(image) => {
            let bytes = crate::capture::pixel_export::encode_pixels(
                image.pixels(),
                crate::capture::pixel_export::ColorExport::PreserveHdr,
            )?;
            SystemClipboard.copy_hdr_png(&bytes)
        }
        CopyTarget::Content(snapshot) => match snapshot.content() {
            PinContent::Image(image) => SystemClipboard.copy_pixels(image.pixels()),
            PinContent::Document(doc) => {
                SystemClipboard.copy_text_with_sources(doc.text(), snapshot.sources())
            }
            PinContent::Files(files) => SystemClipboard.copy_files(
                &files
                    .iter()
                    .map(|file| file.path().to_owned())
                    .collect::<Vec<_>>(),
            ),
        },
    }
}
