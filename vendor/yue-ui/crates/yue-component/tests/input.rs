//! Headless checks for what Yue adds on top of GPUI Base: the accessibility node of
//! each control, the character limit, and the validation a form runs on submit.
//! Editing, IME composition and caret movement belong to Base; they are exercised
//! here only as far as Yue's own behaviour depends on them.
//!
//! Queries go through `gpui-base`'s observation layer because enabling
//! `gpui-kit/test-support` would also activate the optional GPUI Fast backend, whose
//! pinned dependencies conflict with the GPUI backend this workspace locks.

use gpui_base::test_support::ElementSnapshot;
use gpui_kit::base::input::{InputState, TextareaState};
use gpui_kit::{
    AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render, Role, SharedString,
    Styled as _, TestAppContext, Window, div, px, rems,
};
use yue_component::{
    Accent, InputType, ThemeMode, YueField, YueForm, YueInput, YueNumberInput, YueTextarea,
    YueTheme,
};

fn theme() -> YueTheme {
    YueTheme::new(ThemeMode::Light, Accent::Azure)
}

/// `CLAMPED` is exactly the limit the fields below declare; `OVER_LENGTH` is one
/// character longer.
const OVER_LENGTH: &str = "表单可访问性与状态一致性别";
const CLAMPED: &str = "表单可访问性与状态一致性";
const LIMIT: usize = 12;

fn render(window: &mut Window, cx: &mut gpui_kit::App) {
    window.refresh();
    window.draw(cx).clear(cx);
}

fn node(window: &Window, label: &str) -> ElementSnapshot {
    let matches: Vec<ElementSnapshot> = gpui_base::test_support::snapshots(window)
        .into_iter()
        .filter(|node| node.label() == Some(label))
        .collect();
    assert_eq!(
        matches.len(),
        1,
        "expected exactly one node labelled {label}, found {}",
        matches.len()
    );
    matches.into_iter().next().expect("one match")
}

struct Probe {
    theme: YueTheme,
    project: Entity<InputState>,
    secret: Entity<InputState>,
    note: Entity<TextareaState>,
    quantity: Entity<InputState>,
    quantity_disabled: bool,
    quantity_status: yue_component::InputStatus,
    narrow: Entity<InputState>,
    semantic: [Entity<InputState>; 3],
}

impl Probe {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            theme: theme(),
            project: cx.new(|cx| InputState::new(window, cx).default_value(OVER_LENGTH)),
            secret: cx.new(|cx| InputState::new(window, cx).default_value("yue-component")),
            note: cx.new(|cx| {
                TextareaState::new(window, cx)
                    .auto_grow(2, 4)
                    .default_value(OVER_LENGTH)
            }),
            quantity: cx.new(|cx| {
                InputState::new(window, cx)
                    .step(1.0)
                    .min(0.0)
                    .max(100.0)
                    .default_value("12")
            }),
            quantity_disabled: false,
            quantity_status: yue_component::InputStatus::Normal,
            narrow: cx.new(|cx| InputState::new(window, cx).default_value("yue@example.com")),
            semantic: [
                cx.new(|cx| InputState::new(window, cx)),
                cx.new(|cx| InputState::new(window, cx)),
                cx.new(|cx| InputState::new(window, cx)),
            ],
        }
    }
}

