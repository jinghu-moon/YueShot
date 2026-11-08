//! Stateful choice fields composed with Base Select, YueInput, YueTag and Popup.
mod behavior;
mod element;
mod layout;
mod model;
mod option;
mod panel;
mod state;
#[cfg(all(test, feature = "headless-tests"))]
mod tests;
use crate::{InputStatus, YueTag, YueTheme, control_layout::ControlLayout};
use gpui_kit::{
    AnyElement, App, DefiniteLength, ElementId, Entity, FocusHandle, IntoElement, Pixels,
    RenderOnce, SharedString, Window,
};
pub use model::{
    SelectApply, SelectChange, SelectChangeSource, SelectCloseReason, SelectEntry, SelectError,
    SelectEvent, SelectGroup, SelectMode, SelectOpenChange, SelectOption, SelectPage,
    SelectPanelWidth, SelectRequest, SelectRequestTicket, SelectRowLayout, SelectSearch,
    SelectSize, SelectTagLayout, SelectValue, SelectVariant, SelectVirtualization,
};
pub use state::{SelectResultState, YueSelectState};
use std::rc::Rc;

/// Read-only row projection delivered to a custom content renderer. The
/// component retains the native option node, activation and fixed row height.
#[derive(Clone)]
pub struct SelectOptionContext {
    option: SelectOption,
    selected: bool,
    active: bool,
    disabled: bool,
    query: SharedString,
    size: SelectSize,
}
impl SelectOptionContext {
    pub fn option(&self) -> &SelectOption {
        &self.option
    }
    pub fn is_selected(&self) -> bool {
        self.selected
    }
    pub fn is_active(&self) -> bool {
        self.active
    }
    pub fn is_disabled(&self) -> bool {
        self.disabled
    }
    pub fn query(&self) -> &SharedString {
        &self.query
    }
    pub fn size(&self) -> SelectSize {
        self.size
    }
}
type OptionRenderer = Rc<dyn Fn(&SelectOptionContext, &mut Window, &mut App) -> AnyElement>;
type ValueRenderer = Rc<dyn Fn(&[SelectOption], &mut Window, &mut App) -> AnyElement>;
type TagRenderer = Rc<dyn Fn(YueTag, &SelectOption, &mut Window, &mut App) -> YueTag>;
type CollapseRenderer = Rc<dyn Fn(usize, &mut Window, &mut App) -> AnyElement>;
type PanelRenderer = Rc<dyn Fn(bool, &mut Window, &mut App) -> AnyElement>;
#[derive(Clone, Copy, Default)]
enum Width {
    #[default]
    Default,
    Fill,
    Content,
}
#[derive(Clone)]
pub(super) struct Presentation {
    id: ElementId,
    theme: Option<YueTheme>,
    size: SelectSize,
    variant: SelectVariant,
    placeholder: SharedString,
    name: SharedString,
    description: Option<SharedString>,
    prefix: Option<SharedString>,
    icon: Option<crate::Icon>,
    width: Width,
    layout: Rc<ControlLayout>,
    panel_width: SelectPanelWidth,
    max_height: Option<Pixels>,
    row_layout: SelectRowLayout,
    virtualization: SelectVirtualization,
    tag_layout: SelectTagLayout,
    collapse_count: usize,
    clearable: bool,
    focus: Option<FocusHandle>,
    tab_index: isize,
    tab_stop: bool,
    key_context: Option<SharedString>,
    status: InputStatus,
    required: bool,
    empty_text: SharedString,
    loading_text: SharedString,
    option_renderer: Option<OptionRenderer>,
    value_renderer: Option<ValueRenderer>,
    tag_renderer: Option<TagRenderer>,
    collapse_renderer: Option<CollapseRenderer>,
    header: Option<PanelRenderer>,
    footer: Option<PanelRenderer>,
    header_focus: Vec<FocusHandle>,
    footer_focus: Vec<FocusHandle>,
}
/// A selection field. Keep its state in the host; the same state may be mounted
/// once. User changes are retained SelectEvent subscriptions, external setters
/// are silent.
///
/// ```
/// use yue_component::{YueSelect, YueSelectState, SelectMode, SelectOption, gpui_kit};
/// use gpui_kit::{AppContext as _, Context, Entity, IntoElement, Render, Window};
/// struct Settings { language: Entity<YueSelectState> }
/// impl Settings {
///     fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
///         let language = cx.new(|cx| {
///             let mut state = YueSelectState::new(SelectMode::Single, window, cx);
///             state.set_options([SelectOption::new("rust", "Rust")], cx).unwrap();
///             state
///         });
///         Self { language }
///     }
/// }
/// impl Render for Settings {
///     fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
///         YueSelect::new("language", &self.language).accessibility_label("Language")
///     }
/// }
/// ```
#[derive(IntoElement)]
pub struct YueSelect {
    state: Entity<YueSelectState>,
    presentation: Presentation,
}
impl YueSelect {
    pub fn new(id: impl Into<ElementId>, state: &Entity<YueSelectState>) -> Self {
        Self {
            state: state.clone(),
            presentation: Presentation {
                id: id.into(),
                theme: None,
                size: SelectSize::Medium,
                variant: SelectVariant::Outline,
                placeholder: "请选择".into(),
                name: "".into(),
                description: None,
                prefix: None,
                icon: None,
                width: Width::Default,
                layout: Rc::new(ControlLayout::default()),
                panel_width: SelectPanelWidth::MatchTrigger,
                max_height: None,
                row_layout: SelectRowLayout::SingleLine,
                virtualization: SelectVirtualization::Auto,
                tag_layout: SelectTagLayout::Collapse,
                collapse_count: 2,
                clearable: false,
                focus: None,
                tab_index: 0,
                tab_stop: true,
                key_context: None,
                status: InputStatus::Normal,
                required: false,
                empty_text: "暂无匹配选项".into(),
                loading_text: "正在加载选项".into(),
                option_renderer: None,
                value_renderer: None,
                tag_renderer: None,
                collapse_renderer: None,
                header: None,
                footer: None,
                header_focus: vec![],
                footer_focus: vec![],
            },
        }
    }
    pub fn theme(mut self, theme: YueTheme) -> Self {
        self.presentation.theme = Some(theme);
        self
    }
    pub fn use_default_theme(mut self) -> Self {
        self.presentation.theme = None;
        self
    }
    pub fn size(mut self, size: SelectSize) -> Self {
        self.presentation.size = size;
        self
    }
    pub fn variant(mut self, variant: SelectVariant) -> Self {
        self.presentation.variant = variant;
        self
    }
    pub fn placeholder(mut self, text: impl Into<SharedString>) -> Self {
        self.presentation.placeholder = text.into();
        self
    }
    pub fn accessibility_label(mut self, text: impl Into<SharedString>) -> Self {
        self.presentation.name = text.into();
        self
    }
    pub fn accessibility_description(mut self, text: impl Into<SharedString>) -> Self {
        self.presentation.description = Some(text.into());
        self
    }
    pub fn prefix_text(mut self, text: impl Into<SharedString>) -> Self {
        self.presentation.prefix = Some(text.into());
        self
    }
    pub fn leading_icon(mut self, icon: impl crate::IconDefinition) -> Self {
        self.presentation.icon = Some(crate::Icon::new(icon));
        self
    }
    pub fn width(mut self, width: impl Into<DefiniteLength>) -> Self {
        Rc::make_mut(&mut self.presentation.layout).width = Some(width.into());
        self.presentation.width = Width::Default;
        self
    }
    pub fn block(mut self) -> Self {
        Rc::make_mut(&mut self.presentation.layout).width = None;
        self.presentation.width = Width::Fill;
        self
    }
    pub fn auto_width(mut self) -> Self {
        Rc::make_mut(&mut self.presentation.layout).width = None;
        self.presentation.width = Width::Content;
        self
    }
    pub fn min_width(mut self, width: impl Into<gpui_kit::Length>) -> Self {
        Rc::make_mut(&mut self.presentation.layout).min_width = Some(width.into());
        self
    }
    pub fn max_width(mut self, width: impl Into<gpui_kit::Length>) -> Self {
        Rc::make_mut(&mut self.presentation.layout).max_width = Some(width.into());
        self
    }
    pub fn margin(mut self, value: impl Into<gpui_kit::Length>) -> Self {
        Rc::make_mut(&mut self.presentation.layout).margin = Some(value.into());
        self
    }
    pub fn flex_grow(mut self, value: f32) -> Self {
        assert!(value.is_finite() && value >= 0.);
        Rc::make_mut(&mut self.presentation.layout).grow = Some(value);
        self
    }
    pub fn panel_width(mut self, width: SelectPanelWidth) -> Self {
        if let SelectPanelWidth::Fixed(width) = width {
            assert!(f32::from(width).is_finite() && width > gpui_kit::px(0.));
        }
        self.presentation.panel_width = width;
        self
    }
    pub fn max_panel_height(mut self, height: Pixels) -> Self {
        assert!(f32::from(height).is_finite() && height > gpui_kit::px(0.));
        self.presentation.max_height = Some(height);
        self
    }
    pub fn row_layout(mut self, value: SelectRowLayout) -> Self {
        self.presentation.row_layout = value;
        self
    }
    pub fn virtualization(mut self, value: SelectVirtualization) -> Self {
        self.presentation.virtualization = value;
        self
    }
    pub fn tag_layout(mut self, value: SelectTagLayout) -> Self {
        self.presentation.tag_layout = value;
        self
    }
    pub fn collapse_count(mut self, value: usize) -> Self {
        self.presentation.collapse_count = value;
        self
    }
    pub fn clearable(mut self, value: bool) -> Self {
        self.presentation.clearable = value;
        self
    }
    pub fn track_focus(mut self, focus: &FocusHandle) -> Self {
        self.presentation.focus = Some(focus.clone());
        self
    }
    pub fn tab_index(mut self, value: isize) -> Self {
        self.presentation.tab_index = value;
        self
    }
    pub fn tab_stop(mut self, value: bool) -> Self {
        self.presentation.tab_stop = value;
        self
    }
    pub fn key_context(mut self, value: impl Into<SharedString>) -> Self {
        self.presentation.key_context = Some(value.into());
        self
    }
    pub fn status(mut self, value: InputStatus) -> Self {
        self.presentation.status = value;
        self
    }
    pub fn required(mut self, value: bool) -> Self {
        self.presentation.required = value;
        self
    }
    pub fn empty_text(mut self, value: impl Into<SharedString>) -> Self {
        self.presentation.empty_text = value.into();
        self
    }
    pub fn loading_text(mut self, value: impl Into<SharedString>) -> Self {
        self.presentation.loading_text = value.into();
        self
    }
    /// Replace only the option's non-interactive content. Select retains the
    /// row height, vertical centering, trailing check and activation semantics.
    pub fn render_option<E: IntoElement>(
        mut self,
        render: impl Fn(&SelectOptionContext, &mut Window, &mut App) -> E + 'static,
    ) -> Self {
        self.presentation.option_renderer = Some(Rc::new(move |context, w, cx| {
            render(context, w, cx).into_any_element()
        }));
        self
    }
    pub fn render_value<E: IntoElement>(
        mut self,
        render: impl Fn(&[SelectOption], &mut Window, &mut App) -> E + 'static,
    ) -> Self {
        self.presentation.value_renderer = Some(Rc::new(move |items, w, cx| {
            render(items, w, cx).into_any_element()
        }));
        self
    }
    pub fn render_tag(
        mut self,
        render: impl Fn(YueTag, &SelectOption, &mut Window, &mut App) -> YueTag + 'static,
    ) -> Self {
        self.presentation.tag_renderer = Some(Rc::new(render));
        self
    }
    pub fn render_collapsed<E: IntoElement>(
        mut self,
        render: impl Fn(usize, &mut Window, &mut App) -> E + 'static,
    ) -> Self {
        self.presentation.collapse_renderer = Some(Rc::new(move |count, w, cx| {
            render(count, w, cx).into_any_element()
        }));
        self
    }
    pub fn panel_header<E: IntoElement>(
        mut self,
        render: impl Fn(bool, &mut Window, &mut App) -> E + 'static,
    ) -> Self {
        self.presentation.header = Some(Rc::new(move |interactive, w, cx| {
            render(interactive, w, cx).into_any_element()
        }));
        self
    }
    pub fn panel_footer<E: IntoElement>(
        mut self,
        render: impl Fn(bool, &mut Window, &mut App) -> E + 'static,
    ) -> Self {
        self.presentation.footer = Some(Rc::new(move |interactive, w, cx| {
            render(interactive, w, cx).into_any_element()
        }));
        self
    }
    /// Register the visible, enabled header controls in reading order. Render
    /// these controls with the same handles and `tab_stop(false)`; Select owns
    /// their temporary traversal. Slot factories receive false during exit
    /// and must disable their controls for that frame.
    pub fn panel_header_focus(mut self, handles: impl IntoIterator<Item = FocusHandle>) -> Self {
        self.presentation.header_focus = handles.into_iter().collect();
        self
    }
    /// Like `panel_header_focus`, for footer commands.
    pub fn panel_footer_focus(mut self, handles: impl IntoIterator<Item = FocusHandle>) -> Self {
        self.presentation.footer_focus = handles.into_iter().collect();
        self
    }
}
impl RenderOnce for YueSelect {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        self.state.update(cx, |state, _| {
            if let Some(focus) = &self.presentation.focus {
                state.focus = focus.clone();
            }
            state.presentation = Some(self.presentation);
        });
        self.state
    }
}
pub(crate) fn init(cx: &mut App) {
    behavior::init(cx);
}
