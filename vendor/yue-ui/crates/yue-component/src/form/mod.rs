use gpui_kit::{
    App, Context, Entity, SharedString, Window,
    base::input::{InputBaseState, InputEvent, InputModeKind},
};

type Focus = Box<dyn Fn(&mut Window, &mut App)>;
type Validation = Box<dyn Fn(&App) -> Option<SharedString>>;
type Submitted = Box<dyn Fn(bool, &mut Window, &mut App)>;

struct FormField {
    id: SharedString,
    focus: Focus,
    validate: Validation,
}

/// The validation state shared by the fields of one form.
///
/// The form owns rules and the checked flag, never the text: each field keeps
/// its own GPUI Base state, so editing, IME, and caret behaviour stay in Base.
/// [`YueField::bind`](crate::YueField::bind) shows a rule's result next to its
/// control, and [`YueForm::submit`] focuses the first field still failing.
///
/// ```
/// use yue_component::YueForm;
/// let form = YueForm::new();
/// ```
#[derive(Default)]
pub struct YueForm {
    fields: Vec<FormField>,
    checked: bool,
    submitted: Option<Submitted>,
    subscriptions: Vec<gpui_kit::Subscription>,
}

impl YueForm {
    pub fn new() -> Self {
        Self::default()
    }

    /// Require a non-empty value, trimming whitespace.
    pub fn require<M: InputModeKind>(
        &mut self,
        id: impl Into<SharedString>,
        state: &Entity<InputBaseState<M>>,
        message: impl Into<SharedString>,
    ) -> &mut Self {
        let message = message.into();
        self.rule(id, state, move |value, _| {
            value.trim().is_empty().then(|| message.clone())
        })
    }

    /// Validate a field. `None` accepts the value; `Some(message)` says what to fix.
    pub fn rule<M, R>(
        &mut self,
        id: impl Into<SharedString>,
        state: &Entity<InputBaseState<M>>,
        rule: R,
    ) -> &mut Self
    where
        M: InputModeKind,
        R: Fn(&str, &App) -> Option<SharedString> + 'static,
    {
        let read_state = state.clone();
        let focus_state = state.clone();
        self.fields.push(FormField {
            id: id.into(),
            focus: Box::new(move |window, cx| {
                focus_state.update(cx, |input, cx| input.focus(window, cx));
            }),
            validate: Box::new(move |cx| rule(&read_state.read(cx).value(), cx)),
        });
        self
    }

    /// Require at least one selected key. Query text is never a field value.
    pub fn require_select(
        &mut self,
        id: impl Into<SharedString>,
        state: &Entity<crate::YueSelectState>,
        message: impl Into<SharedString>,
    ) -> &mut Self {
        let message = message.into();
        self.rule_select(id, state, move |value, _| {
            value.is_empty().then(|| message.clone())
        })
    }

    /// Validate typed single/multiple selection without joining keys to text.
    pub fn rule_select(
        &mut self,
        id: impl Into<SharedString>,
        state: &Entity<crate::YueSelectState>,
        rule: impl Fn(&crate::SelectValue, &App) -> Option<SharedString> + 'static,
    ) -> &mut Self {
        let read = state.clone();
        let focus = state.clone();
        self.fields.push(FormField {
            id: id.into(),
            validate: Box::new(move |cx| rule(read.read(cx).value(), cx)),
            focus: Box::new(move |window, cx| {
                focus.update(cx, |state, cx| {
                    state.set_open(false, window, cx);
                    state.focus_handle().focus(window, cx);
                })
            }),
        });
        self
    }

    /// The rule's current message for a field, once the form has been submitted.
    ///
    /// Re-evaluated against the live value, so a corrected field loses its error
    /// without a second submit.
    pub fn error(&self, id: &str, cx: &App) -> Option<SharedString> {
        if !self.checked {
            return None;
        }
        self.field_error(id, cx)
    }

    /// Whether every registered field passes its rule right now.
    pub fn is_valid(&self, cx: &App) -> bool {
        self.fields
            .iter()
            .all(|field| (field.validate)(cx).is_none())
    }

    /// Validate every field, reveal the results, and focus the first failure.
    ///
    /// Returns whether the form is ready to be committed.
    pub fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        self.checked = true;
        let invalid = self
            .fields
            .iter()
            .position(|field| (field.validate)(cx).is_some());
        if let Some(index) = invalid {
            let field = &self.fields[index];
            (field.focus)(window, cx);
        }
        let valid = invalid.is_none();
        cx.notify();
        if let Some(submitted) = self.submitted.as_ref() {
            submitted(valid, window, cx);
        }
        valid
    }

    /// React to a submit, including one triggered by an attached field's enter key.
    ///
    /// The callback receives whether every rule passed. It runs while the form is
    /// borrowed, so it may update other entities but must not submit this form again.
    pub fn on_submit<F>(&mut self, on_submit: F) -> &mut Self
    where
        F: Fn(bool, &mut Window, &mut App) + 'static,
    {
        self.submitted = Some(Box::new(on_submit));
        self
    }

    /// Hide the validation results, for example after the form is committed.
    pub fn reset(&mut self, cx: &mut Context<Self>) {
        self.checked = false;
        cx.notify();
    }

    /// Submit whenever this field's enter key is pressed.
    ///
    /// GPUI Base owns the key itself: a single-line input emits
    /// [`InputEvent::PressEnter`] on Enter, and a multi-line one does so only
    /// when it was built with `submit_on_enter(true)`.
    pub fn attach<M: InputModeKind>(
        &mut self,
        state: &Entity<InputBaseState<M>>,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let subscription = cx.subscribe_in(state, window, move |form, _, event, window, cx| {
            if matches!(event, InputEvent::PressEnter { .. }) {
                form.submit(window, cx);
            }
        });
        self.subscriptions.push(subscription);
    }

    fn field_error(&self, id: &str, cx: &App) -> Option<SharedString> {
        self.fields
            .iter()
            .find(|field| field.id == id)
            .and_then(|field| (field.validate)(cx))
    }
}
