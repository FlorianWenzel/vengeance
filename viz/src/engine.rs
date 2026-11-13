//! Renderer-independent state: scene layout, camera, input, picking, labels.
//! Everything here is plain Rust so it is unit-testable without a browser.

use glam::{Vec2, Vec3, Vec4Swizzles};
use serde::Serialize;

use crate::camera::{Camera, Pose};
use crate::geo::{rgba, Aabb, Geo, Lod, DASH};
use crate::layout::{self, Kind, Layout, AMBER};
use crate::scene::Scene;

const DRAG_THRESHOLD: f32 = 4.0;
const FLY_MS: f64 = 900.0;
const OVERVIEW_YAW: f32 = 0.48;
const OVERVIEW_PITCH: f32 = 0.42;

#[derive(Clone, Copy, Debug, PartialEq)]
enum DragMode {
    Orbit,
    Pan,
}

#[derive(Clone, Copy, Debug)]
struct Drag {
    start: Vec2,
    last: Vec2,
    button: i32,
    mode: DragMode,
    moved: bool,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct Label<'a> {
    pub id: &'a str,
    pub text: &'a str,
    pub sub: &'a str,
    pub x: f32,
    pub y: f32,
    pub depth: f32,
    pub level: &'static str,
    pub kind: &'a str,
    /// Where the text goes relative to (x, y): above | right | inside | below.
    pub anchor: &'static str,
    pub stale: bool,
    pub selected: bool,
    pub hovered: bool,
    /// Walk mode: what the metaphor means (empty in orbit mode).
    pub meaning: &'a str,
}

pub struct Engine {
    pub layout: Layout,
    /// Bumped whenever `layout.geo` changes (the GL side re-uploads).
    pub geo_version: u64,
    pub cam: Camera,
    pub css_w: f32,
    pub css_h: f32,
    pub dpr: f32,
    pub now: f64,
    pub hovered: Option<usize>,
    pub selected: Option<usize>,
    pub focus: Option<usize>,
    pub shift: bool,
    pub reduced_motion: bool,
    pub highlight: Geo,
    pub highlight_version: u64,
    pub(crate) highlight_key: (Option<usize>, Option<usize>),
    pub(crate) pointer: Option<Vec2>,
    drag: Option<Drag>,
    pub(crate) dirty: bool,
    pub(crate) has_scene: bool,
    pub mode: Mode,
    pub(crate) scene: Scene,
    pub walker: crate::walk::Walker,
    pub(crate) last_now: Option<f64>,
    pub(crate) orbit_framed: bool,
    pub(crate) look_drag: Option<(Vec2, Vec2, bool)>,
    /// Per-frame geometry (searchlight beams), rebuilt in place.
    pub dynamic: Geo,
    pub dynamic_version: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Orbit,
    Walk,
}

impl Default for Engine {
    fn default() -> Self {
        Engine::new()
    }
}

impl Engine {
    pub fn new() -> Engine {
        Engine {
            layout: Layout::default(),
            geo_version: 0,
            cam: Camera::default(),
            css_w: 300.0,
            css_h: 150.0,
            dpr: 1.0,
            now: 0.0,
            hovered: None,
            selected: None,
            focus: None,
            shift: false,
            reduced_motion: false,
            highlight: Geo::default(),
            highlight_version: 0,
            highlight_key: (None, None),
            pointer: None,
            drag: None,
            dirty: true,
            has_scene: false,
            mode: Mode::Orbit,
            scene: Scene::default(),
            walker: Default::default(),
            last_now: None,
            orbit_framed: false,
            look_drag: None,
            dynamic: Geo::default(),
            dynamic_version: 0,
        }
    }

    fn fly_ms(&self) -> f64 {
        if self.reduced_motion {
            0.0
        } else {
            FLY_MS
        }
    }

    // ---- scene -------------------------------------------------------------------------

    pub fn set_scene(&mut self, json: &str) -> Result<(), String> {
        let scene = Scene::parse(json)?;
        self.apply_scene(&scene);
        Ok(())
    }