impl Render for Probe {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        div()
            .flex()
            .flex_col()
            .gap_2()
            .p_4()
            .child(
                YueField::new(theme)
                    .label("项目名称")
                    .description("名称会展示在工作区列表中")
                    .required(true)
                    .input(
                        YueInput::new("project", theme, &self.project)
                            .max_length(LIMIT)
                            .show_counter(true)
                            .width(rems(26.0)),
                    ),
            )
            .child(
                YueField::new(theme).label("搜索项目").input(
                    YueInput::new("search", theme, &self.semantic[0])
                        .input_type(InputType::Search)
                        .width(rems(26.0)),
                ),
            )
            .child(
                YueField::new(theme).label("邮箱").input(
                    YueInput::new("email", theme, &self.semantic[1])
                        .input_type(InputType::Email)
                        .width(rems(26.0)),
                ),
            )
            .child(
                YueField::new(theme).label("主页").input(
                    YueInput::new("url", theme, &self.semantic[2])
                        .input_type(InputType::Url)
                        .width(rems(26.0)),
                ),
            )
            .child(
                YueInput::new("secret", theme, &self.secret)
                    .password(true)
                    .accessibility_label("密码")
                    .width(rems(26.0)),
            )
            .child(
                YueField::new(theme).label("说明").textarea(
                    YueTextarea::new("note", theme, &self.note)
                        .max_length(8)
                        .show_counter(true)
                        .width(rems(26.0)),
                ),
            )
            .child(
                YueField::new(theme)
                    .label("数量")
                    .description("可设置 0 到 100 件")
                    .number_input(
                        YueNumberInput::new("quantity", theme, &self.quantity)
                            .status(self.quantity_status)
                            .disabled(self.quantity_disabled),
                    ),
            )
            .child(
                YueInput::new("narrow", theme, &self.narrow)
                    .accessibility_label("窄槽位")
                    .leading_icon(yue_component::outline::Tag)
                    .trailing("kg")
                    .clearable(true)
                    .width(rems(8.0)),
            )
    }
}

#[gpui_kit::test]
fn numeric_error_colors_reach_the_actual_editor_text_runs(cx: &mut TestAppContext) {
    cx.update(|cx| yue_component::init(cx, theme()).unwrap());
    let (view, cx) = cx.add_window_view(Probe::new);
    for mode in [ThemeMode::Light, ThemeMode::Dark] {
        for accent in [Accent::Azure, Accent::Neutral] {
            for status in [
                yue_component::InputStatus::Error,
                yue_component::InputStatus::Normal,
            ] {
                view.update(cx, |v, cx| {
                    v.theme = YueTheme::new(mode, accent);
                    v.quantity_status = status;
                    cx.notify();
                });
                cx.update(|w, cx| {
                    render(w, cx);
                    let v = view.read(cx);
                    let expected: gpui_kit::Hsla = v.theme.token(if status == yue_component::InputStatus::Error {
                        "--error-foreground"
                    } else { "--input-color" }).unwrap().rgba().unwrap().into();
                    assert_eq!(v.quantity.read(cx).rendered_text_color(), Some(expected),
                        "{mode:?}/{accent:?}/{status:?}: verify the color actually sent to text shaping");
                });
            }
        }
    }
}

#[gpui_kit::test]
fn the_frame_is_the_named_node_of_its_control(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (_, cx) = cx.add_window_view(Probe::new);
    cx.update(|window, cx| {
        render(window, cx);
        assert_eq!(
            node(window, "项目名称").role(),
            Some(Role::TextInput),
            "Base renders the editor as an unlabelled container, so the frame carries the role"
        );
        assert_eq!(node(window, "搜索项目").role(), Some(Role::SearchInput));
        assert_eq!(node(window, "邮箱").role(), Some(Role::EmailInput));
        assert_eq!(node(window, "主页").role(), Some(Role::UrlInput));
        let secret = node(window, "密码");
        assert_eq!(secret.role(), Some(Role::PasswordInput));
        assert_eq!(
            secret.value(),
            None,
            "a masked value must not reach the node"
        );
        assert_eq!(node(window, "说明").role(), Some(Role::MultilineTextInput));
        assert_eq!(
            node(window, "数量").role(),
            Some(Role::SpinButton),
            "the spin button node stays the one GPUI Base publishes"
        );
    });
}

#[gpui_kit::test]
fn an_over_length_value_is_clamped_before_it_is_shown(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (view, cx) = cx.add_window_view(Probe::new);
    let (project, note) = view.update(cx, |probe, _| (probe.project.clone(), probe.note.clone()));
    cx.update(|window, cx| {
        render(window, cx);
        assert_eq!(OVER_LENGTH.chars().count(), LIMIT + 1, "fixture");
        assert_eq!(
            project.read(cx).value().chars().count(),
            LIMIT,
            "an over-length default value is trimmed to the limit"
        );
        assert_eq!(project.read(cx).value().as_str(), CLAMPED);
        assert_eq!(
            node(window, "项目名称").value(),
            Some(CLAMPED),
            "the announced value matches what the field holds"
        );
        assert_eq!(
            note.read(cx).value().chars().count(),
            8,
            "the textarea clamps to its own limit"
        );
    });
}

