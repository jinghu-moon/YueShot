//! Capture lifecycle notifications emitted by the capture capability.

use yue_shot_model::{ArtifactRef, CaptureState, ErrorCode};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureEvent {
    pub session_id: String,
    pub state: CaptureState,
    pub artifact: Option<ArtifactRef>,
    pub error_code: Option<ErrorCode>,
    pub generation: u64,
}
