//! Comp: build one take out of pieces of others. Select a range on a take's track and comp it
//! onto the track above (or below): that stretch of the take replaces what the target holds
//! there, with short crossfades at the two seams. The take itself stays untouched.
use super::{CommandSpec, bad, f64_or, has_range, range_param, track_param, tracks_required};
use crate::edit as ops;
use crate::{Engine, Result, cmd};
use serde_json::{Value, json};
use soundcraft_model::TrackId;

/// The crossfade at each seam when none is given, in milliseconds.
const DEFAULT_XFADE_MS: f64 = 8.0;
/// The longest crossfade a command may ask for, in milliseconds.
const MAX_XFADE_MS: f64 = 500.0;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            "edit.comp",
            "Comp to Track Above",
            ["Edit"],
            None,
            "{source?: track (default: the first selected track), target?: 'above'|'below'|track id or name (default 'above'), start?, end? (default: the edit selection), crossfade_ms?: 0..=500 (default 8)} — copies the source's material in the range onto the target, in place, replacing what is there, with equal-power crossfades at both seams. The source keeps its material, so a take can be comped from again. Audio tracks of the same channel format only",
            has_range,
            |e, p| comp(e, p, "edit.comp", true)
        ),
        cmd!(
            "edit.comp_below",
            "Comp to Track Below",
            ["Edit"],
            None,
            "{source?, target? (default 'below'), start?, end?, crossfade_ms?} — as edit.comp, with the target defaulting to the track below",
            has_range,
            |e, p| comp(e, p, "edit.comp_below", false)
        ),
    ]
}

fn comp(e: &mut Engine, p: &Value, id: &str, above: bool) -> Result<Value> {
    let source = match track_param(e, id, p, "source")? {
        Some(t) => t,
        None => *tracks_required(e, id, p)?.first().ok_or_else(|| bad(id, "no source track"))?,
    };
    let target = target_param(e, id, p, source, above)?;
    let range = range_param(e, id, p)?;
    let ms = f64_or(p, "crossfade_ms", DEFAULT_XFADE_MS);
    if !(0.0..=MAX_XFADE_MS).contains(&ms) {
        return Err(bad(id, "`crossfade_ms` must be a number from 0 to 500"));
    }
    let s = e.session_mut();
    let xfade = s.sample_rate.samples(ms / 1000.0);
    let placed = ops::comp_range(s, source, target, range, xfade).map_err(|m| bad(id, m))?;
    Ok(json!({"source": source, "target": target, "pieces": placed, "start": range.start, "end": range.end}))
}

/// `target`: 'above' / 'below' (the shown neighbour of `source`), or a track by id or name.
fn target_param(e: &Engine, id: &str, p: &Value, source: TrackId, default_above: bool) -> Result<TrackId> {
    let neighbor = |above: bool| {
        ops::neighbor_track(e.session(), source, above)
            .ok_or_else(|| bad(id, if above { "there is no track above" } else { "there is no track below" }))
    };
    match p.get("target") {
        None | Some(Value::Null) => neighbor(default_above),
        Some(Value::String(s)) if s.eq_ignore_ascii_case("above") => neighbor(true),
        Some(Value::String(s)) if s.eq_ignore_ascii_case("below") => neighbor(false),
        Some(_) => track_param(e, id, p, "target")?.ok_or_else(|| bad(id, "`target` must be 'above', 'below', or a track")),
    }
}

#[cfg(test)]
mod tests {
    use crate::Engine;
    use serde_json::json;
    use soundcraft_model::TrackId;

    /// Two mono audio tracks, "Main" above "Take", each with a clip of a different source covering
    /// 0..48000 (a 1 s ramp for the main, a constant for the take). No audio device involved.
    fn rig() -> (Engine, TrackId, TrackId) {
        let mut e = Engine::default();
        let main = e.execute("track.new", &json!({"name": "Main", "format": "mono"})).unwrap()["tracks"][0].as_u64().unwrap();
        let take = e.execute("track.new", &json!({"name": "Take", "format": "mono"})).unwrap()["tracks"][0].as_u64().unwrap();
        let rate = e.session().sample_rate.hz();
        for (t, level) in [(main, 0.1_f32), (take, 0.9_f32)] {
            let src = crate::io::add_source(
                e.session_mut(),
                &format!("src{t}"),
                soundcraft_audio_io::AudioBuffer { sample_rate: rate, channels: vec![vec![level; 96_000]] },
                None,
                soundcraft_audio_io::FileFormat::Wav,
            );
            let s = e.session_mut();
            let cid = s.new_clip_id();
            crate::edit::place_clip(s, TrackId(t), soundcraft_model::Clip::audio(cid, format!("c{t}"), src, 0, 0, 96_000));
        }
        (e, TrackId(main), TrackId(take))
    }

    fn layout(e: &Engine, t: TrackId) -> Vec<(i64, i64, i64)> {
        e.session().track(t).unwrap().clips().iter().map(|c| (c.start, c.length, c.source_offset())).collect()
    }

