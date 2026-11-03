//! Markers are picked in the marker ruler or on their line in a track lane: Delete removes the
//! picked one, a double click on the line renames it, and a click elsewhere drops the pick.

use egui::{Event, Key, Modifiers, PointerButton, Pos2, RawInput, Rect, Shape, epaint::ClippedShape, pos2, vec2};
use serde_json::json;
use soundcraft_ui_egui::{Services, SoundApp};

struct Rig {
    ctx: egui::Context,
    app: SoundApp,
    shapes: Vec<ClippedShape>,
}

impl Rig {
    fn new() -> Rig {
        let mut engine = soundcraft_engine::Engine::default();
        engine.execute("track.new", &json!({})).unwrap();
        engine.execute("markers.add", &json!({"name": "Verse", "at": 96_000})).unwrap();
        engine.execute("markers.add", &json!({"name": "Chorus", "at": 192_000})).unwrap();
        let mut r = Rig { ctx: egui::Context::default(), app: SoundApp::new(engine, None, Services::default()), shapes: Vec::new() };
        r.frames(4);
        r
    }

    fn frame(&mut self, events: Vec<Event>) {
        let raw = RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1600.0, 1000.0))), events, ..Default::default() };
        let mut out = self.ctx.run_ui(raw, |ui| {
            self.app.logic(ui.ctx());
            self.app.ui(ui);
        });
        out.textures_delta.clear();
        self.shapes = out.shapes;
    }

    fn frames(&mut self, n: usize) {
        for _ in 0..n {
            self.frame(vec![]);
        }
    }

    /// The ruler flag of the marker named `name` (its label starts 8pt to the right).
    fn flag(&self, name: &str) -> Pos2 {
        self.shapes
            .iter()
            .find_map(|c| match &c.shape {
                Shape::Text(t) if t.galley.text() == name => Some(pos2(t.pos.x - 8.0, t.galley.rect.translate(t.pos.to_vec2()).center().y)),
                _ => None,
            })
            .expect("marker not on the ruler")
    }

    /// A point on the marker's line in the first track lane.
    fn on_line(&self, name: &str) -> Pos2 {
        let lane = self.app.edit_layout.rows.first().expect("a track lane").1;
        pos2(self.flag(name).x, (lane[1] + lane[3]) / 2.0)
    }

    fn click(&mut self, at: Pos2) {
        for pressed in [true, false] {
            self.frame(vec![
                Event::PointerMoved(at),
                Event::PointerButton { pos: at, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE },
            ]);
        }
        self.frames(2);
    }

    fn double_click(&mut self, at: Pos2) {
        for pressed in [true, false, true, false] {
            self.frame(vec![
                Event::PointerMoved(at),
                Event::PointerButton { pos: at, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE },
            ]);
        }
        self.frames(2);
    }

    /// Press at `from`, move to `to` in a few steps (the drag must pass egui's click distance), and
    /// let go there. `escape_at` presses Escape once the pointer is at `to`, before the release.
    fn drag(&mut self, from: Pos2, to: Pos2, escape_at: bool) {
        let button = |pos: Pos2, pressed: bool| Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE };
        self.frame(vec![Event::PointerMoved(from)]);
        self.frame(vec![Event::PointerMoved(from), button(from, true)]);
        for i in 1..=6 {
            let p = from + (to - from) * (i as f32 / 6.0);
            self.frame(vec![Event::PointerMoved(p)]);
        }
        if escape_at {
            self.frame(vec![Event::Key { key: Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE }]);
        }
        self.frame(vec![Event::PointerMoved(to), button(to, false)]);
        self.frames(2);
    }

    fn start_of(&self, name: &str) -> i64 {
        self.app.engine.session().markers.iter().find(|m| m.name == name).map(|m| m.start).expect("marker")
    }

    fn samples_per_px(&self) -> f64 {
        self.app.engine.session().edit.zoom.samples_per_px
    }

    fn key(&mut self, key: Key) {
        for pressed in [true, false] {
            self.frame(vec![Event::Key { key, physical_key: None, pressed, repeat: false, modifiers: Modifiers::NONE }]);
        }
        self.frames(2);
    }

    fn names(&self) -> Vec<String> {
        self.app.engine.session().markers.iter().map(|m| m.name.clone()).collect()
    }

    fn picked(&self) -> Option<String> {
        let id = self.app.selected_marker?;
        self.app.engine.session().markers.iter().find(|m| m.id == id).map(|m| m.name.clone())
    }
}

