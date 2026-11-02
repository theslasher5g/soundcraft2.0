//! Window › Mix Docked at Bottom: compact Mix strips under the Edit window, so Edit and Mix are
//! seen together without a second window.
use egui::vec2;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::json;
use soundcraft_ui_egui::{MainWindow, Services, SoundApp};

fn harness(app: SoundApp) -> Harness<'static, SoundApp> {
    Harness::builder().with_size(vec2(1600.0, 1000.0)).build_ui_state(
        |ui, app: &mut SoundApp| {
            app.logic(&ui.ctx().clone());
            app.ui(ui);
        },
        app,
    )
}

fn demo() -> SoundApp {
    SoundApp::new(soundcraft_engine::demo::demo_engine(), None, Services::default())
}

/// How many "Mute for Kick" controls are on screen: the Edit window's track header, plus the
/// strip when the Mix is shown.
fn mutes(h: &Harness<'_, SoundApp>) -> usize {
    h.query_all_by_label("Mute for Kick").count()
}

#[test]
fn the_dock_is_off_by_default_and_the_command_toggles_it() {
    let mut app = demo();
    assert!(!app.ui.mix_dock);
    app.run("window.mix_dock", json!({})).unwrap();
    assert!(app.ui.mix_dock);
    app.run("window.mix_dock", json!({"value": false})).unwrap();
    assert!(!app.ui.mix_dock);
    app.run("window.mix_dock", json!({"value": true})).unwrap();
    assert!(app.ui.mix_dock);
}

#[test]
fn docking_adds_the_strips_under_the_edit_window_and_undocking_removes_them() {
    let mut h = harness(demo());
    h.run_steps(4);
    let without = mutes(&h);
    assert!(without >= 1, "the Edit window's header has a Mute for Kick");
    h.state_mut().run("window.mix_dock", json!({"value": true})).unwrap();
    h.run_steps(4);
    assert_eq!(h.state().ui.window, MainWindow::Edit, "still the Edit window");
    assert_eq!(mutes(&h), without + 1, "plus the compact strip's Mute");
    h.state_mut().run("window.mix_dock", json!({"value": false})).unwrap();
    h.run_steps(4);
    assert_eq!(mutes(&h), without);
}

#[test]
fn the_compact_strip_is_live_and_acts_on_the_session() {
    let mut app = demo();
    app.run("window.mix_dock", json!({"value": true})).unwrap();
    let mut h = harness(app);
    h.run_steps(4);
    let before = h.state().engine.session().track_by_name("Kick").unwrap().mixer.mute;
    // The strip's Mute is the last one in the tree (the Edit header's comes first).
    h.query_all_by_label("Mute for Kick").last().unwrap().click();
    h.run_steps(3);
    assert_ne!(h.state().engine.session().track_by_name("Kick").unwrap().mixer.mute, before);
}

#[test]
fn the_docked_strips_leave_the_mix_window_settings_alone() {
    let mut app = demo();
    let views = app.ui.mix_views.clone();
    app.run("window.mix_dock", json!({"value": true})).unwrap();
    let mut h = harness(app);
    h.run_steps(5);
    assert_eq!(h.state().ui.mix_views, views, "the compact form borrows the section list for a frame and gives it back");
    assert!(!h.state().ui.mix_dock_height.is_nan() && h.state().ui.mix_dock_height >= 200.0);
}

#[test]
fn no_dock_when_the_mix_is_in_its_own_window_or_fills_the_main_one() {
    let mut app = demo();
    app.run("window.mix_dock", json!({"value": true})).unwrap();
    app.run("window.mix", json!({})).unwrap();
    let mut h = harness(app);
    h.run_steps(4);
    assert!(!h.state().docked_mix_shown(), "the full Mix window is showing already");
}

#[test]
fn the_setting_survives_a_restart_and_old_preferences_have_none() {
    let mut app = demo();
    app.run("window.mix_dock", json!({"value": true})).unwrap();
    app.ui.mix_dock_height = 410.0;
    let saved = serde_json::to_string(&app.ui).unwrap();
    let back: soundcraft_ui_egui::UiState = serde_json::from_str(&saved).unwrap();
    assert!(back.mix_dock);
    assert_eq!(back.mix_dock_height, 410.0);
    let old: soundcraft_ui_egui::UiState = serde_json::from_str("{}").unwrap();
    assert!(!old.mix_dock, "off for preferences from before the setting");
}
