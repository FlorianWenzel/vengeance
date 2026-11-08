//! Walk mode (docs/topology.md section 4): first-person movement, crosshair picking,
//! nearby labels and the minimap.

use glam::{Vec2, Vec3, Vec4Swizzles};
use serde::Serialize;

use crate::engine::{Engine, Label, Mode};
use crate::layout::{self, Kind, Layout};
use crate::room;
use crate::walk::{self, Walker, EYE, REACH};

const LABEL_RANGE: f32 = 8.0;
const CLOSE_RANGE: f32 = 2.0;
const GLIDE_MS: f64 = 900.0;

#[derive(Serialize)]
struct Player {
    x: f32,
    z: f32,
    heading: f32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Bounds {
    min_x: f32,
    min_z: f32,
    max_x: f32,
    max_z: f32,
}

#[derive(Serialize)]
struct Item<'a> {
    id: &'a str,
    kind: &'a str,
    x: f32,
    z: f32,
    w: f32,
    d: f32,
    level: &'static str,
}

#[derive(Serialize)]
struct Minimap<'a> {
    mode: &'static str,
    player: Player,
    bounds: Bounds,
    items: Vec<Item<'a>>,
}

fn round2(v: f32) -> f32 {
    (v * 100.0).round() / 100.0
}

impl Engine {
    // ---- mode switching ----------------------------------------------------------------

