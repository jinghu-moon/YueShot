//! Controlled loading feedback. The host owns work, results and retained content.
use crate::YueTheme;
use gpui_kit::{
    AnyElement, App, ElementId, IntoElement, ParentElement as _, RenderOnce, SharedString,
    Styled as _, Window, div,
};
use std::rc::Rc;
mod paint;
mod progress;
mod skeleton;
mod spinner;
pub use gpui_kit::base::ProgressValue;
pub(crate) use paint::spinner_glyph;
pub use progress::{ProgressShape, ProgressStatus, YueProgress};
pub use skeleton::{SkeletonAnimation, SkeletonShape, YueSkeleton};
pub use spinner::YueSpinner;

/// Shared density tiers; each feedback family resolves its own geometry tokens.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum FeedbackSize {
    Small,
    #[default]
    Medium,
    Large,
}
impl FeedbackSize {
    pub(super) fn choose<T: Copy>(self, small: T, medium: T, large: T) -> T {
        match self {
            Self::Small => small,
            Self::Medium => medium,
            Self::Large => large,
        }
    }
    pub(super) fn font(self) -> crate::theme::TokenKey {
        use crate::theme::keys as k;
        self.choose(
            k::LOADING_FONT_SIZE_SM,
            k::LOADING_FONT_SIZE_MD,
            k::LOADING_FONT_SIZE_LG,
        )
    }
}

type Renderer = Rc<dyn Fn(&mut Window, &mut App) -> AnyElement>;

/// A nonmodal replacement region. Only the active factory is called. Retain
/// editor/input Entities in the host, and coordinate focus before hiding them.
///
/// ```
/// use yue_component::{YueLoadingRegion,YueSkeleton};
/// use yue_component::gpui_kit::{IntoElement,ParentElement,div};
/// let region=YueLoadingRegion::new("profile",true)
///     .label("Loading profile")
///     .placeholder(|_,_|YueSkeleton::new("name").into_any_element())
///     .content(|_,_|div().child("Profile ready").into_any_element());
/// ```
#[derive(IntoElement)]
pub struct YueLoadingRegion {
    id: ElementId,
    loading: bool,
    label: SharedString,
    placeholder: Option<Renderer>,
    content: Option<Renderer>,
    layout: crate::control_layout::ControlLayout,
}
crate::control_layout::control_layout!(YueLoadingRegion);
impl YueLoadingRegion {
    pub fn new(id: impl Into<ElementId>, loading: bool) -> Self {
        Self {
            id: id.into(),
            loading,
            label: "正在加载内容".into(),
            placeholder: None,
            content: None,
            layout: Default::default(),
        }
    }
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = label.into();
        assert!(!self.label.trim().is_empty());
        self
    }
    pub fn placeholder(
        mut self,
        render: impl Fn(&mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        self.placeholder = Some(Rc::new(render));
        self
    }
    pub fn content(
        mut self,
        render: impl Fn(&mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        self.content = Some(Rc::new(render));
        self
    }
}
impl RenderOnce for YueLoadingRegion {
    fn render(self, w: &mut Window, cx: &mut App) -> impl IntoElement {
        let mut region = gpui_kit::base::LoadingRegion::new((self.id, "region"), self.loading)
            .accessibility_label(self.label);
        if self.loading {
            if let Some(render) = self.placeholder {
                region = region.placeholder(render(w, cx));
            }
        } else if let Some(render) = self.content {
            region = region.content(render(w, cx));
        }
        self.layout.apply(div().w_full().min_w_0().child(region))
    }
}
