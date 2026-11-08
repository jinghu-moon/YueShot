use super::*;

#[test]
#[should_panic(expected = "visible text")]
fn whitespace_label_is_rejected() {
    let _ = YueLink::new("bad", " \n", "https://example.com");
}
#[test]
#[should_panic(expected = "resource target")]
fn whitespace_target_is_rejected() {
    let _ = YueLink::new("bad", "目标", " \n");
}
#[test]
#[should_panic(expected = "accessible name")]
fn empty_accessible_name_is_rejected() {
    let _ = YueLink::new("bad", "目标", "mailto:help@example.com").accessibility_label(" ");
}
