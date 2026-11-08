//! Yue design tokens and components for applications built on GPUI Kit.
//!
//! Applications own their window and lifecycle. This crate provides presentation
//! data and components while reusing the GPUI types pinned by `gpui-kit`.

mod application;
mod breadcrumb;
mod button;
mod checkbox;
mod collapse;
mod color_picker;
mod control_layout;
mod dialog;
mod divider;
mod dropdown;
mod field;
mod form;
pub mod icon;
mod image_preview;
mod input;
pub mod layout;
mod link;
mod list;
mod loading;
pub mod motion;
mod popup;
mod radio;
mod segmented;
mod select;
mod slider;
mod swatch;
mod switch;
mod tabs;
mod tag;
pub mod theme;
mod toast;
mod tokens;
mod tooltip;
pub mod typography;
mod virtual_list;
pub use virtual_list::{
    VirtualGrid, VirtualGridCell, VirtualGridCellIdentity, VirtualGridCellKey, VirtualGridSnapshot,
    VirtualGridSpan, VirtualGridSpanSet, VirtualGridTrack, VirtualMasonry, VirtualMasonryCache,
    VirtualMasonryCacheItem, VirtualMasonryCapabilities, VirtualMasonryItem,
    VirtualMasonrySnapshot, VirtualMasonryState, VirtualizerAlign, VirtualizerAnchorPosition,
    VirtualizerAttachment, VirtualizerBinding, VirtualizerCache, VirtualizerCapabilities,
    VirtualizerChange, VirtualizerCommandOutcome, VirtualizerError, VirtualizerEvent,
    VirtualizerItem, VirtualizerItemIdentity, VirtualizerLayout, VirtualizerMeasurement,
    VirtualizerMotion, VirtualizerRange, VirtualizerRangeExtractor, VirtualizerScrollDirection,
    VirtualizerScrollOptions, VirtualizerScrollRequest, VirtualizerScrollbarHandle,
    VirtualizerSizeUpdate, VirtualizerSizing, VirtualizerSizingKind, VirtualizerSnapshot,
    VirtualizerSource, VirtualizerState, VirtualizerViewport, YueVirtualList,
};