    #[test]
    fn a_range_of_the_take_replaces_the_same_range_above() {
        let (mut e, main, take) = rig();
        let r = e.execute("edit.comp", &json!({"source": take.0, "start": 24_000, "end": 48_000, "crossfade_ms": 0})).unwrap();
        assert_eq!((r["target"].as_u64(), r["pieces"].as_u64()), (Some(main.0), Some(1)));
        // The main track: before, the comped piece, after.
        let l = layout(&e, main);
        assert_eq!(l.len(), 3, "{l:?}");
        assert_eq!((l[0].0, l[0].1), (0, 24_000));
        assert_eq!((l[1].0, l[1].1, l[1].2), (24_000, 24_000, 24_000), "the take's material at its own position");
        assert_eq!((l[2].0, l[2].1), (48_000, 48_000));
        // The piece plays the take's audio, the rest still the main track's own.
        let src = |t: TrackId| e.session().track(t).unwrap().clips().iter().map(|c| c.source()).collect::<Vec<_>>();
        let (m, k) = (src(main), src(take));
        assert_eq!(m[1], k[0], "the comped piece uses the take's source");
        assert_ne!(m[0], k[0]);
        assert_eq!(m[0], m[2]);
    }

    #[test]
    fn the_take_keeps_all_its_material() {
        let (mut e, _, take) = rig();
        let before = layout(&e, take);
        e.execute("edit.comp", &json!({"source": take.0, "start": 10_000, "end": 20_000})).unwrap();
        assert_eq!(layout(&e, take), before);
    }

    #[test]
    fn the_seams_get_a_crossfade_and_zero_means_none() {
        let (mut e, main, take) = rig();
        e.execute("edit.comp", &json!({"source": take.0, "start": 24_000, "end": 48_000})).unwrap();
        let fades: Vec<(i64, i64)> = e.session().track(main).unwrap().clips().iter().map(|c| (c.fade_in.len, c.fade_out.len)).collect();
        assert!(fades.iter().any(|f| f.0 > 0) && fades.iter().any(|f| f.1 > 0), "fades at the seams: {fades:?}");
        let (mut e, main, take) = rig();
        e.execute("edit.comp", &json!({"source": take.0, "start": 24_000, "end": 48_000, "crossfade_ms": 0})).unwrap();
        assert!(e.session().track(main).unwrap().clips().iter().all(|c| c.fade_in.len == 0 && c.fade_out.len == 0));
    }

    #[test]
    fn it_is_one_undo_step() {
        let (mut e, main, take) = rig();
        let before = layout(&e, main);
        e.execute("edit.comp", &json!({"source": take.0, "start": 24_000, "end": 48_000})).unwrap();
        assert_ne!(layout(&e, main), before);
        e.execute("edit.undo", &json!({})).unwrap();
        assert_eq!(layout(&e, main), before);
    }

    #[test]
    fn the_target_defaults_to_the_track_above_and_below_goes_down() {
        let (mut e, main, take) = rig();
        let r = e.execute("edit.comp_below", &json!({"source": main.0, "start": 0, "end": 1_000, "crossfade_ms": 0})).unwrap();
        assert_eq!(r["target"].as_u64(), Some(take.0));
        let r = e.execute("edit.comp", &json!({"source": take.0, "start": 0, "end": 1_000, "crossfade_ms": 0})).unwrap();
        assert_eq!(r["target"].as_u64(), Some(main.0));
        let r = e.execute("edit.comp", &json!({"source": main.0, "target": "Take", "start": 0, "end": 1_000, "crossfade_ms": 0})).unwrap();
        assert_eq!(r["target"].as_u64(), Some(take.0), "a track by name");
    }

    #[test]
    fn bad_requests_change_nothing() {
        let (mut e, main, take) = rig();
        let before = (layout(&e, main), layout(&e, take));
        for bad in [
            json!({"source": main.0, "start": 0, "end": 1_000}), // nothing above the top track
            json!({"source": take.0, "target": take.0, "start": 0, "end": 1_000}), // same track
            json!({"source": take.0, "start": 5, "end": 5}),     // empty range
            json!({"source": take.0, "start": 0, "end": 1_000, "crossfade_ms": -1}), // bad fade
            json!({"source": take.0, "start": 0, "end": 1_000, "crossfade_ms": 9_999}), // bad fade
            json!({"source": take.0, "target": "Nope", "start": 0, "end": 1_000}), // no such track
            json!({"source": take.0, "start": 500_000, "end": 600_000}), // nothing there
        ] {
            assert!(e.execute("edit.comp", &bad).is_err(), "{bad}");
        }
        assert_eq!((layout(&e, main), layout(&e, take)), before);
    }

    #[test]
    fn tracks_of_different_formats_are_refused() {
        let (mut e, main, _) = rig();
        let st = e.execute("track.new", &json!({"name": "Stereo", "format": "stereo"})).unwrap()["tracks"][0].as_u64().unwrap();
        let err = e.execute("edit.comp", &json!({"source": main.0, "target": st, "start": 0, "end": 1_000})).unwrap_err();
        assert!(err.to_string().contains("channel formats"), "{err}");
    }

    #[test]
    fn huge_positions_do_not_overflow() {
        let (mut e, _, take) = rig();
        let _ = e.execute("edit.comp", &json!({"source": take.0, "start": i64::MAX - 5, "end": i64::MAX, "crossfade_ms": 500}));
        let _ = e.execute("edit.comp", &json!({"source": take.0, "start": -1_000_000, "end": 1_000, "crossfade_ms": 500}));
        assert!(e.session().track(take).is_some());
    }

    #[test]
    fn the_commands_are_in_the_edit_menu() {
        for id in ["edit.comp", "edit.comp_below"] {
            let c = crate::cmd::find_command(id).unwrap();
            assert_eq!(c.menu, &["Edit"][..]);
        }
    }
}