    pub fn apply_scene(&mut self, scene: &Scene) {
        self.scene = scene.clone();
        if self.mode == Mode::Walk {
            self.rebuild();
            self.after_walk_rebuild();
            self.has_scene = true;
            return;
        }
        let id_of =
            |l: &Layout, i: Option<usize>| i.and_then(|i| l.nodes.get(i)).map(|n| n.id.clone());
        let focus_id = id_of(&self.layout, self.focus);
        let sel_id = id_of(&self.layout, self.selected);
        let hov_id = id_of(&self.layout, self.hovered);
        let old_focus_center = self.focus.map(|f| self.focus_target(f));
        let old_hosts: Vec<String> = self
            .layout
            .hosts
            .iter()
            .map(|&i| self.layout.nodes[i].id.clone())
            .collect();
        let had_content = self.has_scene && !self.layout.nodes.is_empty();

        self.layout = layout::build(scene);
        self.geo_version += 1;
        let find = |l: &Layout, id: Option<String>| id.and_then(|id| l.index.get(&id).copied());
        self.focus = find(&self.layout, focus_id.clone());
        self.selected = find(&self.layout, sel_id);
        self.hovered = find(&self.layout, hov_id);
        self.highlight_key = (None, Some(usize::MAX)); // force a rebuild
        let r = self.layout.bounds.map(|b| b.radius()).unwrap_or(2.0);
        self.cam.max_dist = r * 5.0 + 8.0;

        let new_hosts: Vec<String> = self
            .layout
            .hosts
            .iter()
            .map(|&i| self.layout.nodes[i].id.clone())
            .collect();
        if !had_content || !self.orbit_framed {
            self.orbit_framed = !self.layout.nodes.is_empty();
            self.cam.stop();
            self.cam.pose = self.overview_pose();
        } else if focus_id.is_some() && self.focus.is_none() {
            let to = self.overview_pose();
            self.cam.fly_to(to, self.now, self.fly_ms());
        } else if let (Some(f), Some(old)) = (self.focus, old_focus_center) {
            let delta = self.focus_target(f) - old;
            if delta.length() > 0.005 {
                let mut to = self.cam.destination();
                to.target += delta;
                self.cam.fly_to(to, self.now, self.fly_ms());
            }
        } else if self.focus.is_none() && old_hosts != new_hosts {
            let to = self.overview_pose();
            self.cam.fly_to(to, self.now, self.fly_ms());
        }
        self.has_scene = true;
        self.dirty = true;
    }

    // ---- camera poses ------------------------------------------------------------------

    pub fn overview_pose(&self) -> Pose {
        let b = self.layout.bounds.unwrap_or(Aabb::new(
            Vec3::new(-1.0, 0.0, -1.0),
            Vec3::new(1.0, 2.0, 1.0),
        ));
        self.cam.frame_box(&b, OVERVIEW_YAW, OVERVIEW_PITCH, 0.9)
    }

    /// The front face of an element (what the camera wants to look at).
    fn front_box(&self, idx: usize) -> Aabb {
        let n = &self.layout.nodes[idx];
        match n.kind {
            Kind::Group | Kind::Workload => {
                let b = n.body;
                Aabb::new(Vec3::new(b.min.x, b.min.y, b.max.z - 0.06), b.max)
            }
            Kind::Image => n.body.grow(0.004),
            _ => n.pick,
        }
    }

    fn focus_target(&self, idx: usize) -> Vec3 {
        self.front_box(idx).center()
    }

    pub fn pose_for(&self, idx: usize) -> Pose {
        let n = &self.layout.nodes[idx];
        let cur = self.cam.destination().yaw;
        let front = |lim: f32| {
            let y = cur.clamp(-lim, lim);
            if y.abs() < 0.08 {
                0.18
            } else {
                y
            }
        };
        let b = self.front_box(idx);
        let (yaw, pitch, margin) = match n.kind {
            Kind::Host => (front(0.4), 0.26, 0.88),
            Kind::Group => (front(0.45), 0.20, 0.55),
            Kind::Workload => (front(0.35), 0.16, 0.42),
            Kind::Image => (front(0.3), 0.14, 0.3),
            Kind::External => (cur, 0.40, 0.6),
        };
        self.cam.frame_box(&b, yaw, pitch, margin)
    }

    pub fn focus_index(&mut self, idx: usize) {
        self.focus = Some(idx);
        self.selected = Some(idx);
        let to = self.pose_for(idx);
        self.cam.fly_to(to, self.now, self.fly_ms());
        self.dirty = true;
    }

    pub fn focus_id(&mut self, id: &str) {
        if let Some(&i) = self.layout.index.get(id) {
            if self.mode == Mode::Walk {
                self.walk_to(i);
            } else {
                self.focus_index(i);
            }
        }
    }

