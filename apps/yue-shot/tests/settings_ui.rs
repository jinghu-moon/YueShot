//! Existing screenshot settings UI acceptance, moved from the old combined product.
use gpui_kit::{AppContext, Entity, TestAppContext, base::Root, px, size, test::TestWindowExt};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
use yue_shot::settings::{Settings, SettingsChanged, SettingsStore, SettingsView};
use yue_shot_capture::units::{LabelDisplay, LengthUnit};
static SEQ: AtomicU64 = AtomicU64::new(0);
fn init(cx: &mut gpui_kit::App) {
    gpui_kit::init(cx);
    yue_component::init(
        cx,
        yue_component::YueTheme::new(
            yue_component::ThemeMode::Light,
            yue_component::Accent::Azure,
        ),
    )
    .unwrap();
}

#[test]
fn product_text_roles_keep_system_families_and_upstream_metrics() {
    use yue_component::{Accent, TextRole, ThemeMode, TypeStyle, YueTheme, typography};
    for mode in [ThemeMode::Light, ThemeMode::Dark] {
        let theme = YueTheme::new(mode, Accent::Azure);
        assert_eq!(typography::ui_font(theme).family.as_ref(), "Segoe UI");
        assert_eq!(typography::number_font(theme), typography::ui_font(theme));
        for role in TextRole::ALL {
            let style = role.style(theme);
            let token = theme.token(role.token_name()).expect("role token");
            let original = TypeStyle::parse(token.value()).expect("token metrics");
            assert_eq!(
                (style.weight, style.size, style.line_height),
                (original.weight, original.size, original.line_height),
                "{role:?} metrics"
            );
            let (family, fallback) = if role == TextRole::Code {
                ("Consolas", "Courier New")
            } else {
                ("Segoe UI", "Microsoft YaHei UI")
            };
            assert_eq!(style.font.family.as_ref(), family, "{role:?}");
            assert_eq!(style.font.fallbacks, [fallback], "{role:?}");
        }
    }
}
fn temp_root(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "yueshot-settings-ui-{name}-{}-{}",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    ))
}
/// The settings page's acceptance is "change it, and the change is real": a switch that
/// only looks toggled is the failure mode this catches.
#[gpui_kit::test]
fn toggling_the_setting_writes_it_to_disk(cx: &mut TestAppContext) {
    cx.update(init);

    let data = temp_root("settings-switch");
    let store = SettingsStore::new(&data);
    store.save(&Settings::default()).expect("seed settings");

    let mut view: Option<Entity<SettingsView>> = None;
    let handle = cx.open_window(size(px(640.), px(480.)), |window, cx| {
        let entity = cx.new(|_| SettingsView::new(SettingsStore::new(&data)));
        view = Some(entity.clone());
        Root::new(entity, window, cx)
    });
    let view = view.expect("view constructed");

    cx.update_window(handle.into(), |_, window, cx| {
        window.render_frame(cx);
        assert!(
            view.read(cx).settings().deep_select_text_runs,
            "the switch starts from the file, which holds the default"
        );

        window.click("deep-select-text-runs", cx);

        assert!(!view.read(cx).settings().deep_select_text_runs);
    })
    .unwrap();

    // The assertion that matters: the file changed, not just the rendered state.
    assert!(
        !SettingsStore::new(&data)
            .load()
            .unwrap()
            .deep_select_text_runs
    );
    let _ = std::fs::remove_dir_all(&data);
}

