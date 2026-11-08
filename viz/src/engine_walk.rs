//! Walk mode (docs/topology.md sections 4 and 5): first-person movement through the night city,
//! crosshair picking, nearby labels with their metaphor meaning, aim info and the minimap.

use glam::{Vec2, Vec3, Vec4Swizzles};
use serde::Serialize;

use crate::engine::{Engine, Label, Mode};
use crate::layout::{self, Kind, Layout};
use crate::city::{self, beam_dir};
use crate::geo::{rgba, GLOW};
use crate::walk::{self, Walker, EYE, REACH};

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
            Mode::Walk => city::build_city(&self.scene),
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
        let Some(city) = self.layout.room.as_ref() else { return };
        let n = &self.layout.nodes[idx];
        let b = n.body;
        let c = b.center();
        let f = if n.facing == Vec3::ZERO {
            // Searchlights, lamps, cranes: look at them from where we are.
            (self.walker.pos - Vec2::new(c.x, c.z)).normalize_or(Vec2::Y)
        } else {
            Vec2::new(n.facing.x, n.facing.z).normalize_or(Vec2::Y)
        };
        let half = (b.size().x * f.x.abs() + b.size().z * f.y.abs()) * 0.5;
        let front = Vec2::new(c.x, c.z) + f * half;
        // Stand far enough back that the element sits comfortably in view.
        let mut dist: f32 = match n.kind {
            Kind::Host => 12.0,
            Kind::Group => 6.0,
            Kind::Workload => 3.0,
            Kind::Image => 1.6,
            Kind::External => match n.kind_name.as_str() {
                "repository" => 1.7,
                "image" => 2.5,
                "source" | "registry" => 14.0,
                _ => 3.0,
            },
        };
        let rise = (c.y - EYE).max(0.0);
        dist = dist.max(rise / 0.62f32.tan());
        let mut stand = front + f * dist;
        while walk::blocked(stand, &city.colliders) && dist > 0.4 {
            dist -= 0.25;
            stand = front + f * dist;
        }
        if walk::blocked(stand, &city.colliders) {
            stand = self.walker.pos;
        }
        let d = Vec2::new(front.x - stand.x, front.y - stand.y);
        let heading = d.x.atan2(-d.y);
        let pitch = (c.y - EYE).atan2(d.length().max(0.1)).clamp(-0.7, 1.2);
        let ms = if self.reduced_motion { 0.0 } else { GLIDE_MS };
        self.walker.glide_to(stand, heading, pitch, self.now, ms);
        self.sync_fps();
    }

    pub(crate) fn walk_update(&mut self, now: f64, dt: f32) {
        let changed = match self.layout.room.as_ref() {
            Some(city) => self.walker.step(now, dt, &city.colliders),
            None => self.walker.step(now, dt, &[]),
        };
        let beams_move = !self.reduced_motion && self.layout.room.as_ref().is_some_and(|c| c.beams.iter().any(|b| b.on));
        if changed || self.cam.fps.is_none() {
            self.sync_fps();
            self.dirty = true;
            self.walk_update_hover();
        } else if beams_move && self.walker.keys == 0 {
            // A sweeping beam can cross the crosshair.
            self.walk_update_hover();
        }
    }

    /// Searchlight beams for this frame (no allocation once the buffer has grown).
    pub(crate) fn rebuild_dynamic(&mut self) {
        self.dynamic.clear();
        self.dynamic_version += 1;
        let Some(city) = self.layout.room.as_ref() else { return };
        for beam in city.beams.iter().filter(|b| b.on) {
            let dir = beam_dir(beam, self.now, self.reduced_motion);
            let len = 150.0;
            let hovered = self.hovered == Some(beam.node) || self.selected == Some(beam.node);
            let tint = if hovered { [1.0, 0.85, 0.55] } else { [0.75, 0.85, 1.0] };
            let seg = 8;
            for s in 0..seg {
                let t0 = s as f32 / seg as f32;
                let t1 = (s + 1) as f32 / seg as f32;
                let a = beam.origin + dir * len * t0;
                let b = beam.origin + dir * len * t1;
                let fade = 1.0 - t0;
                for (w, alpha) in [(3.0, 0.5), (14.0, 0.14), (40.0, 0.06)] {
                    self.dynamic.line(a, b, rgba(tint, alpha * fade), w * (1.0 + t0 * 1.5), crate::geo::Lod::ALWAYS, GLOW);
                }
            }
        }
    }

    // ---- picking -----------------------------------------------------------------------

    /// What the ray at (x, y) aims at: the most detailed visible element within reach,
    /// a searchlight beam, or nothing (walls of the library and warehouse stop the ray).
    pub fn pick_walk(&self, x: f32, y: f32) -> Option<usize> {
        let (o, d) = self.cam.ray(x, y, self.css_w, self.css_h);
        let eye = self.cam.eye();
        let nodes = &self.layout.nodes;
        let mut reach = REACH;
        if let Some(city) = self.layout.room.as_ref() {
            for b in &city.blockers {
                if let Some(t) = b.ray_hit(o, d) {
                    if !b.contains(eye) {
                        reach = reach.min(t);
                    }
                }
            }
        }
        let nearest = |cands: &[usize], reach: f32| -> Option<(f32, usize)> {
            let mut best: Option<(f32, usize)> = None;
            for &i in cands {
                if nodes[i].lod.vis(eye) < 0.5 {
                    continue;
                }
                if let Some(t) = nodes[i].pick.ray_hit(o, d) {
                    if t <= reach && best.is_none_or(|(bt, _)| t < bt) {
                        best = Some((t, i));
                    }
                }
            }
            best
        };
        let mut found = nearest(&self.layout.roots, reach);
        if let Some((_, mut cur)) = found {
            while let Some((_, c)) = nearest(&nodes[cur].children, reach) {
                cur = c;
            }
            found = found.map(|(t, _)| (t, cur));
        }
        // Searchlight beams: aim anywhere along the shaft.
        if let Some(city) = self.layout.room.as_ref() {
            for beam in city.beams.iter().filter(|b| b.on) {
                let dir = beam_dir(beam, self.now, self.reduced_motion);
                if let Some((t, dist)) = ray_segment(o, d, beam.origin, beam.origin + dir * 150.0) {
                    if dist < 1.2 + t * 0.012 && found.is_none_or(|(ft, _)| t < ft) {
                        found = Some((t, beam.node));
                    }
                }
            }
        }
        found.map(|f| f.1)
    }

    pub(crate) fn walk_update_hover(&mut self) {
        let h = self.pick_walk(self.css_w * 0.5, self.css_h * 0.5);
        if h != self.hovered {
            self.hovered = h;
            self.dirty = true;
        }
    }

    /// JSON `{ id, title, meaning }` for the crosshair target, or `null`.
    pub fn aim_info_json(&self) -> String {
        #[derive(Serialize)]
        struct Aim<'a> {
            id: &'a str,
            title: &'a str,
            meaning: &'a str,
        }
        match self.hovered.map(|i| &self.layout.nodes[i]) {
            Some(n) => serde_json::to_string(&Aim { id: &n.id, title: if n.title.is_empty() { &n.text } else { &n.title }, meaning: &n.meaning }).unwrap_or_else(|_| "null".into()),
            None => "null".into(),
        }
    }

    // ---- labels ------------------------------------------------------------------------

    fn root_of(&self, mut i: usize) -> usize {
        while let Some(p) = self.layout.nodes[i].parent {
            i = p;
        }
        i
    }

    fn occluded(&self, eye: Vec3, p: Vec3, own: usize) -> bool {
        let d = p - eye;
        let len = d.length();
        if len < 1e-4 {
            return false;
        }
        let dir = d / len;
        let blocked = |b: &crate::geo::Aabb| !b.contains(eye) && b.ray_hit(eye, dir).is_some_and(|t| t < len - 0.3);
        self.layout.hosts.iter().any(|&h| h != own && blocked(&self.layout.nodes[h].body))
            || self.layout.room.as_ref().is_some_and(|c| c.blockers.iter().any(|b| !b.contains(p) && blocked(b)))
    }

    fn ancestor_of_kind(&self, i: usize, k: Kind) -> Option<usize> {
        let mut cur = Some(i);
        while let Some(c) = cur {
            if self.layout.nodes[c].kind == k {
                return Some(c);
            }
            cur = self.layout.nodes[c].parent;
        }
        None
    }

    pub(crate) fn labels_walk(&self) -> Vec<Label<'_>> {
        // Window and image labels only for the floor / window you are aiming at (or selected).
        let aim = [self.hovered, self.selected];
        let groups = aim.map(|a| a.and_then(|a| self.ancestor_of_kind(a, Kind::Group)));
        let workloads = aim.map(|a| a.and_then(|a| self.ancestor_of_kind(a, Kind::Workload)));
        let vp = self.cam.view_proj();
        let eye = self.cam.eye();
        let px_per_m_at_w = self.css_h * 0.5 / (self.cam.fov() * 0.5).tan();
        let mut out = Vec::new();
        for (i, n) in self.layout.nodes.iter().enumerate() {
            let d = eye.distance(n.label_at);
            let special = Some(i) == self.selected || Some(i) == self.hovered;
            let range = match (n.kind, n.kind_name.as_str()) {
                (Kind::Host, _) => 140.0,
                (Kind::Group, _) => 90.0,
                (Kind::Workload, _) => 45.0,
                (Kind::Image, _) => 12.0,
                (_, "repository") => 0.0,
                (_, "image") => 12.0,
                (_, "domain") => 18.0,
                (_, "lamp") | (_, "source") => 0.0,
                _ => 60.0,
            };
            if d > range && !(special && d < 160.0) {
                continue;
            }
            // Painted / mounted labels are only readable from the side they face.
            if n.facing != Vec3::ZERO && (eye - n.label_at).dot(n.facing) <= 0.0 {
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
                    Kind::Group => px >= 14.0,
                    Kind::Workload => vis > 0.6 && px >= 22.0 && groups.contains(&n.parent),
                    Kind::Image => vis > 0.6 && px >= 20.0 && workloads.contains(&n.parent),
                    Kind::External => px >= 26.0,
                };
            if !show {
                continue;
            }
            let own = self.root_of(i);
            if self.occluded(eye, n.label_at, own) {
                continue;
            }
            out.push(Label {
                id: &n.id,
                text: &n.text,
                sub: &n.sub,
                x: ((ndc.x * 0.5 + 0.5) * self.css_w * 10.0).round() / 10.0,
                y: ((0.5 - ndc.y * 0.5) * self.css_h * 10.0).round() / 10.0,
                depth: ((ndc.z * 0.5 + 0.5) * 10000.0).round() / 10000.0,
                level: n.level.as_str(),
                kind: &n.kind_name,
                anchor: n.anchor.as_str(),
                stale: n.stale,
                selected: Some(i) == self.selected,
                hovered: Some(i) == self.hovered,
                meaning: &n.meaning,
            });
        }
        // Declutter painted floor names: nearer ones win when two would overlap on screen.
        out.sort_by(|a, b| a.depth.total_cmp(&b.depth));
        let mut kept: Vec<(f32, f32)> = Vec::new();
        out.retain(|l| {
            if l.kind != "group" || l.selected || l.hovered {
                return true;
            }
            if kept.iter().any(|&(x, y)| (l.y - y).abs() < 16.0 && (l.x - x).abs() < 140.0) {
                return false;
            }
            kept.push((l.x, l.y));
            true
        });
        out.sort_by(|a, b| b.depth.total_cmp(&a.depth));
        out
    }

    // ---- minimap -----------------------------------------------------------------------

    pub fn minimap_json(&self) -> String {
        let (player, bounds) = match self.mode {
            Mode::Walk => (Player { x: round2(self.walker.pos.x), z: round2(self.walker.pos.y), heading: round2(self.walker.heading) }, self.layout.bounds),
            Mode::Orbit => {
                let p = self.cam.pose;
                (Player { x: round2(p.target.x), z: round2(p.target.z), heading: round2(-p.yaw) }, self.layout.bounds.map(|b| b.grow(0.9)))
            }
        };
        let mut items = Vec::new();
        let mut raw: Vec<(&str, &'static str, crate::geo::Aabb, &'static str)> = Vec::new();
        for &i in &self.layout.roots {
            let n = &self.layout.nodes[i];
            let kind: &'static str = match (self.mode, n.kind, n.kind_name.as_str()) {
                (Mode::Orbit, Kind::Host, _) => "host",
                (Mode::Walk, Kind::Host, _) => "building",
                (Mode::Walk, _, "domain") => "door",
                (Mode::Walk, _, "registry") => "crane",
                (Mode::Walk, _, "lamp" | "repository" | "image" | "source") => continue,
                (Mode::Walk, _, _) => "kiosk",
                (Mode::Orbit, _, k) => match k {
                    "repository" => "repository",
                    "domain" => "domain",
                    "registry" => "registry",
                    "image" => "image",
                    "host" => "host",
                    "cluster" => "cluster",
                    _ => "external",
                },
            };
            raw.push((&n.id, kind, n.body, n.level.as_str()));
        }
        if let Some(city) = &self.layout.room {
            for (kind, f) in &city.furniture {
                raw.push(("", kind, *f, "unknown"));
            }
        }
        for (id, kind, b, level) in &raw {
            let c = b.center();
            let s = b.size();
            items.push(Item { id, kind, x: round2(c.x), z: round2(c.z), w: round2(s.x), d: round2(s.z), level });
        }
        // Streets and squares first so buildings draw on top.
        items.sort_by_key(|it| !matches!(it.kind, "square" | "quay" | "street"));
        let b = bounds.unwrap_or(crate::geo::Aabb::new(Vec3::splat(-1.0), Vec3::splat(1.0)));
        let b = match &self.layout.room {
            // The minimap shows the walkable city, not the water and sky beyond it.
            Some(c) => crate::geo::Aabb::new(Vec3::new(b.min.x, 0.0, b.min.z), Vec3::new(c.water_x + 4.0, 0.0, b.max.z)),
            None => b,
        };
        let m = Minimap {
            mode: self.mode_name(),
            player,
            bounds: Bounds { min_x: round2(b.min.x), min_z: round2(b.min.z), max_x: round2(b.max.x), max_z: round2(b.max.z) },
            items,
        };
        serde_json::to_string(&m).unwrap_or_else(|_| "{}".into())
    }
}