    pub fn reset_view(&mut self) {
        if self.mode == Mode::Walk {
            return self.walk_respawn(true);
        }
        self.focus = None;
        let to = self.overview_pose();
        self.cam.fly_to(to, self.now, self.fly_ms());
        self.dirty = true;
    }

    /// Escape: image → workload → group → host → overview (then clears the selection).
    pub fn zoom_out(&mut self) {
        let cur = self.focus.or(self.selected);
        match cur.and_then(|f| self.layout.nodes[f].parent) {
            Some(p) => self.focus_index(p),
            None if self.focus.is_some() => {
                self.selected = None;
                self.reset_view();
            }
            None => {
                self.selected = None;
                self.reset_view();
            }
        }
    }

    // ---- input -------------------------------------------------------------------------

    pub fn resize(&mut self, w: f32, h: f32, dpr: f32) {
        self.css_w = if w.is_finite() { w.max(1.0) } else { 1.0 };
        self.css_h = if h.is_finite() { h.max(1.0) } else { 1.0 };
        self.dpr = if dpr.is_finite() && dpr > 0.0 {
            dpr.min(4.0)
        } else {
            1.0
        };
        self.cam.aspect = self.css_w / self.css_h;
        self.dirty = true;
    }

    fn inside(&self, p: Vec2) -> bool {
        p.x >= 0.0 && p.y >= 0.0 && p.x <= self.css_w && p.y <= self.css_h
    }

    pub fn pointer_down(&mut self, x: f32, y: f32, button: i32) {
        if self.mode == Mode::Walk {
            return self.walk_pointer_down(x, y, button);
        }
        let p = Vec2::new(x, y);
        let mode = if button == 0 && !self.shift {
            DragMode::Orbit
        } else {
            DragMode::Pan
        };
        self.drag = Some(Drag {
            start: p,
            last: p,
            button,
            mode,
            moved: false,
        });
        self.pointer = Some(p);
    }

    pub fn pointer_move(&mut self, x: f32, y: f32) {
        if self.mode == Mode::Walk {
            return self.walk_pointer_move(x, y);
        }
        let p = Vec2::new(x, y);
        if let Some(mut d) = self.drag {
            if !d.moved && p.distance(d.start) > DRAG_THRESHOLD {
                d.moved = true;
                self.cam.stop();
            }
            if d.moved {
                let delta = p - d.last;
                match d.mode {
                    DragMode::Orbit => self.orbit(-delta.x * 0.006, delta.y * 0.005),
                    DragMode::Pan => self.pan(delta),
                }
            }
            d.last = p;
            self.drag = Some(d);
        }
        self.pointer = if self.inside(p) { Some(p) } else { None };
        self.update_hover();
    }

    pub fn pointer_up(&mut self, x: f32, y: f32, button: i32) {
        if self.mode == Mode::Walk {
            return self.walk_pointer_up(x, y, button);
        }
        let Some(d) = self.drag.take() else { return };
        if d.moved || d.button != button {
            return;
        }
        if button == 0 {
            match self.pick(x, y) {
                Some(i) => self.focus_index(i),
                None => {
                    self.selected = None;
                    self.dirty = true;
                }
            }
        }
    }

    fn orbit(&mut self, dyaw: f32, dpitch: f32) {
        let mut p = self.cam.pose;
        p.yaw += dyaw;
        p.pitch += dpitch;
        Camera::clamp(&mut p, self.cam.max_dist);
        self.cam.pose = p;
        self.dirty = true;
    }

    fn pan(&mut self, delta: Vec2) {
        let p = &mut self.cam.pose;
        let per_px = 2.0 * p.dist * (self.cam.fovy * 0.5).tan() / self.css_h;
        let eye = p.eye();
        let fwd = (p.target - eye).normalize_or(Vec3::NEG_Z);
        let right = fwd.cross(Vec3::Y).normalize_or(Vec3::X);
        let up = right.cross(fwd);
        p.target += (-right * delta.x + up * delta.y) * per_px;
        // Keep the pivot within reach of the scene.
        if let Some(b) = self.layout.bounds {
            let b = b.grow(b.radius() + 2.0);
            p.target = p.target.clamp(b.min, b.max);
        }
        p.target.y = p.target.y.max(0.0);
        self.dirty = true;
    }

