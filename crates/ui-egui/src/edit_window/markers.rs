//! Markers in the track lanes: they are picked, renamed and deleted where they are drawn, not only
//! in the marker ruler.

use super::input::snap;
use super::layout::{sample_at, x_of};
use crate::{MarkerDrag, SoundApp};
use egui::{Key, Rect, Response, Ui};
use serde_json::json;
use soundcraft_model::{MarkerId, MemoryLocation, Session};
use soundcraft_time::Samples;

/// How far (in points) from a marker's line a click still counts as on it.
pub(super) const REACH: f32 = 4.0;

/// The marker whose line is closest to `x` and within [`REACH`] of it. Lines of markers outside
/// the timeline `tl` are not drawn, so they are not hit either.
pub(super) fn marker_near(s: &Session, tl: Rect, x: f32) -> Option<&MemoryLocation> {
    s.markers
        .iter()
        .map(|m| (m, (x_of(s, tl, m.start) - x).abs()))
        .filter(|(m, d)| *d <= REACH && (tl.min.x..=tl.max.x).contains(&x_of(s, tl, m.start)))
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(m, _)| m)
}

/// Where to draw `m`: at its place, or where the mouse is dragging it.
pub(super) fn shown_start(drag: Option<MarkerDrag>, m: &MemoryLocation) -> Samples {
    match drag {
        Some(d) if d.id == m.id => d.at,
        _ => m.start,
    }
}

/// The position a marker grabbed with `grab` lands on when the pointer is at `x`: snapped to the
/// grid in the grid modes, and never before the start of the timeline.
pub(super) fn drop_position(s: &Session, tl: Rect, x: f32, grab: Samples) -> Samples {
    snap(s, sample_at(s, tl, x).saturating_add(grab)).max(0)
}

/// One frame of dragging a marker, for the response `resp` of the ruler row or lane the pointer is
/// in. `grab` is the marker under the press when `resp` starts a drag (the caller works that out,
/// as the ruler and the lanes find markers differently). The marker follows the pointer on screen;
/// the session changes only when the mouse is let go (`markers.edit`, one undo step), and Escape
/// drops the drag. Returns true when this response is the one driving a marker drag this frame,
/// so the caller leaves the pointer alone.
pub(super) fn drive_drag(app: &mut SoundApp, ui: &Ui, resp: &Response, tl: Rect, grab: Option<MarkerId>) -> bool {
    if resp.drag_started()
        && let Some(id) = grab
        && let Some(m) = app.engine.session().markers.iter().find(|m| m.id == id)
    {
        let origin = ui.input(|i| i.pointer.press_origin()).or_else(|| resp.interact_pointer_pos());
        let under = origin.map_or(m.start, |p| sample_at(app.engine.session(), tl, p.x));
        app.marker_drag = Some(MarkerDrag { id, from: m.start, at: m.start, grab: m.start.saturating_sub(under) });
        app.selected_marker = Some(id);
    }
    let Some(d) = app.marker_drag else { return false };
    let mine = resp.drag_started() || resp.dragged() || resp.drag_stopped();
    if !mine {
        return false;
    }
    if ui.input(|i| i.key_pressed(Key::Escape)) {
        app.marker_drag = None;
        return true;
    }
    ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
    let at = match resp.interact_pointer_pos() {
        Some(p) => drop_position(app.engine.session(), tl, p.x, d.grab),
        None => d.at,
    };
    if resp.drag_stopped() {
        app.marker_drag = None;
        if at != d.from {
            let _ = app.run("markers.edit", json!({"id": d.id.0, "at": at}));
        }
    } else {
        app.marker_drag = Some(MarkerDrag { at, ..d });
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{pos2, vec2};
    use serde_json::json;

    fn session() -> (soundcraft_engine::Engine, Rect) {
        let mut e = soundcraft_engine::Engine::default();
        e.execute("markers.add", &json!({"name": "A", "at": 48_000})).unwrap();
        e.execute("markers.add", &json!({"name": "B", "at": 96_000})).unwrap();
        (e, Rect::from_min_size(pos2(100.0, 0.0), vec2(800.0, 400.0)))
    }

    #[test]
    fn a_click_on_the_line_hits_it_and_one_far_away_does_not() {
        let (e, tl) = session();
        let s = e.session();
        let x = x_of(s, tl, 96_000);
        assert_eq!(marker_near(s, tl, x).map(|m| m.name.as_str()), Some("B"));
        assert_eq!(marker_near(s, tl, x + REACH - 0.5).map(|m| m.name.as_str()), Some("B"));
        assert!(marker_near(s, tl, x + REACH + 1.0).is_none());
        assert!(marker_near(s, tl, tl.min.x + 1.0).is_none(), "empty space between the lines");
    }

    #[test]
    fn of_two_close_lines_the_nearer_one_wins() {
        let mut e = soundcraft_engine::Engine::default();
        e.execute("markers.add", &json!({"name": "A", "at": 48_000})).unwrap();
        let tl = Rect::from_min_size(pos2(0.0, 0.0), vec2(800.0, 400.0));
        let xa = x_of(e.session(), tl, 48_000);
        let gap = e.session().edit.zoom.samples_per_px * 3.0;
        e.execute("markers.add", &json!({"name": "B", "at": 48_000 + gap as i64})).unwrap();
        let s = e.session();
        assert_eq!(marker_near(s, tl, xa + 0.5).map(|m| m.name.as_str()), Some("A"));
        assert_eq!(marker_near(s, tl, xa + 2.8).map(|m| m.name.as_str()), Some("B"));
    }

    #[test]
    fn the_drop_position_keeps_the_grab_offset_and_never_goes_before_zero() {
        let (e, tl) = session();
        let s = e.session();
        let x = x_of(s, tl, 96_000);
        assert_eq!(drop_position(s, tl, x, 0), 96_000);
        assert_eq!(drop_position(s, tl, x, 500), 96_500, "grabbed a little to the left of the marker's start");
        assert_eq!(drop_position(s, tl, tl.min.x - 5_000.0, 0), 0, "far left of the timeline");
        assert_eq!(drop_position(s, tl, tl.min.x, -100_000_000), 0, "a huge negative grab");
        let far = drop_position(s, tl, x, i64::MAX);
        assert!(far > 0, "a huge positive grab saturates instead of overflowing");
    }

    #[test]
    fn a_dragged_marker_is_drawn_where_it_would_land() {
        let (e, _) = session();
        let m = &e.session().markers[0];
        let drag = MarkerDrag { id: m.id, from: m.start, at: m.start + 777, grab: 0 };
        assert_eq!(shown_start(Some(drag), m), m.start + 777);
        let other = &e.session().markers[1];
        assert_eq!(shown_start(Some(drag), other), other.start, "only the dragged one moves");
        assert_eq!(shown_start(None, m), m.start);
    }

    #[test]
    fn a_marker_outside_the_visible_timeline_is_not_hit() {
        let (e, tl) = session();
        let s = e.session();
        let x = x_of(s, tl, 96_000);
        let narrow = Rect::from_min_max(pos2(x + 20.0, 0.0), pos2(x + 200.0, 400.0));
        assert!(marker_near(s, narrow, x).is_none());
    }
}
