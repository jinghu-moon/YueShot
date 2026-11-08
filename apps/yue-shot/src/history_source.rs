//! Application composition for durable capture sources and full-precision replay.
//! The database stores references; windows retain immutable Arc pixels separately.
use std::{
    io::Read,
    sync::{Arc, Mutex},
};
use yue_media::pixels::PixelBuffer;
use yue_pin_model::PinImage;
use yue_shot_model::{CaptureMetadata, CaptureSource};
use yue_shot_storage::capture_history::CaptureHistoryStore;
use yue_shot_storage::pixel_master::PixelMaster;

/// Shared by clones of one capture panel; retries archive again only after failure.
pub(crate) struct CaptureArchive {
    metadata: CaptureMetadata,
    writer: crate::capture::HistoryArtifactWriter,
    stored: Mutex<bool>,
    events: Option<async_channel::Sender<()>>,
}
impl CaptureArchive {
    pub(crate) fn new(store: CaptureHistoryStore, metadata: CaptureMetadata) -> Self {
        Self {
            metadata,
            writer: crate::capture::HistoryArtifactWriter::new(store.artifact_root())
                .with_history(store),
            stored: Mutex::new(false),
            events: None,
        }
    }
    pub(crate) fn with_refresh(mut self, events: async_channel::Sender<()>) -> Self {
        self.events = Some(events);
        self
    }
    pub(crate) fn persist(&self, pixels: &PixelBuffer) -> Result<(), String> {
        let mut stored = self.stored.lock().map_err(|e| e.to_string())?;
        if !*stored {
            self.writer
                .write_pixels(&self.metadata, pixels)
                .map_err(|e| e.to_string())?;
            *stored = true;
            if let Some(events) = &self.events {
                let _ = events.try_send(());
            }
        }
        Ok(())
    }
    pub(crate) fn persist_bands(
        &self,
        spec: &yue_shot_capture::ports::SelectionSpec,
        bands: &mut dyn yue_media::pixels::BandSource,
    ) -> Result<(), String> {
        use yue_shot_capture::ports::ArtifactWriter;
        if spec.region.width() as u32 != self.metadata.width
            || spec.region.height() as u32 != self.metadata.height
            || spec.source != self.metadata.source
            || spec.captured_at_unix_ms != self.metadata.captured_at_unix_ms
        {
            return Err("长截图原图与归档元数据不一致".into());
        }
        let mut stored = self.stored.lock().map_err(|e| e.to_string())?;
        if !*stored {
            self.writer
                .write_bands(
                    &self.metadata.session_id,
                    spec,
                    bands,
                    self.metadata.dpi,
                    self.metadata.monitor_device_name.clone(),
                )
                .map_err(|e| e.to_string())?;
            *stored = true;
            if let Some(events) = &self.events {
                let _ = events.try_send(());
            }
        }
        Ok(())
    }
}