    pub fn wheel(&mut self, delta_y: f32, x: f32, y: f32) {
        if !delta_y.is_finite() || delta_y == 0.0 || self.mode == Mode::Walk {
            return;
        }
        self.cam.stop();
        let factor = (delta_y * 0.0015).clamp(-0.7, 0.7).exp();
        let pose = self.cam.pose;
        let new_dist = (pose.dist * factor).clamp(crate::camera::MIN_DIST, self.cam.max_dist);
        let f = new_dist / pose.dist;
        let (o, d) = self.cam.ray(x, y, self.css_w, self.css_h);
        let hit = self
            .pick_ray(o, d)
            .and_then(|i| {
                self.layout.nodes[i]
                    .body
                    .ray_hit(o, d)
                    .or_else(|| self.layout.nodes[i].pick.ray_hit(o, d))
            })
            .or_else(|| if d.y < -1e-4 { Some(-o.y / d.y) } else { None })
            .map(|t| o + d * t)
            .unwrap_or(pose.target);
        let mut p = pose;
        p.dist = new_dist;
        p.target = hit + (pose.target - hit) * f;
        Camera::clamp(&mut p, self.cam.max_dist);
        self.cam.pose = p;
        self.dirty = true;
        self.update_hover();
    }

    pub fn key(&mut self, code: &str) {
        if self.mode == Mode::Walk {
            return;
        }
        let mut to = self.cam.destination();
        let step = 15f32.to_radians();
        match code {
            "Escape" => return self.zoom_out(),
            "Home" => return self.reset_view(),
            "ArrowLeft" => to.yaw -= step,
            "ArrowRight" => to.yaw += step,
            "ArrowUp" => to.pitch += step * 0.6,
            "ArrowDown" => to.pitch -= step * 0.6,
            "Equal" | "NumpadAdd" => to.dist *= 0.8,
            "Minus" | "NumpadSubtract" => to.dist *= 1.25,
            _ => return,
        }
        let ms = if self.reduced_motion { 0.0 } else { 260.0 };
        self.cam.fly_to(to, self.now, ms);
        self.dirty = true;
    }

    // ---- picking -----------------------------------------------------------------------

    fn on_focus_path(&self, idx: usize) -> bool {
        let mut cur = self.focus;
        while let Some(c) = cur {
            if c == idx {
                return true;
            }
            cur = self.layout.nodes[c].parent;
        }
        false
    }

    /// Whether the camera is close enough to an element that its parts should be clickable.
    fn zoomed_into(&self, idx: usize, eye: Vec3) -> bool {
        let p = &self.layout.nodes[idx].pick;
        eye.distance(p.center()) < p.radius() * 2.2 + 0.15
    }

    pub fn pick_ray(&self, o: Vec3, d: Vec3) -> Option<usize> {
        let eye = self.cam.eye();
        let nodes = &self.layout.nodes;
        let nearest = |cands: &[usize]| -> Option<usize> {
            let mut best: Option<(f32, usize)> = None;
            for &i in cands {
                let n = &nodes[i];
                if n.lod.vis(eye) < 0.5 {
                    continue;
                }
                // Hit the element's own body, or one of its (visible) parts poking out of it.
                if let Some(t) = n.pick.ray_hit(o, d) {
                    if best.is_none_or(|(bt, _)| t < bt) {
                        best = Some((t, i));
                    }
                }
            }
            best.map(|b| b.1)
        };
        let mut cur = nearest(&self.layout.roots)?;
        loop {
            if !(self.on_focus_path(cur) || self.zoomed_into(cur, eye)) {
                break;
            }
            match nearest(&nodes[cur].children) {
                Some(c) => cur = c,
                None => break,
            }
        }
        Some(cur)
    }

    pub fn pick(&self, x: f32, y: f32) -> Option<usize> {
        if !self.inside(Vec2::new(x, y)) {
            return None;
        }
        let (o, d) = self.cam.ray(x, y, self.css_w, self.css_h);
        self.pick_ray(o, d)
    }

    pub(crate) fn update_hover(&mut self) {
        if self.mode == Mode::Walk {
            return self.walk_update_hover();
        }
        let h = if self.drag.is_some_and(|d| d.moved) {
            None
        } else {
            self.pointer.and_then(|p| self.pick(p.x, p.y))
        };
        if h != self.hovered {
            self.hovered = h;
            self.dirty = true;
        }
    }