#[gpui_kit::test]
fn typing_and_pasting_stop_at_the_limit(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (view, cx) = cx.add_window_view(Probe::new);
    let project = view.update(cx, |probe, _| probe.project.clone());
    cx.update(|window, cx| {
        render(window, cx);
        project.update(cx, |input, cx| input.focus(window, cx));
    });
    cx.simulate_input("abc");
    cx.update(|window, cx| {
        render(window, cx);
        let typed = project.read(cx).value();
        assert_eq!(
            typed.chars().count(),
            LIMIT,
            "keystrokes cannot grow a value past the limit, but {typed}"
        );

        // A paste that overshoots keeps its leading characters and the caret the
        // user was working at, so the edit stays undoable through Base.
        project.update(cx, |input, cx| {
            input.set_value("abcdefghijklmnop", window, cx);
            input.set_selected_range(4..4, cx);
        });
        render(window, cx);
        assert_eq!(project.read(cx).value().as_str(), "abcdefghijkl");
        assert_eq!(project.read(cx).selected_range(), 4..4);

        project.update(cx, |input, cx| {
            input.set_value("abcdefghijklmnop", window, cx);
            input.set_selected_range(16..16, cx);
        });
        render(window, cx);
        assert_eq!(project.read(cx).selected_range(), 12..12);
    });
}

#[gpui_kit::test]
fn a_narrow_slot_keeps_its_editor_visible(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (view, cx) = cx.add_window_view(Probe::new);
    let narrow = view.update(cx, |probe, _| probe.narrow.clone());
    cx.update(|window, cx| {
        render(window, cx);
        let frame = node(window, "窄槽位");
        assert!(frame.visible(), "the frame is drawn, not clipped away");
        let width = frame.bounds().size.width;
        assert!(
            (width - px(128.0)).abs() < px(0.5),
            "8rem slot, measured {width}"
        );
        assert_eq!(
            narrow.read(cx).value().as_str(),
            "yue@example.com",
            "affixes and the clear button do not touch the value"
        );
    });
}

#[gpui_kit::test]
fn number_input_renders_a_real_editor_and_exposes_description_and_numeric_bounds(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    let (view, cx) = cx.add_window_view(Probe::new);
    let quantity = view.update(cx, |v, _| v.quantity.clone());
    cx.update(|w, cx| {
        w.activate_window();
        render(w, cx);
        quantity.update(cx, |s, cx| s.focus(w, cx));
    });
    cx.simulate_keystrokes("ctrl-a");
    cx.simulate_input("42");
    cx.update(|w, cx| {
        render(w, cx);
        assert_eq!(quantity.read(cx).value().as_ref(), "42");
        let field = node(w, "数量");
        assert_eq!(field.role(), Some(Role::SpinButton));
        assert_eq!(field.description(), Some("可设置 0 到 100 件"));
        assert_eq!(field.value(), Some("42"));
        assert_eq!(field.numeric_value(), Some(42.));
        assert_eq!(field.min_numeric_value(), Some(0.));
        assert_eq!(field.max_numeric_value(), Some(100.));
        assert_eq!(field.numeric_value_step(), Some(1.));
    });
    cx.simulate_keystrokes("up");
    cx.update(|w, cx| {
        render(w, cx);
        assert_eq!(quantity.read(cx).value().as_ref(), "43");
    });
}