#[test]
fn clicking_a_ruler_marker_picks_it_and_delete_removes_it() {
    let mut r = Rig::new();
    r.click(r.flag("Verse"));
    assert_eq!(r.picked().as_deref(), Some("Verse"));
    r.key(Key::Delete);
    assert_eq!(r.names(), vec!["Chorus".to_string()]);
    assert_eq!(r.app.selected_marker, None, "the pick goes with the marker");
}

#[test]
fn backspace_deletes_the_picked_marker_too() {
    let mut r = Rig::new();
    r.click(r.flag("Chorus"));
    r.key(Key::Backspace);
    assert_eq!(r.names(), vec!["Verse".to_string()]);
}

#[test]
fn delete_with_nothing_picked_leaves_every_marker() {
    let mut r = Rig::new();
    r.key(Key::Delete);
    assert_eq!(r.names().len(), 2);
}

#[test]
fn clicking_the_line_in_a_lane_picks_the_marker() {
    let mut r = Rig::new();
    r.click(r.on_line("Chorus"));
    assert_eq!(r.picked().as_deref(), Some("Chorus"));
    r.key(Key::Delete);
    assert_eq!(r.names(), vec!["Verse".to_string()]);
}

#[test]
fn double_clicking_the_line_in_a_lane_renames_the_marker() {
    let mut r = Rig::new();
    r.double_click(r.on_line("Verse"));
    assert_eq!(r.app.dialogs.open_name(), Some("rename_marker"));
    r.frame(vec![Event::Text("Intro".into())]);
    r.frame(vec![Event::Key { key: Key::Enter, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE }]);
    r.frames(2);
    assert_eq!(r.names(), vec!["Intro".to_string(), "Chorus".to_string()]);
}

#[test]
fn a_click_elsewhere_in_the_lanes_drops_the_pick_so_delete_cannot_reach_it() {
    let mut r = Rig::new();
    r.click(r.on_line("Verse"));
    assert!(r.picked().is_some());
    let away = r.on_line("Verse") + vec2(150.0, 0.0);
    r.click(away);
    assert_eq!(r.picked(), None);
    r.key(Key::Delete);
    assert_eq!(r.names().len(), 2, "no marker was deleted");
}

#[test]
fn clicking_empty_ruler_drops_the_pick() {
    let mut r = Rig::new();
    r.click(r.flag("Verse"));
    r.click(r.flag("Verse") + vec2(300.0, 0.0));
    assert_eq!(r.picked(), None);
}

#[test]
fn a_deleted_marker_leaves_no_stale_pick_after_undo() {
    let mut r = Rig::new();
    r.click(r.flag("Verse"));
    r.key(Key::Delete);
    r.app.run("edit.undo", json!({})).unwrap();
    r.frames(2);
    assert_eq!(r.names().len(), 2, "undo brings it back");
    r.key(Key::Delete);
    assert_eq!(r.names().len(), 2, "but nothing is picked any more");
}

// ---- dragging -------------------------------------------------------------------------------

/// The marker moved by about `px` pixels' worth of samples (the pointer is on pixels, the marker
/// on samples: allow a pixel either way).
fn moved_by(r: &Rig, before: i64, name: &str, px: f64) -> bool {
    let spp = r.samples_per_px();
    ((r.start_of(name) - before) as f64 - px * spp).abs() <= spp * 1.5
}

