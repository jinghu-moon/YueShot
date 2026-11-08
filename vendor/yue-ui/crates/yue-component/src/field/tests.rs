use super::*;

#[test]
fn field_message_statuses_resolve_to_yue_semantic_tokens() {
    assert_eq!(
        field_message_token(InputStatus::Normal),
        crate::theme::keys::INPUT_AFFIX_COLOR
    );
    assert_eq!(
        field_message_token(InputStatus::Success),
        crate::theme::keys::SUCCESS_FOREGROUND
    );
    assert_eq!(
        field_message_token(InputStatus::Warning),
        crate::theme::keys::WARNING_FOREGROUND
    );
    assert_eq!(
        field_message_token(InputStatus::Error),
        crate::theme::keys::ERROR_FOREGROUND
    );
}