pub use application::{current_theme, init, set_theme};
pub use breadcrumb::{
    BreadcrumbCollapse, BreadcrumbEllipsis, BreadcrumbNavigate, BreadcrumbNavigateSource,
    BreadcrumbSeparator, YueBreadcrumb, YueBreadcrumbItem,
};
pub use button::{ButtonGroup, ButtonShape, ButtonSize, ButtonVariant, IconPosition, YueButton};
pub use checkbox::{CheckboxSize, CheckboxState, YueCheckbox, YueCheckboxGroup, YueCheckboxOption};
pub use collapse::{
    CollapseChangeRequest, CollapseChangeSource, CollapseExpandTrigger, CollapseIndicatorPosition,
    CollapseMode, CollapseMotion, CollapseSize, CollapseVariant, YueCollapse, YueCollapseItem,
};
pub use color_picker::{
    ColorFormat, ColorPickerEvent, ColorPickerSize, ColorPickerTrigger, YueColorPicker,
    YueColorPickerPanel, YueColorPickerState,
};
pub use dialog::{
    DialogChange, DialogCloseReason, DialogDecision, DialogFooter, DialogOpenError, DialogOutcome,
    DialogSize, DialogStatus, DialogSubmitRequest, DialogTicket, YueDialog, YueDialogEvent,
    YueDialogState,
};
pub use divider::{DividerAlign, DividerLayout, DividerLineStyle, YueDivider};
pub use dropdown::{
    DropdownAlign, DropdownIndicatorSide, DropdownPlacement, YueContextMenu, YueDropdownMenu,
    YueDropdownSplitButton, YueMenuItem,
};
pub use field::YueField;
pub use form::YueForm;
pub use gpui_kit;
pub use gpui_kit::base::input::{InputEvent, InputState, TextareaState};
pub use icon::{IconSize, YueIcon};
pub use image_preview::{
    ImagePreviewBackground, ImagePreviewChangeSource, ImagePreviewColorSpace, ImagePreviewDocument,
    ImagePreviewError, ImagePreviewEvent, ImagePreviewFit, ImagePreviewImage, ImagePreviewLimits,
    ImagePreviewLoad, ImagePreviewMode, ImagePreviewOverlay, ImagePreviewPixels,
    ImagePreviewSampling, ImagePreviewSnapshot, ImagePreviewToolbar, ImagePreviewView,
    YueImagePreview, YueImagePreviewState,
};
pub use input::{
    InputSize, InputStatus, InputTextAlign, InputType, YueInput, YueNumberInput, YueTextarea,
};
pub use layout::Breakpoint;
pub use link::{LinkHover, LinkSize, LinkVariant, YueLink};
pub use list::{
    ListGrid, ListGridGeometry, ListGridSizing, ListGridState, ListItemLayout, ListLayout,
    ListSize, YueList, YueListGroup, YueListItem, YueListItemMeta, YueVirtualListGrid,
};
pub use loading::{
    FeedbackSize, ProgressShape, ProgressStatus, ProgressValue, SkeletonAnimation, SkeletonShape,
    YueLoadingRegion, YueProgress, YueSkeleton, YueSpinner,
};
pub use motion::{MotionDuration, MotionEase};
pub use radio::{RadioAppearance, RadioSize, YueRadio, YueRadioGroup, YueRadioOption};
pub use segmented::{
    SegmentedDistribution, SegmentedShape, SegmentedSize, SegmentedVariant, YueSegmented,
    YueSegmentedOption,
};
pub use select::{
    SelectApply, SelectChange, SelectChangeSource, SelectCloseReason, SelectEntry, SelectError,
    SelectEvent, SelectGroup, SelectMode, SelectOpenChange, SelectOption, SelectOptionContext,
    SelectPage, SelectPanelWidth, SelectRequest, SelectRequestTicket, SelectResultState,
    SelectRowLayout, SelectSearch, SelectSize, SelectTagLayout, SelectValue, SelectVariant,
    SelectVirtualization, YueSelect, YueSelectState,
};
pub use slider::{
    SliderControls, SliderDomain, SliderLayout, SliderMark, SliderSize, SliderTooltip, SliderValue,
    YueSlider, YueSliderEvent, YueSliderState,
};
pub use swatch::{YueSwatchGroup, YueSwatchOption};
pub use switch::{SwitchIndicator, SwitchShape, SwitchSize, YueSwitch};
pub use tabs::{
    TabsActivation, TabsChangeRequest, TabsChangeSource, TabsCloseRequest, TabsContentMotion,
    TabsHandle, TabsPlacement, TabsScrollAlign, TabsSize, TabsVariant, YueTabItem, YueTabs,
};
pub use tag::{TagColors, TagShape, TagSize, TagTone, TagVariant, YueTag, YueTagGroup};
pub use theme::{Accent, ColorRole, NeutralTone, ThemeMode, ThemePreference, YueTheme, YueToken};
pub use toast::{
    FocusToasts, ToastAction, ToastAnnouncement, ToastAppearance, ToastDecision,
    ToastDismissReason, ToastError, ToastEvent, ToastId, ToastInsets, ToastKind, ToastPhase,
    ToastPlayback, ToastPosition, ToastSnapshot, ToastSpec, ToastTicket, ToasterOptions,
    YueToaster, YueToasterState,
};
pub use tooltip::{
    TooltipAlign, TooltipOptions, TooltipPlacement, TooltipTrigger, TooltipVariant,
    YueOverflowTooltip, YueTooltip, YueTooltipGroup,
};
pub use typography::{FontStack, TextRole, TypeStyle, tabular_nums};
/// Common component types and GPUI composition traits. Import less frequently
/// used component options explicitly from the crate root.
pub mod prelude {
    pub use crate::{
        Accent, Icon, SelectMode, SelectOption, SelectValue, SliderControls, SliderDomain,
        SliderLayout, SliderMark, SliderSize, SliderTooltip, SliderValue, ThemeMode,
        ThemePreference, YueBreadcrumb, YueButton, YueCheckbox, YueColorPicker,
        YueColorPickerPanel, YueColorPickerState, YueDivider, YueDropdownMenu, YueField, YueForm,
        YueInput, YueLink, YueMenuItem, YueNumberInput, YueOverflowTooltip, YueRadio,
        YueRadioGroup, YueSegmented, YueSegmentedOption, YueSelect, YueSelectState, YueSlider,
        YueSliderEvent, YueSliderState, YueSwatchGroup, YueSwatchOption, YueSwitch, YueTabItem,
        YueTabs, YueTextarea, YueTheme, YueTooltip, YueTooltipGroup, current_theme, outline,
    };
    pub use crate::{
        ToastSpec, VirtualizerSizing, VirtualizerSource, VirtualizerState, YueCollapse,
        YueCollapseItem, YueDialog, YueDialogState, YueImagePreview, YueImagePreviewState, YueList,
        YueListGroup, YueListItem, YueListItemMeta, YueLoadingRegion, YueProgress, YueSkeleton,
        YueSpinner, YueTag, YueTagGroup, YueToaster, YueToasterState, YueVirtualList,
    };
    pub use gpui_kit::{
        InteractiveElement, IntoElement, ParentElement, StatefulInteractiveElement, Styled,
        prelude::FluentBuilder,
    };
}
pub use yue_component_icons::{Icon, IconDefinition, filled, outline};

/// Which way a group lays its options out.
///
/// A vertical column is the form default; a horizontal row is for a short,
/// bounded set of options and wraps onto another row when the width runs out.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum GroupDirection {
    #[default]
    Vertical,
    Horizontal,
}