/// Closest approach between a ray and a segment: (ray parameter, distance).
fn ray_segment(o: Vec3, d: Vec3, a: Vec3, b: Vec3) -> Option<(f32, f32)> {
    let u = b - a;
    let w = o - a;
    let (aa, bb, cc, dd, ee) = (d.dot(d), d.dot(u), u.dot(u), d.dot(w), u.dot(w));
    let den = aa * cc - bb * bb;
    if den.abs() < 1e-8 {
        return None;
    }
    let t = ((bb * ee - cc * dd) / den).max(0.0);
    let s = ((aa * ee - bb * dd) / den).clamp(0.0, 1.0);
    let p = o + d * t;
    let q = a + u * s;
    Some((t, p.distance(q)))
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
    fn spawns_on_the_square_looking_down_the_street() {
        let e = walking();
        assert_eq!(e.mode_name(), "walk");
        let host = &e.layout.nodes[e.layout.hosts[0]];
        let to = host.body.center() - e.cam.eye();
        let f = e.walker.forward();
        assert!(Vec2::new(to.x, to.z).normalize().dot(Vec2::new(f.x, f.z).normalize()) > 0.8);
        assert!((e.cam.eye().y - EYE).abs() < 0.05);
        // Floor names and the building sign are readable from the square.
        let labels = e.labels();
        assert!(labels.iter().any(|l| l.kind == "host"));
        assert!(labels.iter().any(|l| l.kind == "group" && !l.meaning.is_empty()), "{}", e.labels_json());
    }

    #[test]
    fn walking_down_the_street_is_stopped_by_the_building() {
        let mut e = walking();
        // Turn to face the building front (north) and run into it.
        let host = e.layout.nodes[e.layout.hosts[0]].body;
        e.walk_to(e.layout.hosts[0]);
        run(&mut e, 1.5);
        e.walker.heading = 0.0;
        e.key_down("KeyW");
        e.key_down("ShiftLeft");
        run(&mut e, 8.0);
        e.key_up("KeyW");
        run(&mut e, 0.5);
        assert!(e.walker.pos.y > host.max.z + walk::RADIUS - 0.01, "walked into the building at {}", e.walker.pos);
    }

    #[test]
    fn aim_info_names_the_thing_and_its_meaning() {
        let mut e = walking();
        assert_eq!(e.aim_info_json(), "null");
        e.focus_id("wl:slowbloom/ghost/Deployment/ghost");
        run(&mut e, 1.5);
        let v: serde_json::Value = serde_json::from_str(&e.aim_info_json()).unwrap();
        let id = v["id"].as_str().unwrap();
        assert!(id.contains("ghost"), "aiming at {id}");
        if id == "wl:slowbloom/ghost/Deployment/ghost" {
            assert_eq!(v["title"], "ghost · ghost");
            assert!(v["meaning"].as_str().unwrap().contains("window left open"));
        }
        e.interact();
        assert!(e.selected.is_some());
        // Window labels for the aimed floor show, with meanings.
        assert!(e.labels().iter().any(|l| l.kind == "workload" && l.meaning.contains("window")), "{}", e.labels_json());
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
        assert!(e.layout.room.is_none());
        assert!(e.labels().iter().all(|l| l.meaning.is_empty()));
    }

    #[test]
    fn minimap_shows_the_city() {
        let mut e = walking();
        let v: serde_json::Value = serde_json::from_str(&e.minimap_json()).unwrap();
        let items = v["items"].as_array().unwrap();
        for k in ["building", "library", "warehouse", "street", "door", "square"] {
            assert!(items.iter().any(|i| i["kind"] == k), "no {k}");
        }
        assert!(items.iter().any(|i| i["kind"] == "building" && i["id"] == "host:12"));
        let z0 = v["player"]["z"].as_f64().unwrap();
        let x0 = v["player"]["x"].as_f64().unwrap();
        e.key_down("KeyW");
        run(&mut e, 1.0);
        let v: serde_json::Value = serde_json::from_str(&e.minimap_json()).unwrap();
        let (x1, z1) = (v["player"]["x"].as_f64().unwrap(), v["player"]["z"].as_f64().unwrap());
        assert!(x1 > x0 + 0.8 && (z1 - z0).abs() < 0.5);
    }

    #[test]
    fn searchlight_beams_are_aimable() {
        let mut e = walking();
        let city = e.layout.room.as_ref().unwrap();
        let beam = *city.beams.iter().find(|b| b.on).unwrap();
        // Look straight along the beam from below its origin.
        let dir = beam_dir(&beam, e.now, e.reduced_motion);
        let target = beam.origin + dir * 40.0;
        let to = target - e.cam.eye();
        e.walker.heading = to.x.atan2(-to.z);
        e.walker.pitch = (to.y / Vec2::new(to.x, to.z).length()).atan();
        e.reduced_motion = true; // freeze the sweep
        e.look(0.0, 0.0);
        let h = e.hovered.map(|h| e.layout.nodes[h].id.clone());
        assert!(h.as_deref().is_some_and(|id| id.starts_with("source:")), "aimed at {h:?}");
        let v: serde_json::Value = serde_json::from_str(&e.aim_info_json()).unwrap();
        assert!(v["meaning"].as_str().unwrap().contains("searchlight sweeping"));
        run(&mut e, 0.2);
        assert!(e.dynamic.line_count() > 0);
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
        e.look(10.0, f32::NAN);
        e.interact();
        let _ = e.minimap_json();
    }
}