/// The label unit has the same acceptance as a switch: picking `dp` has to reach the file,
/// because that file is what the shell hands to the next capture session.
#[gpui_kit::test]
fn picking_the_label_unit_writes_it_to_disk(cx: &mut TestAppContext) {
    cx.update(init);

    let data = temp_root("settings-unit");
    let store = SettingsStore::new(&data);
    store.save(&Settings::default()).expect("seed settings");

    let mut view: Option<Entity<SettingsView>> = None;
    let handle = cx.open_window(size(px(640.), px(480.)), |window, cx| {
        let entity = cx.new(|_| SettingsView::new(SettingsStore::new(&data)));
        view = Some(entity.clone());
        Root::new(entity, window, cx)
    });
    let view = view.expect("view constructed");

    cx.update_window(handle.into(), |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(
            view.read(cx).settings().length_unit,
            LengthUnit::Px,
            "the group starts from the file, which holds the default"
        );

        // Yue controls use their theme-defined target sizes. Reach the unit row
        // through the actual scrolling surface in this short window.
        window.scroll(
            "settings-scroll",
            gpui_kit::ScrollDelta::Pixels(gpui_kit::point(px(0.), px(-160.))),
            cx,
        );
        window.click("label-unit-dp", cx);

        assert_eq!(view.read(cx).settings().length_unit, LengthUnit::Dp);
    })
    .unwrap();

    assert_eq!(
        SettingsStore::new(&data).load().unwrap().length_unit,
        LengthUnit::Dp
    );
    let _ = std::fs::remove_dir_all(&data);
}

/// The unit suffix is a switch like the others, and it has the same acceptance: clicking it has to
/// reach the file the next capture session is started from.
#[gpui_kit::test]
fn the_unit_suffix_switch_writes_to_disk(cx: &mut TestAppContext) {
    cx.update(init);

    let data = temp_root("settings-unit-suffix");
    SettingsStore::new(&data)
        .save(&Settings::default())
        .expect("seed settings");

    let mut view: Option<Entity<SettingsView>> = None;
    let handle = cx.open_window(size(px(640.), px(480.)), |window, cx| {
        let entity = cx.new(|_| SettingsView::new(SettingsStore::new(&data)));
        view = Some(entity.clone());
        Root::new(entity, window, cx)
    });
    let view = view.expect("view constructed");

    cx.update_window(handle.into(), |_, window, cx| {
        window.render_frame(cx);
        assert!(view.read(cx).settings().show_unit, "the default is on");

        assert!(!window.find("label-unit-suffix").visible());
        window.scroll(
            "settings-scroll",
            gpui_kit::ScrollDelta::Pixels(gpui_kit::point(px(0.), px(-450.))),
            cx,
        );
        assert!(
            window.find("label-unit-suffix").visible(),
            "the lower setting must be reachable in a compact window: scroll={:?}, target={:?}",
            window.find("settings-scroll").bounds(),
            window.find("label-unit-suffix").bounds()
        );
        window.click("label-unit-suffix", cx);

        assert!(!view.read(cx).settings().show_unit);
    })
    .unwrap();

    assert!(!SettingsStore::new(&data).load().unwrap().show_unit);
    let _ = std::fs::remove_dir_all(&data);
}

/// The badge's menu in the overlay is the second way to change the display settings, and the page is
/// what writes them down: the shell hands the menu's result to the page, not to the file, so the two
/// cannot disagree (docs/23 §14.53).
#[gpui_kit::test]
fn adopting_the_overlay_label_writes_it_to_disk(cx: &mut TestAppContext) {
    cx.update(init);

    let data = temp_root("settings-adopt-label");
    SettingsStore::new(&data)
        .save(&Settings::default())
        .expect("seed settings");

    let mut view: Option<Entity<SettingsView>> = None;
    let handle = cx.open_window(size(px(640.), px(480.)), |window, cx| {
        let entity = cx.new(|_| SettingsView::new(SettingsStore::new(&data)));
        view = Some(entity.clone());
        Root::new(entity, window, cx)
    });
    let view = view.expect("view constructed");

    let from_the_menu = LabelDisplay {
        length_unit: LengthUnit::Dp,
        show_size: false,
        show_coordinates: false,
        show_unit: false,
    };
    cx.update_window(handle.into(), |_, window, cx| {
        window.render_frame(cx);
        assert!(
            view.update(cx, |view, cx| view.adopt_label(from_the_menu, cx)),
            "the menu's values differ from the page's"
        );
        // The same values again are not a change: a capture the user never opened the menu in must
        // not rewrite the file, and must not announce a change to capture either.
        assert!(
            !view.update(cx, |view, cx| view.adopt_label(from_the_menu, cx)),
            "nothing changed the second time"
        );
    })
    .unwrap();

    let saved = SettingsStore::new(&data).load().unwrap();
    assert_eq!(saved.length_unit, LengthUnit::Dp);
    assert!(!saved.show_coordinates && !saved.show_unit);
    let _ = std::fs::remove_dir_all(&data);
}

