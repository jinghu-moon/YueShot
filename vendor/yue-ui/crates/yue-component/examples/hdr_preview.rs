//! Opt-in native-window diagnostic; never launched by the test suite.
//! No file argument: original synthetic 0 / 80 / 203 / 1000-nit bands.
//! One PNG argument explicitly opts this diagnostic into a 256-MiB file budget.
use std::{fs::File, io::BufReader};
use yue_component::gpui_kit::{
    self, AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render, Styled as _,
    Window, WindowBounds, WindowColorMode, WindowColorPolicy, WindowOptions, div, px, size,
};
use yue_component::{
    Accent, ImagePreviewColorSpace, ImagePreviewDocument, ImagePreviewImage, ImagePreviewLimits,
    ImagePreviewLoad, ImagePreviewPixels, ThemeMode, YueImagePreview, YueImagePreviewState,
    YueTheme,
};

struct Preview {
    state: Entity<YueImagePreviewState>,
}
impl Render for Preview {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = yue_component::current_theme(cx);
        div()
            .size_full()
            .p_4()
            .bg(gpui_kit::rgb(theme.page()))
            .text_color(gpui_kit::rgb(theme.text_primary()))
            .child(YueImagePreview::new(&self.state).footer(|window, _| {
                let output = window.color_output();
                let mode = match output.mode {
                    WindowColorMode::Sdr => "SDR",
                    WindowColorMode::LinearSdr => "SDR（FP16）",
                    WindowColorMode::HdrScRgb => "HDR（scRGB）",
                };
                div()
                    .flex()
                    .flex_col()
                    .child(format!(
                        "输出：{mode}；系统：{:?}；SDR 白：{:?} nit；峰值：{:?} nit；代次：{}",
                        output.display_kind,
                        output.sdr_white_nits,
                        output.max_luminance_nits,
                        output.generation
                    ))
                    .children(
                        output
                            .fallback_reason
                            .map(|reason| div().child(format!("回退原因：{reason}"))),
                    )
                    .into_any_element()
            }))
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let limits = ImagePreviewLimits::default().max_encoded_bytes(256 * 1024 * 1024);
    // Preparation is outside GPUI's render/update loop. This is a color probe,
    // not a load-latency benchmark or an application's asynchronous file picker.
    let pixels = if let Some(path) = std::env::args().nth(1) {
        ImagePreviewPixels::from_png_reader(BufReader::new(File::open(path)?), &limits)?
    } else {
        let mut bytes = Vec::with_capacity(512 * 96 * 8);
        for _ in 0..96 {
            for x in 0..512 {
                let v = [0., 1., 2.5375, 12.5][x / 128];
                for c in [v, v, v, 1.] {
                    bytes.extend(half::f16::from_f32(c).to_bits().to_le_bytes());
                }
            }
        }
        ImagePreviewPixels::from_rgba16f(512, 96, 512 * 8, bytes, ImagePreviewColorSpace::ScRgb)?
    };
    let source = pixels.into_preview(&limits, "HDR 原始显示资源")?;
    // A separate explicit SDR derivative permits Source/Compare/Result inspection
    // without relabeling the same eight-bit buffer as HDR.
    let original = source.image();
    // A proxy is never an export source; avoid a second content-peak estimate
    // from its blurred pixels. Large-file diagnostics show Source only.
    let result = if source.color_space() == ImagePreviewColorSpace::ScRgb && !source.is_proxy() {
        let [w, h] = source.texture_size();
        let rgba = ImagePreviewPixels::from_rgba16f(
            w,
            h,
            w as usize * 8,
            original.as_bytes(0).unwrap(),
            ImagePreviewColorSpace::ScRgb,
        )?
        .into_sdr_rgba8();
        Some(
            ImagePreviewImage::from_rgba8(w, h, rgba, "SDR 显示派生结果")?
                .with_source_size(source.pixel_size())?,
        )
    } else {
        None
    };
    gpui_kit::application().run(move |cx| {
        yue_component::init(cx, YueTheme::new(ThemeMode::Dark, Accent::Azure))
            .expect("initialize Yue");
        gpui_kit::open_window(
            WindowOptions {
                color_policy: WindowColorPolicy::AutomaticHdr,
                window_bounds: Some(WindowBounds::centered(size(px(1080.), px(760.)), cx)),
                ..Default::default()
            },
            cx,
            move |window, cx| {
                cx.new(|cx| {
                    let state =
                        cx.new(|cx| YueImagePreviewState::new("hdr-diagnostic", window, cx));
                    let mut document = ImagePreviewDocument::new("hdr-diagnostic", 1)
                        .with_source(ImagePreviewLoad::Ready(source));
                    if let Some(result) = result {
                        document = document
                            .with_result(ImagePreviewLoad::Ready(result))
                            .full_frame(true);
                    }
                    state
                        .update(cx, |state, cx| state.set_document(document, window, cx))
                        .expect("valid HDR document");
                    Preview { state }
                })
            },
        )
        .expect("open HDR diagnostic");
    });
    Ok(())
}
