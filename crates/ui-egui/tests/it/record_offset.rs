//! `setup.record_offset` in the desktop app: one command id for the menu, CLI and control channel,
//! kept with the preferences (per machine), with the automatic part off by default.
use serde_json::json;
use soundcraft_ui_egui::{Services, SoundApp, UiState};

fn new_app() -> SoundApp {
    SoundApp::new(soundcraft_engine::Engine::default(), None, Services::default())
}

#[test]
fn the_default_is_zero_and_manual() {
    let mut app = new_app();
    let r = app.run("setup.record_offset", json!({})).unwrap();
    assert_eq!((r["total_samples"].as_i64(), r["auto"].as_bool(), r["manual_samples"].as_i64()), (Some(0), Some(false), Some(0)));
}

#[test]
fn the_value_is_kept_with_the_preferences_not_the_session() {
    let mut app = new_app();
    let rate = i64::from(app.engine.session().sample_rate.hz());
    let r = app.run("setup.record_offset", json!({"ms": 20, "auto": false})).unwrap();
    assert_eq!(r["total_samples"].as_i64(), Some(rate / 50));
    let saved = serde_json::to_string(&app.ui).unwrap();
    let restored: UiState = serde_json::from_str(&saved).unwrap();
    assert_eq!(restored.record_offset, app.ui.record_offset, "survives a restart");
    let old: UiState = serde_json::from_str("{}").unwrap();
    assert_eq!(old.record_offset.manual_samples(48_000), 0, "preferences from before the setting have none");
    let again = app.run("setup.record_offset", json!({})).unwrap();
    assert_eq!(again["total_samples"], r["total_samples"], "kept between calls");
}

#[test]
fn invalid_values_change_nothing() {
    let mut app = new_app();
    app.run("setup.record_offset", json!({"samples": 96})).unwrap();
    for bad in [json!({"ms": -3}), json!({"samples": 99_999_999}), json!({"ms": 1, "samples": 1}), json!({"auto": "yes"})] {
        assert!(app.run("setup.record_offset", bad.clone()).is_err(), "{bad}");
    }
    assert_eq!(app.run("setup.record_offset", json!({})).unwrap()["total_samples"], 96);
}

#[test]
fn auto_adds_nothing_while_no_device_reports_a_latency() {
    let mut app = new_app();
    let r = app.run("setup.record_offset", json!({"samples": 10, "auto": true})).unwrap();
    assert_eq!(r["auto"], true);
    assert_eq!(r["total_samples"].as_i64(), Some(10 + r["reported_samples"].as_i64().unwrap()));
}

#[test]
fn the_menu_and_the_command_share_one_id() {
    let spec = soundcraft_engine::find_command("setup.record_offset").expect("registered in the engine");
    assert_eq!(spec.menu, &["Setup"]);
}