/// The overlay switches are saved and announced, which is how the shell hands them to capture
/// for the next session.
#[gpui_kit::test]
fn the_overlay_switches_are_saved_and_announced(cx: &mut TestAppContext) {
    cx.update(init);

    let data = temp_root("settings-overlay");
    SettingsStore::new(&data)
        .save(&Settings::default())
        .expect("seed settings");

    let mut view: Option<Entity<SettingsView>> = None;
    let handle = cx.open_window(size(px(640.), px(480.)), |window, cx| {
        let entity = cx.new(|_| SettingsView::new(SettingsStore::new(&data)));
        view = Some(entity.clone());
        Root::new(entity, window, cx)
    });
    let view = view.expect("view constructed");
    let announced = std::rc::Rc::new(std::cell::RefCell::new(Vec::<Settings>::new()));
    let sink = announced.clone();
    let _subscription = cx.update(|cx| {
        cx.subscribe(&view, move |_, event: &SettingsChanged, _| {
            sink.borrow_mut().push(event.0.clone());
        })
    });

    cx.update_window(handle.into(), |_, window, cx| {
        window.render_frame(cx);
        assert!(view.read(cx).settings().mask_blur);
        assert!(view.read(cx).settings().shortcut_guide);
        window.click("mask-blur", cx);
        window.render_frame(cx);
        window.click("shortcut-guide", cx);
    })
    .unwrap();

    let saved = SettingsStore::new(&data).load().unwrap();
    assert!(!saved.mask_blur && !saved.shortcut_guide);
    let announced = announced.borrow();
    assert_eq!(announced.len(), 2);
    assert!(!announced[0].mask_blur && announced[0].shortcut_guide);
    assert_eq!(announced[1], saved);
    let overlay = saved.overlay_preferences();
    assert!(!overlay.mask_blur && !overlay.shortcut_guide);
    let _ = std::fs::remove_dir_all(&data);
}

#[gpui_kit::test]
fn close_to_tray_is_persisted_and_announced_without_changing_capture_preferences(
    cx: &mut TestAppContext,
) {
    cx.update(init);
    let data = temp_root("close-to-tray");
    SettingsStore::new(&data)
        .save(&Settings::default())
        .unwrap();
    let mut view = None;
    let handle = cx.open_window(size(px(640.), px(480.)), |window, cx| {
        let entity = cx.new(|_| SettingsView::new(SettingsStore::new(&data)));
        view = Some(entity.clone());
        Root::new(entity, window, cx)
    });
    let view = view.unwrap();
    let announced = std::rc::Rc::new(std::cell::RefCell::new(Vec::<Settings>::new()));
    let sink = announced.clone();
    let _subscription = cx.update(|cx| {
        cx.subscribe(&view, move |_, event: &SettingsChanged, _| {
            sink.borrow_mut().push(event.0.clone());
        })
    });
    cx.update_window(handle.into(), |_, window, cx| {
        window.render_frame(cx);
        window.scroll(
            "settings-scroll",
            gpui_kit::ScrollDelta::Pixels(gpui_kit::point(px(0.), px(-2000.))),
            cx,
        );
        assert!(view.read(cx).settings().close_to_tray);
        window.click("close-to-tray", cx);
        assert!(!view.read(cx).settings().close_to_tray);
    })
    .unwrap();
    let saved = SettingsStore::new(&data).load().unwrap();
    assert!(!saved.close_to_tray);
    assert_eq!(
        saved.overlay_preferences(),
        Settings::default().overlay_preferences()
    );
    assert_eq!(*announced.borrow(), vec![saved]);
    let _ = std::fs::remove_dir_all(data);
}
