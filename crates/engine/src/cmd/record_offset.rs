//! Setup › Record Offset: how much earlier a new recording is placed to make up for the latency
//! between playing and recording (see [`crate::io::record_offset`]).
use super::more_util::{set_text, text};
use super::{CommandSpec, always};

use crate::io::record_offset::{RecordOffset, ReportedLatency};
use crate::{Engine, Result, cmd};
use serde_json::Value;

const ID: &str = "setup.record_offset";
const KEY: &str = "record.offset";

pub fn specs() -> Vec<CommandSpec> {
    vec![cmd!(
        noundo "setup.record_offset",
        "Record Offset...",
        ["Setup"],
        None,
        "{ms?: 0..=5000 | samples?: 0..=1000000, auto?: bool, reset?: bool} — the record offset: new recordings (also loop-record and punch) are placed this much earlier; a start before 0 cuts the head of the take. `ms` and `samples` set the manual value (not both); `auto` adds the latency the audio driver reports (input: capture to callback, output: callback to playback; many WASAPI/USB devices report none, so the manual value stays necessary); `reset` restores 0 ms, auto off. Returns the setting and the resulting `total_samples`. In the desktop app it is kept with the preferences (per machine); headless it is kept in the session and `reported_samples` is 0",
        always,
        record_offset
    )]
}

/// The setting kept in the session (headless). An unreadable value counts as the default.
fn stored(e: &Engine) -> RecordOffset {
    text(&e.session().edit, KEY).and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

fn record_offset(e: &mut Engine, p: &Value) -> Result<Value> {
    let mut setting = stored(e);
    setting.apply(ID, p)?;
    if let Ok(json) = serde_json::to_string(&setting) {
        set_text(&mut e.session_mut().edit, KEY, Some(&json));
    }
    Ok(setting.report(e.session().sample_rate.hz(), ReportedLatency::default()))
}

#[cfg(test)]
mod tests {
    use crate::Engine;
    use serde_json::json;

    #[test]
    fn the_default_is_no_offset_and_no_automation() {
        let mut e = Engine::default();
        let r = e.execute(super::ID, &json!({})).unwrap();
        assert_eq!((r["total_samples"].as_i64(), r["auto"].as_bool()), (Some(0), Some(false)));
        assert_eq!(r["manual"], json!({"samples": 0}));
    }

    #[test]
    fn ms_and_samples_are_stored_and_reported_at_the_session_rate() {
        let mut e = Engine::default();
        let rate = e.session().sample_rate.hz();
        let r = e.execute(super::ID, &json!({"ms": 10})).unwrap();
        assert_eq!(r["total_samples"].as_i64(), Some(i64::from(rate) / 100));
        let r = e.execute(super::ID, &json!({"samples": 123})).unwrap();
        assert_eq!((r["manual"].clone(), r["total_samples"].as_i64()), (json!({"samples": 123}), Some(123)));
        let again = e.execute(super::ID, &json!({})).unwrap();
        assert_eq!(again["total_samples"].as_i64(), Some(123), "kept between calls");
    }

    #[test]
    fn auto_adds_nothing_without_a_reported_latency_and_can_be_switched_off() {
        let mut e = Engine::default();
        let r = e.execute(super::ID, &json!({"samples": 50, "auto": true})).unwrap();
        assert_eq!((r["auto"].as_bool(), r["reported_samples"].as_i64(), r["total_samples"].as_i64()), (Some(true), Some(0), Some(50)));
        let r = e.execute(super::ID, &json!({"auto": false})).unwrap();
        assert_eq!(r["auto"].as_bool(), Some(false));
    }

    #[test]
    fn bad_values_are_refused_and_change_nothing() {
        let mut e = Engine::default();
        e.execute(super::ID, &json!({"samples": 77})).unwrap();
        for bad in [json!({"ms": -1}), json!({"samples": 5_000_000}), json!({"ms": 1, "samples": 1}), json!({"auto": "yes"}), json!({"samples": 1.5})]
        {
            assert!(e.execute(super::ID, &bad).is_err(), "{bad}");
        }
        assert_eq!(e.execute(super::ID, &json!({})).unwrap()["total_samples"].as_i64(), Some(77));
    }

    #[test]
    fn reset_restores_the_default() {
        let mut e = Engine::default();
        e.execute(super::ID, &json!({"ms": 30, "auto": true})).unwrap();
        let r = e.execute(super::ID, &json!({"reset": true})).unwrap();
        assert_eq!((r["total_samples"].as_i64(), r["auto"].as_bool()), (Some(0), Some(false)));
    }

    #[test]
    fn it_is_registered_in_the_setup_menu() {
        let c = crate::cmd::find_command(super::ID).unwrap();
        assert_eq!((c.menu, c.label), (&["Setup"][..], "Record Offset..."));
        assert!(c.params.contains("samples") && c.params.contains("auto"));
    }
}
