//! Record offset compensation: where a new take goes so it lines up with what was played.
//!
//! What the microphone hears reaches the file late: input hardware, the driver and its buffer
//! on the way in, and, because the performer plays along to what the speakers or headphones
//! play, the output path on the way out. A take recorded at the playhead therefore sits behind
//! the playback it was played to. The **record offset** (in samples) is how much earlier the
//! take is placed.
//!
//! Two sources add up:
//! 1. a **manual** value (milliseconds or samples), kept with the preferences, default 0;
//! 2. optionally the latency the audio driver **reports** (`auto`, off by default): input is the
//!    time between capture and the callback, output the time between the callback and playback.
//!    Many drivers, WASAPI shared mode and USB class-compliant devices above all, do not report
//!    the real round trip (the reported time often leaves out the converter and USB buffering),
//!    so the manual value stays necessary. Calibrate it with a clap test (see
//!    `docs/record-offset.md`).
//!
//! Everything here is pure arithmetic on numbers that may come from commands or drivers, so
//! it saturates and clamps instead of trusting them.

use serde_json::{Value, json};
use soundcraft_time::Samples;
use std::time::Duration;

use crate::{EngineError, Result};

/// The largest manual offset in milliseconds a command accepts.
pub const MAX_MANUAL_MS: f64 = 5_000.0;
/// The largest manual offset in samples a command accepts.
pub const MAX_MANUAL_SAMPLES: Samples = 1_000_000;
/// The largest offset ever applied (manual and reported together), in samples.
pub const MAX_TOTAL_SAMPLES: Samples = 2_000_000;

/// The manual part of the offset, as the user entered it.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Manual {
    Ms(f64),
    Samples(Samples),
}

impl Default for Manual {
    fn default() -> Self {
        Manual::Samples(0)
    }
}

/// The latency the audio drivers report, as measured in the stream callbacks.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ReportedLatency {
    /// Capture to input callback (the microphone's way into the program).
    pub input: Duration,
    /// Output callback to playback (the way out to the headphones).
    pub output: Duration,
}

impl ReportedLatency {
    /// Input and output latency together at `rate`, in samples (0 for a rate of 0).
    pub fn samples(&self, rate: u32) -> Samples {
        seconds_to_samples(self.input.as_secs_f64() + self.output.as_secs_f64(), rate)
    }
}

/// The record offset setting: kept per machine, not in sessions.
#[derive(Debug, Clone, Copy, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct RecordOffset {
    pub manual: Manual,
    /// Add the latency the driver reports to the manual value. Off by default.
    pub auto: bool,
}

/// `seconds` at `rate` as whole samples; NaN, negative and huge values come out as 0 or the cap.
fn seconds_to_samples(seconds: f64, rate: u32) -> Samples {
    let n = (seconds * f64::from(rate)).round();
    if !n.is_finite() || n <= 0.0 { 0 } else { n.min(MAX_TOTAL_SAMPLES as f64) as Samples }
}

impl RecordOffset {
    /// The manual part in samples at `rate` (never negative, at most [`MAX_MANUAL_SAMPLES`]
    /// for samples and [`MAX_MANUAL_MS`] for milliseconds).
    pub fn manual_samples(&self, rate: u32) -> Samples {
        match self.manual {
            Manual::Samples(n) => n.clamp(0, MAX_MANUAL_SAMPLES),
            Manual::Ms(ms) if ms.is_finite() => seconds_to_samples(ms.clamp(0.0, MAX_MANUAL_MS) / 1000.0, rate),
            Manual::Ms(_) => 0,
        }
    }

    /// The offset to apply to a take at `rate`, given what the drivers `reported`.
    pub fn total(&self, rate: u32, reported: ReportedLatency) -> Samples {
        let auto = if self.auto { reported.samples(rate) } else { 0 };
        self.manual_samples(rate).saturating_add(auto).min(MAX_TOTAL_SAMPLES)
    }