    pub fn hovered_id(&self) -> Option<String> {
        self.hovered.map(|i| self.layout.nodes[i].id.clone())
    }

    pub fn selected_id(&self) -> Option<String> {
        self.selected.map(|i| self.layout.nodes[i].id.clone())
    }

    // ---- per frame ---------------------------------------------------------------------

    /// Advance animations. Returns true when a redraw is needed.
    pub fn update(&mut self, now: f64) -> bool {
        let dt = self
            .last_now
            .map(|l| ((now - l) / 1000.0) as f32)
            .unwrap_or(0.0);
        self.last_now = Some(now);
        self.now = now;
        if self.mode == Mode::Walk {
            self.walk_update(now, dt);
        }
        if self.cam.update(now) {
            self.dirty = true;
            if self.pointer.is_some() {
                self.update_hover();
            }
        }
        if self.highlight_key != (self.hovered, self.selected) {
            self.highlight_key = (self.hovered, self.selected);
            self.rebuild_highlight();
        }
        let animated = self.layout.has_stale || (self.mode == Mode::Walk && self.layout.animated);
        let redraw = self.dirty || (animated && !self.reduced_motion);
        self.dirty = false;
        if redraw && self.mode == Mode::Walk {
            self.rebuild_dynamic();
        }
        redraw
    }

    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    fn rebuild_highlight(&mut self) {
        self.highlight.clear();
        self.highlight_version += 1;
        let a = Lod::ALWAYS;
        if let Some(h) = self.hovered.filter(|h| Some(*h) != self.selected) {
            let b = self.layout.nodes[h].body;
            self.highlight.box_edges(&b, rgba(AMBER, 0.7), 1.5, a, 0);
        }
        if let Some(s) = self.selected {
            let b = self.layout.nodes[s].body;
            let g = &mut self.highlight;
            g.box_edges(&b, rgba(AMBER, 0.95), 1.8, a, 0);
            // Corner brackets just outside the box.
            let bb = b.grow(b.size().min_element().max(0.004) * 0.12 + 0.002);
            let arm = (bb.size() * 0.22).min(Vec3::splat(0.12));
            for i in 0..8u8 {
                let c = Vec3::new(
                    if i & 1 != 0 { bb.max.x } else { bb.min.x },
                    if i & 2 != 0 { bb.max.y } else { bb.min.y },
                    if i & 4 != 0 { bb.max.z } else { bb.min.z },
                );
                let s = Vec3::new(
                    if i & 1 != 0 { -1.0 } else { 1.0 },
                    if i & 2 != 0 { -1.0 } else { 1.0 },
                    if i & 4 != 0 { -1.0 } else { 1.0 },
                );
                g.line(c, c + Vec3::X * s.x * arm.x, rgba(AMBER, 1.0), 2.2, a, 0);
                g.line(c, c + Vec3::Y * s.y * arm.y, rgba(AMBER, 1.0), 2.2, a, 0);
                g.line(c, c + Vec3::Z * s.z * arm.z, rgba(AMBER, 1.0), 2.2, a, 0);
            }
            // Footprint on the floor with drop lines.
            g.rect_y(
                b.min.x,
                b.min.z,
                b.max.x,
                b.max.z,
                0.002,
                rgba(AMBER, 0.55),
                1.0,
                a,
                DASH,
            );
            if b.min.y > 0.01 {
                for (x, z) in [(b.min.x, b.max.z), (b.max.x, b.max.z)] {
                    g.line(
                        Vec3::new(x, b.min.y, z),
                        Vec3::new(x, 0.002, z),
                        rgba(AMBER, 0.45),
                        0.9,
                        a,
                        DASH,
                    );
                }
            }
        }
    }

    // ---- labels ------------------------------------------------------------------------

