use crate::{
    AssetSource, DevicePixels, IsZero, RenderImage, Result, SharedString, Size,
    swap_rgba_pa_to_bgra,
};
use image::Frame;
use resvg::tiny_skia::Pixmap;
use smallvec::SmallVec;
use std::{hash::Hash, sync::Arc};

/// When rendering SVGs, we render them at twice the size to get a higher-quality result.
pub const SMOOTH_SVG_SCALE_FACTOR: f32 = 2.;

#[derive(Clone, PartialEq, Hash, Eq)]
#[expect(missing_docs)]
pub struct RenderSvgParams {
    pub path: SharedString,
    pub size: Size<DevicePixels>,
}

#[derive(Clone)]
/// A struct holding everything necessary to render SVGs.
pub struct SvgRenderer {
    asset_source: Arc<dyn AssetSource>,
    usvg_options: Arc<usvg::Options<'static>>,
}

/// An error while parsing or rasterizing a graphical SVG.
#[derive(Debug, thiserror::Error)]
pub enum SvgError {
    /// An SVG parsing or rasterization failure.
    #[error(transparent)]
    Parse(#[from] usvg::Error),
    /// Live text requires font discovery, which this host deliberately excludes.
    #[error("SVG text is unsupported; convert text to paths when authoring the asset")]
    UnsupportedText,
}

/// A parsed SVG document that can be rasterized at any scale.
///
/// Produced by [`SvgRenderer::parse_svg`] and rasterized by
/// [`SvgRenderer::render_parsed`]. This host renders graphical SVGs only; text
/// must be converted to paths when authoring an asset. Retain this value when
/// rasterizing the same SVG at multiple scales to avoid repeated parsing.
pub struct ParsedSvg(usvg::Tree);

/// The size in which to rasterize the SVG.
#[derive(Clone, Copy)]
pub enum SvgSize {
    /// A width in device pixels. The SVG retains its aspect ratio.
    Size(Size<DevicePixels>),
    /// An exact width and height in device pixels.
    ExactSize(Size<DevicePixels>),
    /// A logical scaling factor to apply to the size provided by the SVG.
    ScaleFactor(f32),
}

impl From<f32> for SvgSize {
    fn from(scale_factor: f32) -> Self {
        Self::ScaleFactor(scale_factor)
    }
}

impl SvgRenderer {
    /// Creates a new SVG renderer with the provided asset source.
    pub fn new(asset_source: Arc<dyn AssetSource>) -> Self {
        Self {
            asset_source,
            usvg_options: Arc::new(usvg::Options::default()),
        }
    }

    /// Parses SVG data into a [`ParsedSvg`] that can be rasterized at any scale.
    pub fn parse_svg(&self, bytes: &[u8]) -> Result<ParsedSvg, SvgError> {
        // Match usvg's XML parser without parsing the XML twice. SVGZ remains
        // disabled, as in the upstream host's explicit dependency features.
        if bytes.starts_with(&[0x1f, 0x8b]) {
            return Err(usvg::Error::SvgzFeatureNotEnabled.into());
        }
        let text = std::str::from_utf8(bytes).map_err(|_| usvg::Error::NotAnUtf8Str)?;
        let document = usvg::roxmltree::Document::parse_with_options(
            text,
            usvg::roxmltree::ParsingOptions {
                allow_dtd: true,
                ..Default::default()
            },
        )
        .map_err(usvg::Error::ParsingFailed)?;
        if document.descendants().any(|node| {
            node.tag_name().namespace() == Some("http://www.w3.org/2000/svg")
                && matches!(node.tag_name().name(), "text" | "tspan" | "textPath")
        }) {
            return Err(SvgError::UnsupportedText);
        }
        usvg::Tree::from_xmltree(&document, &self.usvg_options)
            .map(ParsedSvg)
            .map_err(Into::into)
    }

    /// Rasterizes a previously parsed SVG into an image buffer.
    pub fn render_parsed(
        &self,
        svg: &ParsedSvg,
        size: impl Into<SvgSize>,
    ) -> Result<Arc<RenderImage>, SvgError> {
        let (size, image_scale_factor) = match size.into() {
            SvgSize::Size(size) => (SvgSize::Size(size), 1.0),
            SvgSize::ExactSize(size) => (SvgSize::ExactSize(size), 1.0),
            SvgSize::ScaleFactor(scale_factor) => (
                SvgSize::ScaleFactor(scale_factor * SMOOTH_SVG_SCALE_FACTOR),
                SMOOTH_SVG_SCALE_FACTOR,
            ),
        };
        let pixmap = rasterize_tree(&svg.0, size)?;
        let mut buffer =
            image::ImageBuffer::from_raw(pixmap.width(), pixmap.height(), pixmap.take()).unwrap();

        for pixel in buffer.chunks_exact_mut(4) {
            swap_rgba_pa_to_bgra(pixel);
        }

        let mut image = RenderImage::new(SmallVec::from_const([Frame::new(buffer)]));
        image.scale_factor = image_scale_factor;
        Ok(Arc::new(image))
    }