    /// Apply the parameters of `setup.record_offset` (`ms` or `samples`, `auto`, `reset`). The
    /// setting stays as it was when a parameter is invalid.
    pub fn apply(&mut self, id: &str, p: &Value) -> Result<()> {
        let bad = |msg: &str| EngineError::BadParams(id.to_string(), msg.to_string());
        let mut next = if p.get("reset").and_then(Value::as_bool) == Some(true) { RecordOffset::default() } else { *self };
        let (ms, samples) = (p.get("ms").filter(|v| !v.is_null()), p.get("samples").filter(|v| !v.is_null()));
        if ms.is_some() && samples.is_some() {
            return Err(bad("give `ms` or `samples`, not both"));
        }
        if let Some(v) = ms {
            let ms = v.as_f64().filter(|m| m.is_finite() && (0.0..=MAX_MANUAL_MS).contains(m));
            next.manual = Manual::Ms(ms.ok_or_else(|| bad("`ms` must be a number from 0 to 5000"))?);
        }
        if let Some(v) = samples {
            let n = v.as_i64().filter(|n| (0..=MAX_MANUAL_SAMPLES).contains(n));
            next.manual = Manual::Samples(n.ok_or_else(|| bad("`samples` must be a whole number from 0 to 1000000"))?);
        }
        if let Some(v) = p.get("auto") {
            next.auto = v.as_bool().ok_or_else(|| bad("`auto` must be true or false"))?;
        }
        *self = next;
        Ok(())
    }

    /// What `setup.record_offset` returns: the setting, and what it comes to at `rate`.
    pub fn report(&self, rate: u32, reported: ReportedLatency) -> Value {
        let (key, value) = match self.manual {
            Manual::Ms(ms) => ("ms", json!(ms)),
            Manual::Samples(n) => ("samples", json!(n)),
        };
        json!({
            "manual": {key: value},
            "manual_samples": self.manual_samples(rate),
            "auto": self.auto,
            "reported_samples": reported.samples(rate),
            "total_samples": self.total(rate, reported),
            "sample_rate": rate,
        })
    }
}

