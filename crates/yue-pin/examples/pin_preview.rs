//! Production pin views with local fixtures. Copy feedback is simulated;
//! this example never reads or writes the system clipboard.
//! `hdr4k` exercises FP16 display; `hdr45mp` currently exceeds the pin budget.
use gpui_kit::*;
use std::{cell::Cell, process::ExitCode, rc::Rc, sync::Arc};
use yue_media::{
    PixelFormat,
    pixels::{AlphaMode, PixelBuffer, PixelDescriptor, PixelEncoding, PixelSource},
};
use yue_pin::{
    DocumentMode, FileKind, PinCommand, PinContent, PinDocument, PinFile, PinImage, PinManager,
    PinPlacement, PinSnapshot, PinTable,
};

struct Preview {
    _owner: Entity<PinManager>,
}
impl Global for Preview {}

fn sample(mode: &str) -> PinSnapshot {
    let content = match mode {
        "text" => PinContent::Document(PinDocument::new(
            "对照阅读\n\n中文、English、emoji 😀。\n\n  空格与\tTab 应保留；正文可以选择和复制。\r\n".into(),
            DocumentMode::Text,
        )),
        "code" => PinContent::Document(PinDocument::new(
            "fn main() {\n    let title = \"剪贴板贴图\";\n    println!(\"{title}\");\n}\n".into(),
            DocumentMode::Code,
        )),
        "table" | "table-large" => {
            let rows = if mode == "table-large" {
                std::iter::once((0..8).map(|col| format!("列 {col}")).collect())
                    .chain((0..2000).map(|row| {
                        (0..8).map(|col| format!("{row}:{col}")).collect()
                    }))
                    .collect::<Vec<Vec<String>>>()
            } else {
                vec![
                    vec!["项目".into(), "数量".into(), "占比".into()],
                    vec!["图片".into(), "123".into(), "50%".into()],
                    vec!["文档".into(), "42".into(), "30%".into()],
                    vec!["文件".into(), String::new(), "20%".into()],
                ]
            };
            let text = rows.iter().map(|row| row.join("\t")).collect::<Vec<_>>().join("\n");
            PinContent::Document(PinDocument::new(text, DocumentMode::Table)
                .with_table(PinTable::new(rows, true).unwrap()))
        }
        "link" => PinContent::Document(PinDocument::new(
            "https://example.com/design?view=clipboard-pin".into(), DocumentMode::Link,
        )),
        "path" => PinContent::Document(PinDocument::new(
            r"C:\Samples\项目交付\设计规范.pdf".into(), DocumentMode::Path,
        )),
        "files" => PinContent::Files(Arc::new(vec![
            PinFile::new(r"C:\Samples\产品设计规范.pdf".into(), "产品设计规范.pdf".into(), FileKind::Document).with_size(1024 * 230),
            PinFile::new(r"C:\Samples\设计素材".into(), "设计素材".into(), FileKind::Folder),
            PinFile::new(r"C:\Samples\archive.zip".into(), "archive.zip".into(), FileKind::Archive).unavailable("文件已移动或删除（预览样本）".into()),
        ])),
        "image" => {
            let (w, h) = (480, 320);
            let rgba = (0..h).flat_map(|y| (0..w).flat_map(move |x| [
                (x * 255 / w) as u8, (y * 255 / h) as u8, 160, if x < 80 { 96 } else { 255 },
            ])).collect();
            PinContent::Image(PinImage::new(w, h, rgba).unwrap())
        }
        "hdr4k" | "hdr45mp" => {
            let (w, h) = if mode == "hdr4k" { (3840, 2160) } else { (8256, 5504) };
            let descriptor = PixelDescriptor::new(
                PixelFormat::Rgba16Float, PixelEncoding::ScRgb, AlphaMode::Straight,
            ).unwrap();
            let mut bytes = Vec::with_capacity(w as usize * h as usize * 8);
            for y in 0..h {
                for x in 0..w {
                    let channels = if x < w / 16 {
                        [-0.25, 1., 1., 0.5]
                    } else if x >= w * 15 / 16 {
                        [12.5, 12.5, 12.5, 1.]
                    } else {
                        [4. * x as f32 / w as f32, 2. * y as f32 / h as f32, 0.5, 1.]
                    };
                    for value in channels {
                        bytes.extend_from_slice(&half::f16::from_f32(value).to_le_bytes());
                    }
                }
            }
            let pixels = Arc::new(PixelBuffer::from_tight(w, h, descriptor, bytes).unwrap());
            PinContent::Image(PinImage::from_pixels(PixelSource::new(descriptor, 1), pixels).unwrap())
        }
        "rich" => PinContent::Document(PinDocument::new(
            "对照阅读\n\n原生文字可以选择，滚动与窗口移动各有入口。\n\n中文、English、emoji 😀 都应正常显示。\n\n快捷键：Ctrl+A 全选、Ctrl+C 复制选区、Esc 关闭。".into(),
            DocumentMode::Rich,
        ).with_html("<h3>对照阅读</h3><p>原生文字可以<strong>选择</strong>，滚动与窗口移动各有入口。</p><p>中文、English、emoji 😀 都应正常显示。</p><ul><li>Ctrl+A 全选</li><li>Ctrl+C 复制选区</li><li>Esc 关闭</li></ul>".into())),
        _ => panic!("unknown sample: {mode}; choose text, rich, table, table-large, code, link, path, files, image, hdr4k or hdr45mp"),
    };
    PinSnapshot::new(content)
}

fn main() -> ExitCode {
    let mode = std::env::args().nth(1).unwrap_or_else(|| "rich".into());
    let snapshot = Arc::new(sample(&mode));
    eprintln!(
        "[pin-preview] {mode}: 复制仅模拟成功反馈，不修改系统剪贴板；打开命令仍由生产代码执行。"
    );
    let rejected = Rc::new(Cell::new(false));
    let report_rejection = rejected.clone();
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(move |cx| {
            gpui_kit::init(cx);
            let dark = std::env::var_os("SNAPCLIP_PIN_DARK").is_some()
                || matches!(
                    cx.window_appearance(),
                    WindowAppearance::Dark | WindowAppearance::VibrantDark
                );
            yue_component::init(
                cx,
                yue_component::YueTheme::new(
                    if dark {
                        yue_component::ThemeMode::Dark
                    } else {
                        yue_component::ThemeMode::Light
                    },
                    yue_component::Accent::Azure,
                ),
            )
            .unwrap();
            yue_pin::init(cx);
            let (manager, commands) = PinManager::new().unwrap();
            let owner = cx.new(|_| manager);
            if let Err(error) =
                PinManager::open(snapshot, PinPlacement::Pointer((200, 200)), &owner, cx)
            {
                eprintln!("[pin-preview] preview refused: {error}");
                report_rejection.set(true);
                cx.quit();
                return;
            }
            let copy_owner = owner.downgrade();
            cx.spawn(async move |cx| {
                while let Ok(PinCommand::Copy { id, .. }) = commands.recv().await {
                    if copy_owner
                        .update(cx, |manager, cx| manager.report_copy(id, Ok(()), cx))
                        .is_err()
                    {
                        break;
                    }
                }
            })
            .detach();
            cx.set_global(Preview { _owner: owner });
        });
    if rejected.get() {
        ExitCode::from(2)
    } else {
        ExitCode::SUCCESS
    }
}
