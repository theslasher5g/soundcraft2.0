//! Markers in the track lanes: they are picked, renamed and deleted where they are drawn, not only
//! in the marker ruler.

use super::layout::x_of;
use egui::Rect;
use soundcraft_model::{MemoryLocation, Session};

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
    fn a_marker_outside_the_visible_timeline_is_not_hit() {
        let (e, tl) = session();
        let s = e.session();
        let x = x_of(s, tl, 96_000);
        let narrow = Rect::from_min_max(pos2(x + 20.0, 0.0), pos2(x + 200.0, 400.0));
        assert!(marker_near(s, narrow, x).is_none());
    }
}