/// Where a take starts once `offset` samples are taken off its start: `(clip start, skip)`. The
/// clip starts `offset` earlier than `start`; when that would be before 0 the clip starts at 0
/// and the first `skip` samples of the take, which would lie before the start of the timeline,
/// are cut off. Never negative, never overflowing.
pub fn compensate(start: Samples, offset: Samples) -> (Samples, Samples) {
    let moved = start.saturating_sub(offset.max(0));
    if moved >= 0 { (moved, 0) } else { (0, moved.saturating_neg()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compensate_moves_the_start_earlier() {
        assert_eq!(compensate(48_000, 0), (48_000, 0));
        assert_eq!(compensate(48_000, 480), (47_520, 0));
        assert_eq!(compensate(480, 480), (0, 0));
    }

    #[test]
    fn compensate_cuts_the_head_when_the_start_is_too_close_to_zero() {
        assert_eq!(compensate(0, 480), (0, 480));
        assert_eq!(compensate(100, 480), (0, 380));
        assert_eq!(compensate(-10, 5), (0, 15));
    }

    #[test]
    fn compensate_survives_extreme_values() {
        assert_eq!(compensate(0, Samples::MAX), (0, Samples::MAX));
        assert_eq!(compensate(Samples::MIN, Samples::MAX), (0, Samples::MAX));
        assert_eq!(compensate(Samples::MAX, 0), (Samples::MAX, 0));
        assert_eq!(compensate(5, -100), (5, 0), "a negative offset is no offset");
    }

    #[test]
    fn ms_become_samples_at_each_rate() {
        for (rate, expect) in [(44_100, 441), (48_000, 480), (96_000, 960)] {
            let o = RecordOffset { manual: Manual::Ms(10.0), auto: false };
            assert_eq!(o.manual_samples(rate), expect, "{rate}");
        }
        let o = RecordOffset { manual: Manual::Ms(10.4), auto: false };
        assert_eq!(o.manual_samples(44_100), 459, "rounded to the nearest sample");
    }

    #[test]
    fn samples_do_not_depend_on_the_rate() {
        let o = RecordOffset { manual: Manual::Samples(256), auto: false };
        assert_eq!([44_100, 48_000, 96_000].map(|r| o.manual_samples(r)), [256; 3]);
    }

    #[test]
    fn zero_and_hostile_manual_values_are_harmless() {
        let zero = RecordOffset::default();
        assert_eq!(zero.total(48_000, ReportedLatency::default()), 0);
        for manual in [Manual::Ms(f64::NAN), Manual::Ms(f64::INFINITY), Manual::Ms(-5.0), Manual::Samples(-1), Manual::Samples(i64::MIN)] {
            assert_eq!(RecordOffset { manual, auto: false }.manual_samples(48_000), 0, "{manual:?}");
        }
        assert_eq!(RecordOffset { manual: Manual::Samples(i64::MAX), auto: false }.manual_samples(48_000), MAX_MANUAL_SAMPLES);
        assert_eq!(RecordOffset { manual: Manual::Ms(1e300), auto: false }.manual_samples(96_000), 480_000);
        assert_eq!(RecordOffset { manual: Manual::Ms(10.0), auto: false }.manual_samples(0), 0);
    }

    #[test]
    fn reported_latency_counts_only_when_auto_is_on() {
        let reported = ReportedLatency { input: Duration::from_millis(5), output: Duration::from_millis(7) };
        assert_eq!(reported.samples(48_000), 576);
        let manual = Manual::Samples(100);
        assert_eq!(RecordOffset { manual, auto: false }.total(48_000, reported), 100);
        assert_eq!(RecordOffset { manual, auto: true }.total(48_000, reported), 676);
        assert_eq!(RecordOffset { manual, auto: true }.total(96_000, reported), 1_252);
        assert_eq!(RecordOffset { manual, auto: true }.total(44_100, reported), 100 + 529);
    }

    #[test]
    fn the_total_is_capped() {
        let huge = ReportedLatency { input: Duration::MAX, output: Duration::MAX };
        let o = RecordOffset { manual: Manual::Samples(MAX_MANUAL_SAMPLES), auto: true };
        assert_eq!(o.total(192_000, huge), MAX_TOTAL_SAMPLES);
        assert_eq!(o.total(u32::MAX, huge), MAX_TOTAL_SAMPLES);
    }

    #[test]
    fn apply_validates_and_keeps_the_old_setting_on_error() {
        let mut o = RecordOffset::default();
        o.apply("t", &json!({"ms": 12.5, "auto": true})).unwrap();
        assert_eq!(o, RecordOffset { manual: Manual::Ms(12.5), auto: true });
        o.apply("t", &json!({"samples": 300})).unwrap();
        assert_eq!(o, RecordOffset { manual: Manual::Samples(300), auto: true });
        for bad in [
            json!({"ms": -1}),
            json!({"ms": 5001}),
            json!({"samples": -1}),
            json!({"samples": 1.5}),
            json!({"samples": 1_000_001}),
            json!({"ms": 1, "samples": 1}),
            json!({"ms": "x"}),
        ] {
            assert!(o.apply("t", &bad).is_err(), "{bad}");
        }
        assert!(o.apply("t", &json!({"samples": 5, "auto": 1})).is_err());
        assert_eq!(o, RecordOffset { manual: Manual::Samples(300), auto: true }, "unchanged by errors");
        o.apply("t", &json!({"reset": true})).unwrap();
        assert_eq!(o, RecordOffset::default());
    }

    #[test]
    fn the_setting_survives_a_round_trip_and_old_preferences() {
        let o = RecordOffset { manual: Manual::Ms(3.5), auto: true };
        let back: RecordOffset = serde_json::from_str(&serde_json::to_string(&o).unwrap()).unwrap();
        assert_eq!(o, back);
        assert_eq!(serde_json::from_str::<RecordOffset>("{}").unwrap(), RecordOffset::default());
    }
}