    /// Renders the given bytes into an image buffer.
    pub fn render_single_frame(
        &self,
        bytes: &[u8],
        scale_factor: f32,
    ) -> Result<Arc<RenderImage>, SvgError> {
        let svg = self.parse_svg(bytes)?;
        self.render_parsed(&svg, scale_factor)
    }

    pub(crate) fn render_alpha_mask(
        &self,
        params: &RenderSvgParams,
        bytes: Option<&[u8]>,
    ) -> Result<Option<(Size<DevicePixels>, Vec<u8>)>> {
        anyhow::ensure!(!params.size.is_zero(), "can't render at a zero size");

        let render_pixmap = |bytes| {
            let pixmap = self.render_pixmap(bytes, SvgSize::Size(params.size))?;

            // Convert the pixmap's pixels into an alpha mask.
            let size = Size::new(
                DevicePixels(pixmap.width() as i32),
                DevicePixels(pixmap.height() as i32),
            );
            let alpha_mask = pixmap
                .pixels()
                .iter()
                .map(|p| p.alpha())
                .collect::<Vec<_>>();

            Ok(Some((size, alpha_mask)))
        };

        if let Some(bytes) = bytes {
            render_pixmap(bytes)
        } else if let Some(bytes) = self.asset_source.load(&params.path)? {
            render_pixmap(&bytes)
        } else {
            Ok(None)
        }
    }

    fn render_pixmap(&self, bytes: &[u8], size: SvgSize) -> Result<Pixmap, SvgError> {
        let svg = self.parse_svg(bytes)?;
        rasterize_tree(&svg.0, size).map_err(Into::into)
    }
}

fn rasterize_tree(tree: &usvg::Tree, size: SvgSize) -> Result<Pixmap, usvg::Error> {
    // Cap the size of the rendered pixmap to avoid texture allocation panics
    // Related issue: #56466
    const MAX_SIZE: f32 = 8192.0;

    let svg_size = tree.size();
    let (mut width, mut height) = match size {
        SvgSize::Size(size) => {
            let scale = i32::from(size.width) as f32 / svg_size.width();
            (svg_size.width() * scale, svg_size.height() * scale)
        }
        SvgSize::ExactSize(size) => (i32::from(size.width) as f32, i32::from(size.height) as f32),
        SvgSize::ScaleFactor(scale) => (svg_size.width() * scale, svg_size.height() * scale),
    };

    if width > MAX_SIZE {
        log::warn!("Attempted to render pixmap where width ({width}) > MAX_SIZE ({MAX_SIZE})");
    }
    if height > MAX_SIZE {
        log::warn!("Attempted to render pixmap where height ({height}) > MAX_SIZE ({MAX_SIZE})");
    }
    let scale = (MAX_SIZE / width).min(MAX_SIZE / height).min(1.0);
    width *= scale;
    height *= scale;

    // Render the SVG to a pixmap with the specified width and height.
    let mut pixmap = resvg::tiny_skia::Pixmap::new(width as u32, height as u32)
        .ok_or(usvg::Error::InvalidSize)?;

    let transform = resvg::tiny_skia::Transform::from_scale(
        width / svg_size.width(),
        height / svg_size.height(),
    );

    resvg::render(tree, transform, &mut pixmap.as_mut());

    Ok(pixmap)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_parsed_svg_at_requested_size() -> Result<()> {
        let renderer = SvgRenderer::new(Arc::new(()));
        let svg = renderer.parse_svg(
            br#"<svg xmlns="http://www.w3.org/2000/svg" width="24pt" height="12pt"></svg>"#,
        )?;
        let requested_size = Size::new(DevicePixels(24), DevicePixels(12));
        let image = renderer.render_parsed(&svg, SvgSize::ExactSize(requested_size))?;

        assert_eq!(image.size(0), requested_size);
        Ok(())
    }

    #[test]
    fn preserves_aspect_ratio_for_width_constrained_size() -> Result<()> {
        let renderer = SvgRenderer::new(Arc::new(()));
        let svg = renderer.parse_svg(
            br#"<svg xmlns="http://www.w3.org/2000/svg" width="24pt" height="12pt"></svg>"#,
        )?;
        let image = renderer.render_parsed(
            &svg,
            SvgSize::Size(Size::new(DevicePixels(24), DevicePixels(24))),
        )?;

        assert_eq!(image.size(0), Size::new(DevicePixels(24), DevicePixels(12)));
        Ok(())
    }

    #[test]
    fn rejects_live_svg_text_instead_of_omitting_it() {
        let renderer = SvgRenderer::new(Arc::new(()));
        for tag in ["text", "tspan", "textPath"] {
            let svg = format!(
                r#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20"><{tag}>YueShot</{tag}></svg>"#
            );
            assert!(matches!(
                renderer.parse_svg(svg.as_bytes()),
                Err(SvgError::UnsupportedText)
            ));
            let params = RenderSvgParams {
                path: "unsupported-text.svg".into(),
                size: Size::new(DevicePixels(20), DevicePixels(20)),
            };
            assert!(
                renderer
                    .render_alpha_mask(&params, Some(svg.as_bytes()))
                    .is_err()
            );
        }
    }
}