#[test]
fn dragging_a_ruler_flag_moves_the_marker() {
    let mut r = Rig::new();
    let (before, chorus) = (r.start_of("Verse"), r.start_of("Chorus"));
    let from = r.flag("Verse");
    r.drag(from, from + vec2(100.0, 0.0), false);
    assert!(moved_by(&r, before, "Verse", 100.0), "Verse is at {}", r.start_of("Verse"));
    assert_eq!(r.start_of("Chorus"), chorus, "the other marker stays");
    assert_eq!(r.picked().as_deref(), Some("Verse"), "the dragged marker is picked");
}

#[test]
fn dragging_the_line_in_a_lane_moves_the_marker() {
    let mut r = Rig::new();
    let before = r.start_of("Chorus");
    let from = r.on_line("Chorus");
    r.drag(from, from - vec2(60.0, 0.0), false);
    assert!(moved_by(&r, before, "Chorus", -60.0), "Chorus is at {}", r.start_of("Chorus"));
}

#[test]
fn the_whole_drag_is_one_undo_step() {
    let mut r = Rig::new();
    let before = r.start_of("Verse");
    let from = r.flag("Verse");
    r.drag(from, from + vec2(80.0, 0.0), false);
    assert_ne!(r.start_of("Verse"), before);
    r.app.run("edit.undo", json!({})).unwrap();
    assert_eq!(r.start_of("Verse"), before, "one undo puts it back");
}

#[test]
fn a_drag_does_not_move_the_playhead_but_a_click_does() {
    let mut r = Rig::new();
    let from = r.flag("Verse");
    let playhead = r.app.engine.transport.position;
    r.drag(from, from + vec2(50.0, 0.0), false);
    assert_eq!(r.app.engine.transport.position, playhead, "dragging does not recall the marker");
    r.click(r.flag("Verse"));
    assert_eq!(r.app.engine.transport.position, r.start_of("Verse"), "a plain click still does");
}

#[test]
fn dragging_left_past_the_start_stops_at_zero() {
    let mut r = Rig::new();
    let from = r.flag("Verse");
    r.drag(from, pos2(from.x - 5_000.0, from.y), false);
    assert_eq!(r.start_of("Verse"), 0);
}

#[test]
fn escape_drops_the_drag() {
    let mut r = Rig::new();
    let before = r.start_of("Verse");
    let undo = r.app.engine.undo_label().map(str::to_string);
    let from = r.flag("Verse");
    r.drag(from, from + vec2(100.0, 0.0), true);
    assert_eq!(r.start_of("Verse"), before, "nothing changed");
    assert_eq!(r.app.engine.undo_label().map(str::to_string), undo, "and no undo step was made");
}

#[test]
fn dragging_past_another_marker_keeps_them_in_order() {
    let mut r = Rig::new();
    let from = r.flag("Verse");
    let to = r.flag("Chorus") + vec2(60.0, 0.0);
    r.drag(from, pos2(to.x, from.y), false);
    assert_eq!(r.names(), vec!["Chorus".to_string(), "Verse".to_string()]);
}

#[test]
fn dragging_the_empty_ruler_moves_nothing() {
    let mut r = Rig::new();
    let (verse, chorus) = (r.start_of("Verse"), r.start_of("Chorus"));
    let from = r.flag("Verse") + vec2(300.0, 0.0);
    r.drag(from, from + vec2(40.0, 0.0), false);
    assert_eq!((r.start_of("Verse"), r.start_of("Chorus")), (verse, chorus));
    assert_eq!(r.names().len(), 2);
}

#[test]
fn in_a_grid_mode_the_marker_lands_on_the_grid() {
    let mut r = Rig::new();
    r.app.run("edit.mode", json!({"mode": "grid"})).unwrap();
    r.frames(2);
    let from = r.flag("Verse");
    r.drag(from, from + vec2(37.0, 0.0), false);
    let s = r.app.engine.session();
    let at = r.start_of("Verse");
    assert_eq!(s.edit.grid.snap(at, s.sample_rate, &s.tempo, s.frame_rate), at, "{at} is on the grid");
    assert_ne!(at, 96_000);
}