#[gpui_kit::test]
fn horizontal_number_editor_stays_between_controls_at_every_size_and_text_scale(
    cx: &mut TestAppContext,
) {
    use gpui_kit::{ElementId, Pixels};
    use yue_component::InputSize;
    struct NumberProbe {
        input: Entity<InputState>,
        size: InputSize,
    }
    impl Render for NumberProbe {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            YueNumberInput::new("number-layout", theme(), &self.input)
                .accessibility_label("横排数字")
                .size(self.size)
                .width(rems(6.))
        }
    }
    cx.update(|cx| yue_component::init(cx, theme()).unwrap());
    let (view, cx) = cx.add_window_view(|w, cx| NumberProbe {
        input: cx.new(|cx| InputState::new(w, cx).default_value("-99.9")),
        size: InputSize::Medium,
    });
    for rem in [16., 20., 24.] {
        cx.update(|w, _| w.set_rem_size(px(rem)));
        for size in [InputSize::Small, InputSize::Medium, InputSize::Large] {
            view.update(cx, |v, cx| {
                v.size = size;
                cx.notify();
            });
            cx.update(|w, cx| {
                render(w, cx);
                let nodes = gpui_base::test_support::snapshots(w);
                let button = |id| {
                    nodes
                        .iter()
                        .find(|n| n.path().last() == Some(&ElementId::from(id)))
                        .unwrap()
                        .bounds()
                };
                let decrement = button("decrement");
                let increment = button("increment");
                let editor = view.read(cx).input.read(cx).input_bounds();
                assert_eq!(
                    view.read(cx).input.read(cx).presentation().text_align(),
                    gpui_kit::TextAlign::Center,
                    "numeric text must use real editor alignment"
                );
                let text = view
                    .read(cx)
                    .input
                    .read(cx)
                    .range_to_bounds(&(0..5))
                    .unwrap();
                assert!(
                    (text.center().x - editor.center().x).abs() < px(1.),
                    "visible numeric text must be centered in its editor: {text:?} / {editor:?}"
                );
                assert!(
                    editor.left() >= decrement.right(),
                    "{size:?}/{rem}: editor overlaps decrement"
                );
                assert!(
                    editor.right() <= increment.left(),
                    "{size:?}/{rem}: editor overlaps increment"
                );
                assert!(
                    editor.size.width >= px(48. * rem / 16.),
                    "{size:?}/{rem}: editor must retain readable width: {editor:?}"
                );
                let center_distance: Pixels = editor.center().y - increment.center().y;
                assert!(
                    center_distance.abs() < px(1.),
                    "editor and controls share a row"
                );
                assert_eq!(view.read(cx).input.read(cx).value().as_ref(), "-99.9");
            });
        }
    }
}

#[gpui_kit::test]
fn disabling_a_focused_number_editor_blocks_typing_and_stepping(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (view, cx) = cx.add_window_view(Probe::new);
    let quantity = view.update(cx, |v, _| v.quantity.clone());
    cx.update(|w, cx| {
        w.activate_window();
        render(w, cx);
        quantity.update(cx, |s, cx| s.focus(w, cx));
    });
    view.update(cx, |v, cx| {
        v.quantity_disabled = true;
        cx.notify();
    });
    cx.update(render);
    cx.simulate_input("7");
    cx.simulate_keystrokes("up");
    cx.update(|w, cx| {
        render(w, cx);
        assert_eq!(quantity.read(cx).value().as_ref(), "12");
        assert_eq!(node(w, "数量").disabled(), Some(true));
    });
}

struct FormProbe {
    theme: YueTheme,
    form: Entity<YueForm>,
    name: Entity<InputState>,
    email: Entity<InputState>,
}

impl FormProbe {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let name = cx.new(|cx| InputState::new(window, cx).placeholder("例如 Roadmap"));
        let email = cx.new(|cx| InputState::new(window, cx).placeholder("name@example.com"));
        let form = cx.new(|_| YueForm::new());
        form.update(cx, |form, cx| {
            form.require("name", &name, "请填写项目名称");
            form.rule("email", &email, |value, _| {
                let complete = value
                    .rsplit_once('@')
                    .is_some_and(|(_, domain)| !domain.is_empty() && domain.contains('.'));
                (!complete).then(|| SharedString::from("填写完整的邮箱"))
            });
            form.attach(&name, window, cx);
            form.attach(&email, window, cx);
        });
        Self {
            theme: theme(),
            form,
            name,
            email,
        }
    }
}

