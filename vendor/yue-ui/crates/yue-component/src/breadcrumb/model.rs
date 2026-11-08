use std::{collections::HashSet, ops::Range};

use crate::{Icon, IconDefinition, YueIcon};
use gpui_kit::SharedString;

/// A stable destination and its complete visible name. The final item is current.
#[derive(Clone)]
pub struct YueBreadcrumbItem {
    pub(super) value: SharedString,
    pub(super) label: SharedString,
    pub(super) icon: Option<Icon>,
    pub(super) disabled: bool,
    pub(super) description: Option<SharedString>,
}
impl YueBreadcrumbItem {
    pub fn new(value: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        let (value, label) = (value.into(), label.into());
        assert!(
            !value.trim().is_empty(),
            "Breadcrumb value must not be blank"
        );
        assert!(
            !label.trim().is_empty(),
            "Breadcrumb label must not be blank"
        );
        Self {
            value,
            label,
            icon: None,
            disabled: false,
            description: None,
        }
    }
    pub fn icon(mut self, icon: impl IconDefinition) -> Self {
        self.icon = Some(Icon::new(icon));
        self
    }
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
    pub fn description(mut self, text: impl Into<SharedString>) -> Self {
        self.description = Some(text.into());
        self
    }
    pub fn value(&self) -> &SharedString {
        &self.value
    }
    pub fn label(&self) -> &SharedString {
        &self.label
    }
    pub fn is_disabled(&self) -> bool {
        self.disabled
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BreadcrumbNavigateSource {
    Item,
    OverflowMenu,
}

/// A request, not an optimistic route change. The host owns navigation results.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BreadcrumbNavigate {
    pub(super) value: SharedString,
    pub(super) source: BreadcrumbNavigateSource,
}
impl BreadcrumbNavigate {
    pub fn value(&self) -> &SharedString {
        &self.value
    }
    pub fn source(&self) -> BreadcrumbNavigateSource {
        self.source
    }
}

/// Collapse one contiguous middle range when the item count exceeds the limit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BreadcrumbCollapse {
    max: usize,
    before: usize,
    after: usize,
}
impl BreadcrumbCollapse {
    pub fn new(max_items: usize) -> Self {
        assert!(
            max_items >= 3,
            "Breadcrumb max_items must be at least three"
        );
        Self {
            max: max_items,
            before: 1,
            after: 1,
        }
    }
    pub fn before(mut self, count: usize) -> Self {
        self.before = count;
        self
    }
    pub fn after(mut self, count: usize) -> Self {
        self.after = count;
        self
    }
    fn validate(self) {
        assert!(
            self.before > 0 && self.after > 0,
            "Breadcrumb must retain both ends"
        );
        assert!(
            self.before
                .checked_add(self.after)
                .and_then(|n| n.checked_add(1))
                .is_some_and(|n| n <= self.max),
            "Breadcrumb retained items plus ellipsis exceed max_items"
        );
    }
}

#[derive(Clone)]
pub(super) enum Decoration {
    Text(SharedString),
    Icon(Icon),
}
impl Decoration {
    fn text(text: impl Into<SharedString>) -> Self {
        let text = text.into();
        assert!(
            !text.trim().is_empty(),
            "Breadcrumb decoration must not be blank"
        );
        Self::Text(text)
    }
}
/// Decorative only; separators never take focus or navigate.
#[derive(Clone)]
pub struct BreadcrumbSeparator(pub(super) Decoration);
impl BreadcrumbSeparator {
    pub fn text(text: impl Into<SharedString>) -> Self {
        Self(Decoration::text(text))
    }
    pub fn icon(icon: impl IconDefinition) -> Self {
        Self(Decoration::Icon(Icon::new(icon)))
    }
}
impl Default for BreadcrumbSeparator {
    fn default() -> Self {
        Self::icon(YueIcon::ChevronRight)
    }
}
/// The content of the hidden-level menu trigger; its accessible name is separate.
#[derive(Clone)]
pub struct BreadcrumbEllipsis(pub(super) Decoration);
impl BreadcrumbEllipsis {
    pub fn text(text: impl Into<SharedString>) -> Self {
        Self(Decoration::text(text))
    }
    pub fn icon(icon: impl IconDefinition) -> Self {
        Self(Decoration::Icon(Icon::new(icon)))
    }
}
impl Default for BreadcrumbEllipsis {
    fn default() -> Self {
        Self::text("…")
    }
}

pub(super) struct PathPlan {
    pub(super) visible: Vec<Option<usize>>,
    pub(super) hidden: Range<usize>,
}
pub(super) fn plan(items: &[YueBreadcrumbItem], collapse: Option<BreadcrumbCollapse>) -> PathPlan {
    let mut values = HashSet::new();
    for item in items {
        assert!(
            values.insert(&item.value),
            "Breadcrumb values must be unique: {}",
            item.value
        );
    }
    if let Some(collapse) = collapse {
        collapse.validate();
        if items.len() > collapse.max {
            let end = items.len() - collapse.after;
            return PathPlan {
                visible: (0..collapse.before)
                    .map(Some)
                    .chain([None])
                    .chain((end..items.len()).map(Some))
                    .collect(),
                hidden: collapse.before..end,
            };
        }
    }
    PathPlan {
        visible: (0..items.len()).map(Some).collect(),
        hidden: 0..0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn items(n: usize) -> Vec<YueBreadcrumbItem> {
        (0..n)
            .map(|i| YueBreadcrumbItem::new(format!("id-{i}"), "Page"))
            .collect()
    }
    #[test]
    fn collapse_is_deterministic_and_preserves_current_and_order() {
        for n in 0..12 {
            let input = items(n);
            let path = plan(&input, Some(BreadcrumbCollapse::new(5).before(2).after(2)));
            if n <= 5 {
                assert_eq!(path.visible, (0..n).map(Some).collect::<Vec<_>>());
                assert!(path.hidden.is_empty());
            } else {
                assert_eq!(
                    path.visible,
                    vec![Some(0), Some(1), None, Some(n - 2), Some(n - 1)]
                );
                assert_eq!(path.hidden, 2..n - 2);
            }
            assert_eq!(input.len(), n);
        }
        assert_eq!(
            plan(&items(8), Some(BreadcrumbCollapse::new(5))).visible,
            vec![Some(0), None, Some(7)]
        );
        assert_eq!(plan(&items(8), None).visible.len(), 8);
    }
    #[test]
    #[should_panic(expected = "values must be unique")]
    fn duplicate_values_are_rejected() {
        plan(
            &[
                YueBreadcrumbItem::new("a", "First"),
                YueBreadcrumbItem::new("a", "Second"),
            ],
            None,
        );
    }
    #[test]
    #[should_panic(expected = "must retain both ends")]
    fn zero_retained_end_is_rejected() {
        plan(&items(1), Some(BreadcrumbCollapse::new(5).after(0)));
    }
    #[test]
    #[should_panic(expected = "exceed max_items")]
    fn invalid_capacity_is_rejected_even_on_short_paths() {
        plan(&items(1), Some(BreadcrumbCollapse::new(3).before(2)));
    }
    #[test]
    #[should_panic(expected = "value must not be blank")]
    fn blank_value_is_rejected() {
        YueBreadcrumbItem::new(" ", "Page");
    }
    #[test]
    #[should_panic(expected = "label must not be blank")]
    fn blank_label_is_rejected() {
        YueBreadcrumbItem::new("page", "　");
    }
}