    pub fn labels(&self) -> Vec<Label<'_>> {
        if self.mode == Mode::Walk {
            return self.labels_walk();
        }
        let vp = self.cam.view_proj();
        let eye = self.cam.eye();
        let px_per_m_at_w = self.css_h * 0.5 / (self.cam.fov() * 0.5).tan();
        let margin = 40.0;
        let mut out = Vec::new();
        for (i, n) in self.layout.nodes.iter().enumerate() {
            let clip = vp * n.label_at.extend(1.0);
            if clip.w <= 1e-4 {
                continue;
            }
            let ndc = clip.xyz() / clip.w;
            if !(-1.0..=1.0).contains(&ndc.z) {
                continue;
            }
            let x = (ndc.x * 0.5 + 0.5) * self.css_w;
            let y = (0.5 - ndc.y * 0.5) * self.css_h;
            if x < -margin || y < -margin || x > self.css_w + margin || y > self.css_h + margin {
                continue;
            }
            let px = n.label_size * px_per_m_at_w / clip.w;
            let vis = n.lod.vis(eye);
            let show = match n.kind {
                Kind::Host => true,
                Kind::Group => vis > 0.5 && px >= 13.0 && eye.distance(n.body.center()) < 7.0,
                Kind::Workload => vis > 0.6 && px >= 84.0,
                Kind::Image => vis > 0.6 && px >= 50.0,
                Kind::External => px >= 95.0 && !self.behind_host(eye, n.label_at),
            } || Some(i) == self.selected
                || (Some(i) == self.hovered && vis > 0.5);
            if !show {
                continue;
            }
            out.push(Label {
                id: &n.id,
                text: &n.text,
                sub: &n.sub,
                x: (x * 10.0).round() / 10.0,
                y: (y * 10.0).round() / 10.0,
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
        // Far to near, so later (nearer) labels stack on top in DOM order.
        out.sort_by(|a, b| b.depth.total_cmp(&a.depth));
        out
    }

    /// Whether a host cabinet stands between the eye and `p` (hides labels of things behind racks).
    pub(crate) fn behind_host(&self, eye: Vec3, p: Vec3) -> bool {
        let d = p - eye;
        let len = d.length();
        if len < 1e-4 {
            return false;
        }
        let dir = d / len;
        self.layout.hosts.iter().any(|&h| {
            self.layout.nodes[h]
                .body
                .ray_hit(eye, dir)
                .is_some_and(|t| t < len - 0.05)
        })
    }

    pub fn labels_json(&self) -> String {
        serde_json::to_string(&self.labels()).unwrap_or_else(|_| "[]".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn engine() -> Engine {
        let mut e = Engine::new();
        e.resize(1200.0, 800.0, 2.0);
        e.set_scene(include_str!("../fixtures/scene.json")).unwrap();
        e
    }

    fn settle(e: &mut Engine) {
        let t = e.now + 5000.0;
        e.update(t);
    }

    fn screen_of(e: &Engine, p: Vec3) -> Vec2 {
        let c = e.cam.view_proj() * p.extend(1.0);
        let n = c.xyz() / c.w;
        Vec2::new((n.x * 0.5 + 0.5) * e.css_w, (0.5 - n.y * 0.5) * e.css_h)
    }

    #[test]
    fn overview_frames_everything() {
        let e = engine();
        let b = e.layout.bounds.unwrap();
        for i in 0..8u8 {
            let c = Vec3::new(
                if i & 1 != 0 { b.max.x } else { b.min.x },
                if i & 2 != 0 { b.max.y } else { b.min.y },
                if i & 4 != 0 { b.max.z } else { b.min.z },
            );
            let s = screen_of(&e, c);
            assert!(
                s.x > -1.0 && s.x < e.css_w + 1.0 && s.y > -1.0 && s.y < e.css_h + 1.0,
                "corner {c} off screen at {s}"
            );
        }
        assert!(e.cam.eye().y > 0.0);
    }

    #[test]
    fn click_drills_down_and_escape_climbs_up() {
        let mut e = engine();
        let host = e.layout.hosts[0];
        let s = screen_of(&e, e.layout.nodes[host].body.center());
        // Overview: a click on the rack selects the host.
        e.pointer_down(s.x, s.y, 0);
        e.pointer_up(s.x, s.y, 0);
        assert_eq!(e.selected, Some(host));
        settle(&mut e);
        // Now a click on a group selects the group.
        let gi = e.layout.index["ns:slowbloom/kube-system"];
        let s = screen_of(
            &e,
            e.layout.nodes[gi].body.center() + Vec3::new(0.0, 0.0, 0.3),
        );
        e.pointer_down(s.x, s.y, 0);
        e.pointer_up(s.x, s.y, 0);
        assert_eq!(e.selected, Some(gi), "selected {:?}", e.selected_id());
        settle(&mut e);
        // Zoomed into the group: workloads are pickable.
        let wi = e.layout.nodes[gi].children[0];
        let s = screen_of(&e, e.layout.nodes[wi].body.max - Vec3::new(0.03, 0.03, 0.0));
        e.pointer_move(s.x, s.y);
        assert_eq!(e.hovered, Some(wi), "hovered {:?}", e.hovered_id());
        e.key("Escape");
        assert_eq!(e.selected, Some(host));
        e.key("Escape");
        assert_eq!(e.selected, None);
        assert_eq!(e.focus, None);
    }

    #[test]
    fn drag_orbits_without_selecting() {
        let mut e = engine();
        let yaw = e.cam.pose.yaw;
        e.pointer_down(600.0, 400.0, 0);
        e.pointer_move(660.0, 400.0);
        e.pointer_up(660.0, 400.0, 0);
        assert!(e.selected.is_none());
        assert!((e.cam.pose.yaw - yaw).abs() > 0.1);
        // Pitch never goes below the floor.
        e.pointer_down(600.0, 400.0, 0);
        e.pointer_move(600.0, -5000.0);
        e.pointer_up(600.0, -5000.0, 0);
        assert!(e.cam.eye().y > e.cam.pose.target.y);
    }

    #[test]
    fn wheel_zooms_towards_pointer() {
        let mut e = engine();
        let d0 = e.cam.pose.dist;
        let host = e.layout.hosts[0];
        let p = e.layout.nodes[host].body.center();
        let s = screen_of(&e, p);
        for _ in 0..5 {
            e.wheel(-200.0, s.x, s.y);
        }
        assert!(e.cam.pose.dist < d0 * 0.5);
        // The point under the cursor stays roughly under the cursor.
        let s2 = screen_of(&e, p);
        assert!(s.distance(s2) < 40.0, "{s} -> {s2}");
    }

    #[test]
    fn labels_depend_on_zoom() {
        let mut e = engine();
        let count = |e: &Engine, k: &str| e.labels().iter().filter(|l| l.kind == k).count();
        assert_eq!(count(&e, "host"), 1);
        assert_eq!(count(&e, "workload"), 0);
        assert_eq!(count(&e, "image"), 0);
        let gi = e.layout.index["ns:slowbloom/slowbloom"];
        e.focus_index(gi);
        settle(&mut e);
        assert!(count(&e, "workload") >= 4, "{}", e.labels_json());
        let wi = e.layout.nodes[gi].children[0];
        e.focus_index(wi);
        settle(&mut e);
        assert!(count(&e, "image") >= 1, "{}", e.labels_json());
        for l in e.labels() {
            assert!((0.0..=1.0).contains(&l.depth));
        }
        let v: serde_json::Value = serde_json::from_str(&e.labels_json()).unwrap();
        assert!(v.as_array().unwrap()[0].get("x").is_some());
    }

    #[test]
    fn rescene_keeps_camera_and_selection() {
        let mut e = engine();
        let gi = e.layout.index["ns:slowbloom/ghost"];
        e.focus_index(gi);
        settle(&mut e);
        let pose = e.cam.pose;
        e.set_scene(include_str!("../fixtures/scene.json")).unwrap();
        settle(&mut e);
        assert_eq!(e.cam.pose, pose);
        assert_eq!(e.selected_id().as_deref(), Some("ns:slowbloom/ghost"));
        // Garbage keeps the old scene.
        assert!(e.set_scene("not json").is_err());
        assert_eq!(e.selected_id().as_deref(), Some("ns:slowbloom/ghost"));
        // The focused element disappearing flies back out.
        e.set_scene(r#"{"hosts":[{"id":"x","name":"x","groups":[]}],"external":[]}"#)
            .unwrap();
        assert!(e.selected.is_none() && e.focus.is_none());
        settle(&mut e);
        assert_eq!(e.cam.pose, e.overview_pose());
    }

    #[test]
    fn empty_scene_is_fine() {
        let mut e = Engine::new();
        e.resize(0.0, 0.0, f32::NAN);
        e.set_scene("{}").unwrap();
        e.pointer_down(1.0, 1.0, 0);
        e.pointer_up(1.0, 1.0, 0);
        e.wheel(100.0, 1.0, 1.0);
        e.key("Escape");
        e.key("ArrowLeft");
        e.focus_id("nope");
        e.update(16.0);
        assert!(e.labels().is_empty());
    }
}