    pub fn mode_name(&self) -> &'static str {
        match self.mode {
            Mode::Orbit => "orbit",
            Mode::Walk => "walk",
        }
    }

    pub fn set_mode(&mut self, mode: &str) {
        let m = match mode {
            "walk" => Mode::Walk,
            "orbit" => Mode::Orbit,
            _ => return,
        };
        if m == self.mode {
            return;
        }
        self.mode = m;
        self.walker.keys = 0;
        self.hovered = None;
        self.pointer = None;
        self.look_drag = None;
        self.rebuild();
        match m {
            Mode::Walk => {
                self.walk_respawn(false);
            }
            Mode::Orbit => {
                // The orbit pose was never touched while walking: drop the override and it is back.
                self.cam.fps = None;
                if !self.orbit_framed && !self.layout.nodes.is_empty() {
                    self.cam.stop();
                    self.cam.pose = self.overview_pose();
                    self.orbit_framed = true;
                }
            }
        }
        self.dirty = true;
    }

    /// Rebuild the layout for the current mode from the stored scene, keeping ids.
    pub(crate) fn rebuild(&mut self) {
        let id_of = |l: &Layout, i: Option<usize>| i.and_then(|i| l.nodes.get(i)).map(|n| n.id.clone());
        let (f, s, h) = (id_of(&self.layout, self.focus), id_of(&self.layout, self.selected), id_of(&self.layout, self.hovered));
        self.layout = match self.mode {
            Mode::Orbit => layout::build(&self.scene),
            Mode::Walk => room::build_walk(&self.scene),
        };
        self.geo_version += 1;
        let find = |l: &Layout, id: Option<String>| id.and_then(|id| l.index.get(&id).copied());
        self.focus = find(&self.layout, f);
        self.selected = find(&self.layout, s);
        self.hovered = find(&self.layout, h);
        self.highlight_key = (None, Some(usize::MAX));
        if self.mode == Mode::Orbit {
            let r = self.layout.bounds.map(|b| b.radius()).unwrap_or(2.0);
            self.cam.max_dist = r * 5.0 + 8.0;
        }
        self.dirty = true;
    }

    /// After a scene refresh while walking: keep the player unless they now stand inside something.
    pub(crate) fn after_walk_rebuild(&mut self) {
        let Some(room) = self.layout.room.as_ref() else { return };
        if !self.has_scene || walk::blocked(self.walker.pos, &room.colliders) {
            self.walk_respawn(false);
        }
        self.sync_fps();
    }

    pub(crate) fn walk_respawn(&mut self, glide: bool) {
        let Some(room) = self.layout.room.as_ref() else { return };
        let (spawn, heading) = (room.spawn, room.spawn_heading);
        if glide {
            let ms = if self.reduced_motion { 0.0 } else { GLIDE_MS };
            self.walker.glide_to(Vec2::new(spawn.x, spawn.z), heading, -0.06, self.now, ms);
        } else {
            let keys = self.walker.keys;
            self.walker = Walker::spawn(spawn, heading);
            self.walker.keys = keys;
        }
        self.sync_fps();
        self.dirty = true;
    }

    fn sync_fps(&mut self) {
        self.cam.fps = Some((self.walker.eye(self.reduced_motion), self.walker.forward()));
    }

    // ---- input -------------------------------------------------------------------------

    pub fn key_down(&mut self, code: &str) {
        if self.mode != Mode::Walk {
            return;
        }
        if code == "KeyE" {
            return self.interact();
        }
        self.walker.keys |= walk::key_bit(code);
    }

    pub fn key_up(&mut self, code: &str) {
        self.walker.keys &= !walk::key_bit(code);
    }

    pub fn look(&mut self, dx: f32, dy: f32) {
        if self.mode != Mode::Walk {
            return;
        }
        self.walker.look(dx, dy);
        self.sync_fps();
        self.dirty = true;
        self.walk_update_hover();
    }

    /// Select what the crosshair aims at (or clear the selection when it aims at nothing).
    pub fn interact(&mut self) {
        if self.mode != Mode::Walk {
            return;
        }
        let w = self.css_w * 0.5;
        let h = self.css_h * 0.5;
        self.selected = self.pick_walk(w, h);
        self.hovered = self.selected.or(self.hovered);
        self.dirty = true;
    }

    // Without pointer lock: left-drag looks around, a click selects what is under the pointer.
    pub(crate) fn walk_pointer_down(&mut self, x: f32, y: f32, button: i32) {
        if button == 0 {
            self.look_drag = Some((Vec2::new(x, y), Vec2::new(x, y), false));
        }
    }

    pub(crate) fn walk_pointer_move(&mut self, x: f32, y: f32) {
        let p = Vec2::new(x, y);
        if let Some((start, last, mut moved)) = self.look_drag {
            if !moved && p.distance(start) > 4.0 {
                moved = true;
            }
            if moved {
                let d = p - last;
                self.walker.look(-d.x, -d.y);
                self.sync_fps();
                self.dirty = true;
                self.walk_update_hover();
            }
            self.look_drag = Some((start, p, moved));
        }
    }

    pub(crate) fn walk_pointer_up(&mut self, x: f32, y: f32, button: i32) {
        let Some((_, _, moved)) = self.look_drag.take() else { return };
        if button == 0 && !moved {
            self.selected = self.pick_walk(x, y);
            self.dirty = true;
        }
    }

    /// Walk up to an element and face it.
    pub(crate) fn walk_to(&mut self, idx: usize) {
        self.selected = Some(idx);
        self.dirty = true;
        let Some(room) = self.layout.room.as_ref() else { return };
        let n = &self.layout.nodes[idx];
        let f = Vec2::new(n.facing.x, n.facing.z).normalize_or(Vec2::Y);
        let b = n.body;
        let c = b.center();
        let half = (b.size().x * f.x.abs() + b.size().z * f.y.abs()) * 0.5;
        let front = Vec2::new(c.x, c.z) + f * half;
        let mut dist: f32 = match n.kind {
            Kind::Host => 2.2,
            Kind::Group => 1.4,
            Kind::Workload => 0.85,
            Kind::Image => 0.6,
            Kind::External => 1.2,
        };
        // Low elements: stand back far enough that looking at them is not a steep look down.
        let drop = (EYE - c.y).max(0.0);
        if n.kind != Kind::Host {
            dist = dist.max(drop / 0.65_f32.tan());
        }
        let mut stand = front + f * dist;
        while walk::blocked(stand, &room.colliders) && dist > 0.3 {
            dist -= 0.1;
            stand = front + f * dist;
        }
        if walk::blocked(stand, &room.colliders) {
            stand = self.walker.pos;
        }
        let look_at = Vec3::new(front.x, if n.kind == Kind::Host { c.y.max(1.2) } else { c.y }, front.y);
        let d = Vec2::new(look_at.x - stand.x, look_at.z - stand.y);
        let heading = d.x.atan2(-d.y);
        let pitch = (look_at.y - EYE).atan2(d.length().max(0.1)).clamp(-0.7, 0.6);
        let ms = if self.reduced_motion { 0.0 } else { GLIDE_MS };
        self.walker.glide_to(stand, heading, pitch, self.now, ms);
        self.sync_fps();
    }

    pub(crate) fn walk_update(&mut self, now: f64, dt: f32) {
        let changed = match self.layout.room.as_ref() {
            Some(room) => self.walker.step(now, dt, &room.colliders),
            None => self.walker.step(now, dt, &[]),
        };
        if changed || self.cam.fps.is_none() {
            self.sync_fps();
            self.dirty = true;
            self.walk_update_hover();
        }
    }

    // ---- picking -----------------------------------------------------------------------

    /// Deepest visible element under (x, y) within reach.
    pub fn pick_walk(&self, x: f32, y: f32) -> Option<usize> {
        let (o, d) = self.cam.ray(x, y, self.css_w, self.css_h);
        let eye = self.cam.eye();
        let nodes = &self.layout.nodes;
        let nearest = |cands: &[usize]| -> Option<(f32, usize)> {
            let mut best: Option<(f32, usize)> = None;
            for &i in cands {
                if nodes[i].lod.vis(eye) < 0.5 {
                    continue;
                }
                if let Some(t) = nodes[i].pick.ray_hit(o, d) {
                    if t <= REACH && best.is_none_or(|(bt, _)| t < bt) {
                        best = Some((t, i));
                    }
                }
            }
            best
        };
        let (_, mut cur) = nearest(&self.layout.roots)?;
        while let Some((_, c)) = nearest(&nodes[cur].children) {
            cur = c;
        }
        Some(cur)
    }

    pub(crate) fn walk_update_hover(&mut self) {
        let h = self.pick_walk(self.css_w * 0.5, self.css_h * 0.5);
        if h != self.hovered {
            self.hovered = h;
            self.dirty = true;
        }
    }

    // ---- labels ------------------------------------------------------------------------

    fn root_of(&self, mut i: usize) -> usize {
        while let Some(p) = self.layout.nodes[i].parent {
            i = p;
        }
        i
    }

    fn hidden_by_other_rack(&self, eye: Vec3, p: Vec3, own: usize) -> bool {
        let d = p - eye;
        let len = d.length();
        if len < 1e-4 {
            return false;
        }
        let dir = d / len;
        self.layout.hosts.iter().any(|&h| h != own && self.layout.nodes[h].body.ray_hit(eye, dir).is_some_and(|t| t < len - 0.02))
    }

    /// The group an element belongs to (itself for groups).
    fn group_of(&self, i: usize) -> Option<usize> {
        let mut cur = Some(i);
        while let Some(c) = cur {
            if self.layout.nodes[c].kind == Kind::Group {
                return Some(c);
            }
            cur = self.layout.nodes[c].parent;
        }
        None
    }

    fn workload_of(&self, i: usize) -> Option<usize> {
        let mut cur = Some(i);
        while let Some(c) = cur {
            if self.layout.nodes[c].kind == Kind::Workload {
                return Some(c);
            }
            cur = self.layout.nodes[c].parent;
        }
        None
    }

    pub(crate) fn labels_walk(&self) -> Vec<Label<'_>> {
        // Close-up labels only for the unit / module you are looking at (or have selected).
        let aim = [self.hovered, self.selected];
        let groups: [Option<usize>; 2] = aim.map(|a| a.and_then(|a| self.group_of(a)));
        let workloads: [Option<usize>; 2] = aim.map(|a| a.and_then(|a| self.workload_of(a)));
        let vp = self.cam.view_proj();
        let eye = self.cam.eye();
        let px_per_m_at_w = self.css_h * 0.5 / (self.cam.fov() * 0.5).tan();
        let mut out = Vec::new();
        for (i, n) in self.layout.nodes.iter().enumerate() {
            let d = eye.distance(n.label_at);
            let special = Some(i) == self.selected || Some(i) == self.hovered;
            let range = match n.kind {
                Kind::Host => LABEL_RANGE + 1.5,
                Kind::Group | Kind::External => LABEL_RANGE,
                Kind::Workload | Kind::Image => CLOSE_RANGE,
            };
            if d > range && !(special && d < LABEL_RANGE) {
                continue;
            }
            // Labels on a face are only readable from the side that face looks at.
            if n.kind != Kind::Host && (eye - n.label_at).dot(n.facing) <= 0.0 {
                continue;
            }
            let clip = vp * n.label_at.extend(1.0);
            if clip.w <= 0.05 {
                continue;
            }
            let ndc = clip.xyz() / clip.w;
            if ndc.x.abs() > 1.05 || ndc.y.abs() > 1.05 || !(-1.0..=1.0).contains(&ndc.z) {
                continue;
            }
            let px = n.label_size * px_per_m_at_w / clip.w;
            let vis = n.lod.vis(eye);
            let show = special
                || match n.kind {
                    Kind::Host => true,
                    Kind::Group => vis > 0.5 && px >= 11.0,
                    Kind::Workload => vis > 0.6 && px >= 34.0 && groups.contains(&n.parent),
                    Kind::Image => vis > 0.6 && px >= 26.0 && workloads.contains(&n.parent),
                    Kind::External => px >= 34.0,
                };
            if !show {
                continue;
            }
            let own = self.root_of(i);
            if self.hidden_by_other_rack(eye, n.label_at, own) {
                continue;
            }
            out.push(Label {
                id: &n.id,
                text: &n.text,
                sub: &n.sub,
                x: round2(((ndc.x * 0.5 + 0.5) * self.css_w * 10.0).round() / 10.0),
                y: round2(((0.5 - ndc.y * 0.5) * self.css_h * 10.0).round() / 10.0),
                depth: ((ndc.z * 0.5 + 0.5) * 10000.0).round() / 10000.0,
                level: n.level.as_str(),
                kind: &n.kind_name,
                anchor: n.anchor.as_str(),
                stale: n.stale,
                selected: Some(i) == self.selected,
                hovered: Some(i) == self.hovered,
            });
        }
        out.sort_by(|a, b| b.depth.total_cmp(&a.depth));
        out
    }

    // ---- minimap -----------------------------------------------------------------------

    pub fn minimap_json(&self) -> String {
        let (player, bounds) = match self.mode {
            Mode::Walk => (Player { x: round2(self.walker.pos.x), z: round2(self.walker.pos.y), heading: round2(self.walker.heading) }, self.layout.bounds),
            Mode::Orbit => {
                let p = self.cam.pose;
                let h = -p.yaw;
                (Player { x: round2(p.target.x), z: round2(p.target.z), heading: round2(h) }, self.layout.bounds.map(|b| b.grow(0.9)))
            }
        };
        let b = bounds.unwrap_or(crate::geo::Aabb::new(Vec3::splat(-1.0), Vec3::splat(1.0)));
        let mut items = Vec::new();
        for &i in &self.layout.roots {
            let n = &self.layout.nodes[i];
            let c = n.body.center();
            let s = n.body.size();
            items.push(Item { id: &n.id, kind: &n.kind_name, x: round2(c.x), z: round2(c.z), w: round2(s.x), d: round2(s.z), level: n.level.as_str() });
        }
        if let Some(room) = &self.layout.room {
            for (kind, f) in &room.furniture {
                let c = f.center();
                let s = f.size();
                items.push(Item { id: "", kind, x: round2(c.x), z: round2(c.z), w: round2(s.x), d: round2(s.z), level: "unknown" });
            }
        }
        let m = Minimap {
            mode: self.mode_name(),
            player,
            bounds: Bounds { min_x: round2(b.min.x), min_z: round2(b.min.z), max_x: round2(b.max.x), max_z: round2(b.max.z) },
            items,
        };
        serde_json::to_string(&m).unwrap_or_else(|_| "{}".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn walking() -> Engine {
        let mut e = Engine::new();
        e.resize(1200.0, 800.0, 2.0);
        e.set_scene(include_str!("../fixtures/scene.json")).unwrap();
        e.update(0.0);
        e.set_mode("walk");
        e
    }

    fn run(e: &mut Engine, secs: f64) {
        let mut t = e.now;
        let end = t + secs * 1000.0;
        while t < end {
            t += 1000.0 / 60.0;
            e.update(t);
        }
    }

    #[test]
    fn spawns_at_entrance_facing_the_racks() {
        let e = walking();
        assert_eq!(e.mode_name(), "walk");
        let room = e.layout.room.as_ref().unwrap();
        assert!(e.walker.pos.y > room.bounds.max.z - 1.5);
        // The rack is ahead of the player.
        let host = &e.layout.nodes[e.layout.hosts[0]];
        let to = host.body.center() - e.cam.eye();
        assert!(to.normalize().dot(e.walker.forward()) > 0.8);
        assert!((e.cam.eye().y - EYE).abs() < 0.05);
    }

    #[test]
    fn walking_into_the_rack_stops_at_it_and_aims_at_it() {
        let mut e = walking();
        e.key_down("KeyW");
        e.key_down("ShiftLeft");
        run(&mut e, 5.0);
        e.key_up("KeyW");
        e.key_up("ShiftLeft");
        run(&mut e, 0.5);
        let host = &e.layout.nodes[e.layout.hosts[0]];
        assert!(e.walker.pos.y > host.body.max.z + walk::RADIUS - 0.01, "inside the rack at {}", e.walker.pos);
        assert!(e.walker.pos.y < host.body.max.z + 0.6);
        // Crosshair at eye height hits something in the rack.
        let h = e.hovered.expect("aims at the rack");
        assert_eq!(e.root_of(h), e.layout.hosts[0]);
        e.key_down("KeyE");
        assert_eq!(e.selected, Some(h));
        // Close up: workload labels show, other racks' labels do not exist.
        let labels = e.labels();
        assert!(labels.iter().any(|l| l.kind == "workload"), "{}", e.labels_json());
        assert!(labels.iter().all(|l| l.depth >= 0.0 && l.depth <= 1.0));
    }

    #[test]
    fn mode_switch_restores_orbit_camera() {
        let mut e = Engine::new();
        e.resize(1200.0, 800.0, 1.0);
        e.set_scene(include_str!("../fixtures/scene.json")).unwrap();
        e.key("ArrowLeft");
        run(&mut e, 1.0);
        let pose = e.cam.pose;
        let eye = e.cam.eye();
        e.set_mode("walk");
        e.key_down("KeyW");
        run(&mut e, 1.0);
        e.set_mode("orbit");
        run(&mut e, 0.1);
        assert_eq!(e.cam.pose, pose);
        assert!(e.cam.eye().distance(eye) < 1e-4);
        assert_eq!(e.mode_name(), "orbit");
        assert!(e.layout.room.is_none());
    }

    #[test]
    fn minimap_reports_player_and_items() {
        let mut e = walking();
        let v: serde_json::Value = serde_json::from_str(&e.minimap_json()).unwrap();
        let x0 = v["player"]["x"].as_f64().unwrap();
        let z0 = v["player"]["z"].as_f64().unwrap();
        assert!(v["bounds"]["minX"].as_f64().unwrap() < x0 && x0 < v["bounds"]["maxX"].as_f64().unwrap());
        let items = v["items"].as_array().unwrap();
        assert!(items.iter().any(|i| i["kind"] == "host" && i["id"] == "host:12"));
        assert!(items.iter().any(|i| i["kind"] == "domain"));
        assert!(items.iter().any(|i| i["kind"] == "door"));
        e.key_down("KeyW");
        run(&mut e, 1.0);
        let v: serde_json::Value = serde_json::from_str(&e.minimap_json()).unwrap();
        assert!(v["player"]["z"].as_f64().unwrap() < z0 - 0.5);
        assert!(v["player"]["heading"].as_f64().unwrap().abs() < 1e-3);
    }

    #[test]
    fn focus_walks_up_to_the_element() {
        let mut e = walking();
        let w = e.layout.index["wl:slowbloom/slowbloom/Deployment/api"];
        e.focus_id("wl:slowbloom/slowbloom/Deployment/api");
        run(&mut e, 1.5);
        let wb = e.layout.nodes[w].body;
        let eye = e.cam.eye();
        assert!(eye.distance(wb.center()) < 2.4, "eye {eye} far from {}", wb.center());
        assert_eq!(e.selected, Some(w));
        assert_eq!(e.hovered.map(|h| e.root_of(h)), Some(e.layout.hosts[0]));
        // Escape-equivalent from the page is not ours; reset_view walks back to the door.
        e.reset_view();
        run(&mut e, 1.5);
        let room = e.layout.room.as_ref().unwrap();
        assert!(e.walker.pos.distance(Vec2::new(room.spawn.x, room.spawn.z)) < 0.01);
    }

    #[test]
    fn rescene_while_walking_keeps_position() {
        let mut e = walking();
        e.key_down("KeyD");
        run(&mut e, 0.8);
        e.key_up("KeyD");
        run(&mut e, 0.5);
        let p = e.walker.pos;
        e.set_scene(include_str!("../fixtures/scene.json")).unwrap();
        run(&mut e, 0.1);
        assert!(e.walker.pos.distance(p) < 1e-4);
        e.set_scene("{}").unwrap();
        run(&mut e, 0.1);
        assert!(e.labels().is_empty());
        e.look(10.0, f32::NAN);
        e.interact();
    }
}