impl Render for FormProbe {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        div()
            .flex()
            .flex_col()
            .gap_2()
            .p_4()
            .child(
                YueField::new(theme)
                    .label("项目名称")
                    .required(true)
                    .bind(&self.form, "name")
                    .input(YueInput::new("form-name", theme, &self.name).width(rems(26.0))),
            )
            .child(
                YueField::new(theme)
                    .label("邮箱")
                    .bind(&self.form, "email")
                    .input(
                        YueInput::new("form-email", theme, &self.email)
                            .input_type(InputType::Email)
                            .width(rems(26.0)),
                    ),
            )
    }
}

#[gpui_kit::test]
fn submit_reveals_errors_and_locates_the_first_failure(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (view, cx) = cx.add_window_view(FormProbe::new);
    let (form, name, email) = view.update(cx, |probe, _| {
        (probe.form.clone(), probe.name.clone(), probe.email.clone())
    });
    cx.update(|window, cx| {
        render(window, cx);
        email.update(cx, |input, cx| {
            input.set_value("roadmap@example", window, cx)
        });
        render(window, cx);
        assert!(
            form.read(cx).error("name", cx).is_none(),
            "an untouched form shows nothing"
        );

        assert!(
            !form.update(cx, |form, cx| form.submit(window, cx)),
            "an empty required field blocks the submit"
        );
        render(window, cx);
        assert_eq!(
            form.read(cx).error("name", cx).unwrap().as_str(),
            "请填写项目名称"
        );
        assert_eq!(
            node(window, "项目名称").focused(),
            Some(true),
            "submit moves the focus to the first field that still fails"
        );
        assert_eq!(node(window, "邮箱").focused(), Some(false));

        name.update(cx, |input, cx| input.set_value("Roadmap", window, cx));
        render(window, cx);
        assert!(
            form.read(cx).error("name", cx).is_none(),
            "a corrected field loses its error without a second submit"
        );
        assert!(!form.update(cx, |form, cx| form.submit(window, cx)));
        render(window, cx);
        assert_eq!(node(window, "邮箱").focused(), Some(true));
        assert_eq!(
            form.read(cx).error("email", cx).unwrap().as_str(),
            "填写完整的邮箱"
        );

        email.update(cx, |input, cx| {
            input.set_value("roadmap@example.com", window, cx)
        });
        assert!(form.update(cx, |form, cx| form.submit(window, cx)));
        assert!(form.read(cx).is_valid(cx));
        form.update(cx, |form, cx| form.reset(cx));
        assert!(
            form.read(cx).error("email", cx).is_none(),
            "reset hides the results again"
        );
    });
}

#[gpui_kit::test]
fn enter_in_an_attached_field_runs_the_same_validation(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (view, cx) = cx.add_window_view(FormProbe::new);
    let (form, name, email) = view.update(cx, |probe, _| {
        (probe.form.clone(), probe.name.clone(), probe.email.clone())
    });
    cx.update(|window, cx| {
        render(window, cx);
        email.update(cx, |input, cx| {
            input.set_value("roadmap@example.com", window, cx);
            input.focus(window, cx);
        });
        render(window, cx);
        assert_eq!(node(window, "邮箱").focused(), Some(true));
        assert!(
            form.read(cx).error("name", cx).is_none(),
            "nothing is revealed before the first submit"
        );
    });

    cx.simulate_keystrokes("enter");

    cx.update(|window, cx| {
        assert_eq!(
            node(window, "项目名称").focused(),
            Some(true),
            "the attached field's enter key submits the form"
        );
        assert_eq!(
            form.read(cx).error("name", cx).unwrap().as_str(),
            "请填写项目名称"
        );
        assert!(
            name.read(cx).value().is_empty(),
            "the failure is the empty name, not the address"
        );
        assert_eq!(email.read(cx).value().as_str(), "roadmap@example.com");
    });
}