/// Full source only. A row thumbnail is never a substitute on missing/corrupt or
/// over-budget input. A rejected interactive read leaves its durable master intact.
pub(crate) fn read_source(source: &CaptureSource) -> Result<PinImage, String> {
    const MAX_BYTES: u64 = 256 * 1024 * 1024;
    let metadata = &source.metadata;
    let row = metadata
        .source
        .descriptor()
        .row_bytes(metadata.width)
        .ok_or("history source row overflow")?;
    let len = (row as u64)
        .checked_mul(u64::from(metadata.height))
        .filter(|n| *n > 0 && *n <= MAX_BYTES)
        .ok_or(
            "原始图片超过 256 MiB 交互预算，无法打开或复制；历史保真源仍保留，缩略图不会代替原图",
        )?;
    let pixels = match source.artifact.mime.as_str() {
        "application/x-snapclip-pixels" => {
            let mut master = PixelMaster::open(&source.artifact).map_err(|e| e.to_string())?;
            if master.metadata() != metadata {
                return Err("history metadata differs from its master".into());
            }
            let mut bytes = Vec::new();
            bytes
                .try_reserve_exact(len as usize)
                .map_err(|e| e.to_string())?;
            let band_rows = (4 * 1024 * 1024 / row).max(1) as u32;
            let mut first = 0;
            while first < metadata.height {
                let count = (metadata.height - first).min(band_rows);
                bytes.extend_from_slice(
                    master
                        .read_rows(first, count)
                        .map_err(|e| e.to_string())?
                        .as_bytes(),
                );
                first += count;
            }
            PixelBuffer::from_tight(
                metadata.width,
                metadata.height,
                metadata.source.descriptor(),
                bytes,
            )
            .map_err(str::to_owned)?
        }
        "image/png" => {
            if source.artifact.byte_len > 64 * 1024 * 1024 {
                return Err("history PNG exceeds 64 MiB encoded budget".into());
            }
            let mut bytes = Vec::new();
            std::fs::File::open(&source.artifact.absolute_path)
                .map_err(|e| e.to_string())?
                .take(64 * 1024 * 1024 + 1)
                .read_to_end(&mut bytes)
                .map_err(|e| e.to_string())?;
            if bytes.len() as u64 != source.artifact.byte_len
                || blake3::hash(&bytes).to_hex().as_str() != source.artifact.content_fingerprint
            {
                return Err("history PNG identity differs from its immutable reference".into());
            }
            let image = yue_pin::decode_image(&bytes)?;
            if image.pixels().size() != [metadata.width, metadata.height]
                || image.pixels().descriptor() != metadata.source.descriptor()
            {
                return Err("history PNG differs from its recorded capture".into());
            }
            return PinImage::from_pixels(metadata.source, image.pixels().clone())
                .ok_or("invalid history source encoding".into());
        }
        _ => return Err("unsupported history capture source".into()),
    };
    PinImage::from_pixels(metadata.source, Arc::new(pixels))
        .ok_or("invalid history source encoding".into())
}

/// Export from the durable source in bounded bands. Resolve the actual save
/// target before checking identity: save validation may trim the chosen stem.
pub(crate) fn export_source(
    source: &CaptureSource,
    path: &std::path::Path,
) -> Result<crate::save::service::SaveOutcome, String> {
    use crate::save::service::{ConflictPolicy, NoCancel, SaveRequest, commit_stream, target_of};
    let format = match path.extension().and_then(|v| v.to_str()) {
        Some(ext) if ext.eq_ignore_ascii_case("png") => {
            yue_shot_capture::save_format::SaveFormat::Png
        }
        #[cfg(feature = "avif")]
        Some(ext) if ext.eq_ignore_ascii_case("avif") => {
            if source.metadata.source.descriptor().encoding()
                != yue_media::pixels::PixelEncoding::ScRgb
            {
                return Err("AVIF 当前仅支持 HDR 保真源".into());
            }
            yue_shot_capture::save_format::SaveFormat::Avif
        }
        _ => return Err("请选择当前构建支持的 .png 文件名或 HDR .avif 文件名".into()),
    };
    crate::save::caps::check(
        format,
        u64::from(source.metadata.width),
        u64::from(source.metadata.height),
    )
    .map_err(|e| format!("{e:?}"))?;
    let request = SaveRequest {
        directory: path.parent().ok_or("保存路径无父目录")?.to_path_buf(),
        stem: path
            .file_stem()
            .ok_or("保存路径无文件名")?
            .to_string_lossy()
            .into_owned(),
        format,
        conflict: ConflictPolicy::Overwrite,
    };
    let target = target_of(&request).map_err(|e| e.message())?;
    if let (Ok(target), Ok(original)) = (
        target.canonicalize(),
        source.artifact.absolute_path.canonicalize(),
    ) && target == original
    {
        return Err("不能覆盖历史保真源，请选择其他路径".into());
    }
    commit_stream(&request, &mut NoCancel, |file| {
        #[cfg(feature = "avif")]
        if format == yue_shot_capture::save_format::SaveFormat::Avif {
            return crate::save::avif::write_source(
                source,
                file,
                yue_avif::StopToken::new(yue_avif::Unstoppable),
            );
        }
        crate::capture::pixel_export::write_capture_source_png(source, file)
            .map_err(std::io::Error::other)
    })
    .map_err(|e| e.message())
}

#[cfg(test)]
#[path = "history_source_tests.rs"]
mod tests;
