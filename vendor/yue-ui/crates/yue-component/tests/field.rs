//! Public composition contracts exercised before and after the module migration.
use gpui_base::test_support::{ElementSnapshot, snapshots};
use gpui_kit::base::input::{InputState, TextareaState};
use gpui_kit::{
    AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render, Role, Styled as _,
    TestAppContext, VisualTestContext, Window, div, px,
};
use yue_component::{
    Accent, SelectMode, SelectOption, SelectValue, ThemeMode, YueField, YueForm, YueInput,
    YueNumberInput, YueSelect, YueSelectState, YueTextarea, YueTheme,
};

struct Fields {
    name: Entity<InputState>,
    note: Entity<TextareaState>,
    quantity: Entity<InputState>,
    choice: Entity<YueSelectState>,
    form: Entity<YueForm>,
    label: &'static str,
}
impl Fields {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let name = cx.new(|cx| InputState::new(window, cx));
        let note = cx.new(|cx| TextareaState::new(window, cx).default_value("备注"));
        let quantity = cx.new(|cx| InputState::new(window, cx).default_value("12"));
        let choice = cx.new(|cx| {
            let mut state = YueSelectState::new(SelectMode::Single, window, cx);
            state
                .set_options([SelectOption::new("a", "方案 A")], cx)
                .unwrap();
            state
        });
        let form = cx.new(|_| YueForm::new());
        form.update(cx, |form, _| {
            form.require("name", &name, "请填写姓名").require_select(
                "choice",
                &choice,
                "请选择方案",
            );
        });
        Self {
            name,
            note,
            quantity,
            choice,
            form,
            label: "姓名",
        }
    }
}
impl Render for Fields {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = yue_component::current_theme(cx);
        div()
            .flex()
            .flex_col()
            .gap_3()
            .p_4()
            .w(px(500.))
            .child(
                YueField::new(theme)
                    .label(self.label)
                    .description("公开名称")
                    .required(true)
                    .bind(&self.form, "name")
                    .input(YueInput::new("name", theme, &self.name).width(px(200.))),
            )
            .child(
                YueField::new(theme)
                    .label("备注")
                    .textarea(YueTextarea::new("note", theme, &self.note).width(px(200.))),
            )
            .child(YueField::new(theme).label("数量").number_input(
                YueNumberInput::new("quantity", theme, &self.quantity).width(px(100.)),
            ))
            .child(
                YueField::new(theme)
                    .label("方案")
                    .description("选择一个方案")
                    .required(true)
                    .bind(&self.form, "choice")
                    .select(YueSelect::new("choice", &self.choice).width(px(200.))),
            )
    }
}
fn open(cx: &mut TestAppContext) -> (Entity<Fields>, &mut VisualTestContext) {
    cx.update(|cx| {
        yue_component::init(cx, YueTheme::new(ThemeMode::Light, Accent::Azure)).unwrap()
    });
    let mut fields = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| Fields::new(window, cx));
        fields = Some(view.clone());
        gpui_kit::base::Root::new(view, window, cx)
    });
    draw(cx);
    (fields.unwrap(), cx)
}
fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
}
fn node(cx: &mut VisualTestContext, label: &str) -> ElementSnapshot {
    cx.update(|window, _| {
        let nodes = snapshots(window)
            .into_iter()
            .filter(|node| node.label() == Some(label))
            .collect::<Vec<_>>();
        assert_eq!(nodes.len(), 1, "one interactive node for {label}");
        nodes[0].clone()
    })
}
#[gpui_kit::test]
fn field_names_all_control_families_without_duplicate_accessibility_nodes(cx: &mut TestAppContext) {
    let (_, cx) = open(cx);
    for (label, role) in [
        ("姓名", Role::TextInput),
        ("备注", Role::MultilineTextInput),
        ("数量", Role::SpinButton),
        ("方案", Role::ComboBox),
    ] {
        assert_eq!(node(cx, label).role(), Some(role));
    }
    assert_eq!(node(cx, "姓名").description(), Some("Required, 公开名称"));
    assert_eq!(
        node(cx, "方案").description(),
        Some("Required, 选择一个方案")
    );
}
#[gpui_kit::test]
fn mixed_form_focuses_each_live_invalid_control_and_clears_its_message(cx: &mut TestAppContext) {
    let (fields, cx) = open(cx);
    let (form, name, choice) = fields.update(cx, |fields, _| {
        (
            fields.form.clone(),
            fields.name.clone(),
            fields.choice.clone(),
        )
    });
    cx.update(|window, cx| assert!(!form.update(cx, |form, cx| form.submit(window, cx))));
    draw(cx);
    assert_eq!(node(cx, "姓名").focused(), Some(true));
    assert_eq!(
        node(cx, "姓名").description(),
        Some("Required, 公开名称, 请填写姓名")
    );
    cx.update(|window, cx| name.update(cx, |input, cx| input.set_value("Ada", window, cx)));
    cx.update(|window, cx| assert!(!form.update(cx, |form, cx| form.submit(window, cx))));
    draw(cx);
    assert_eq!(node(cx, "方案").focused(), Some(true));
    assert_eq!(node(cx, "姓名").description(), Some("Required, 公开名称"));
    assert_eq!(
        node(cx, "方案").description(),
        Some("Required, 选择一个方案, 请选择方案")
    );
    cx.update(|window, cx| {
        choice.update(cx, |state, cx| {
            state
                .set_value(SelectValue::Single(Some("a".into())), window, cx)
                .unwrap()
        })
    });
    cx.update(|window, cx| assert!(form.update(cx, |form, cx| form.submit(window, cx))));
    draw(cx);
    assert_eq!(
        node(cx, "方案").description(),
        Some("Required, 选择一个方案")
    );
}
#[gpui_kit::test]
fn parent_rebuild_and_theme_change_preserve_editor_focus_and_values(cx: &mut TestAppContext) {
    let (fields, cx) = open(cx);
    let name = fields.update(cx, |fields, _| fields.name.clone());
    cx.update(|window, cx| name.update(cx, |input, cx| input.focus(window, cx)));
    cx.simulate_input("Ada");
    fields.update(cx, |fields, cx| {
        fields.label = "显示名称";
        cx.notify();
    });
    cx.update(|_, cx| {
        yue_component::set_theme(cx, YueTheme::new(ThemeMode::Dark, Accent::Neutral))
    });
    draw(cx);
    cx.simulate_input(" Lovelace");
    draw(cx);
    assert_eq!(node(cx, "显示名称").focused(), Some(true));
    assert_eq!(node(cx, "显示名称").value(), Some("Ada Lovelace"));
    assert_eq!(node(cx, "备注").value(), Some("备注"));
    assert_eq!(node(cx, "数量").value(), Some("12"));
}
