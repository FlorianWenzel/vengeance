//! Turns a `Scene` into a floor plan of physical hardware: the node tree used for picking, LOD and
//! labels, plus the static line/face/LED instance buffers.
//!
//! Units are metres. The floor is y = 0, rack fronts face +z, hosts stand in rows along x.

use std::collections::HashMap;

use glam::Vec3;

use crate::geo::*;
use crate::scene::{Level, Scene, Status};

/// One rack unit (1U = 1.75 in).
pub const U: f32 = 0.04445;
pub const RACK_W: f32 = 0.60;
pub const RACK_D: f32 = 1.00;
pub const PANEL_W: f32 = 0.4826; // 19 in
pub const INNER_W: f32 = 0.450;
pub const PLINTH: f32 = 0.10;
pub const TOPCAP: f32 = 0.05;
pub const MODULE_D: f32 = 0.55;
pub const PER_ROW: usize = 4;
pub const BAY_MAX_U: u32 = 40;
const RACK_SIZES: [u32; 5] = [12, 18, 24, 32, 42];
const FRONT_INSET: f32 = 0.04;

/// LOD distances (eye to anchor, metres).
pub const GROUP_FAR: f32 = 18.0;
pub const WORKLOAD_FAR: f32 = 3.6;
pub const IMAGE_FAR: f32 = 1.25;
pub const DETAIL_FAR: f32 = 7.0;

pub const CYAN: [f32; 3] = [0.40, 0.84, 1.0];
pub const PALE: [f32; 3] = [0.80, 0.94, 1.0];
pub const DIM: [f32; 3] = [0.30, 0.58, 0.85];
pub const AMBER: [f32; 3] = [1.0, 0.72, 0.30];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Host,
    Group,
    Workload,
    Image,
    External,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Anchor {
    Above,
    Right,
    Inside,
    Below,
    Center,
}

impl Anchor {
    pub fn as_str(self) -> &'static str {
        match self {
            Anchor::Above => "above",
            Anchor::Right => "right",
            Anchor::Inside => "inside",
            Anchor::Below => "below",
            Anchor::Center => "center",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Node {
    pub id: String,
    pub kind: Kind,
    /// Label kind: host, group, workload, image, or the external kind.
    pub kind_name: String,
    pub depth: u8,
    pub parent: Option<usize>,
    pub children: Vec<usize>,
    /// The element's own box (highlight outline).
    pub body: Aabb,
    /// `body` ∪ children's pick boxes (ray tests descend through these).
    pub pick: Aabb,
    /// When the element is drawn / pickable.
    pub lod: Lod,
    pub label_at: Vec3,
    pub anchor: Anchor,
    /// World size used to decide whether the label is readable (projected height, metres).
    pub label_size: f32,
    /// Direction the element's front faces (horizontal unit vector).
    pub facing: Vec3,
    pub level: Level,
    pub stale: bool,
    pub text: String,
    pub sub: String,
}

#[derive(Default, Clone, Debug)]
pub struct Layout {
    pub nodes: Vec<Node>,
    pub index: HashMap<String, usize>,
    pub roots: Vec<usize>,
    pub hosts: Vec<usize>,
    pub bounds: Option<Aabb>,
    pub geo: Geo,
    pub has_stale: bool,
    /// Present in walk mode: the datacenter room.
    pub room: Option<crate::room::Room>,
}

pub(crate) fn st_flags(s: &Status) -> u32 {
    let mut f = 0;
    if s.stale {
        f |= BLINK;
    }
    if s.level == Level::P1 {
        f |= ALERT;
    }
    f
}

pub(crate) fn tint(level: Level, base: [f32; 3], a: f32) -> Rgba {
    match level {
        Level::P1 | Level::P2 => {
            let c = level.rgb();
            [base[0] * 0.4 + c[0] * 0.6, base[1] * 0.4 + c[1] * 0.6, base[2] * 0.4 + c[2] * 0.6, a * 1.8]
        }
        _ => rgba(base, a),
    }
}

fn group_units(n: usize) -> u32 {
    if n == 0 {
        1
    } else {
        2 * n.div_ceil(PER_ROW) as u32
    }
}

pub(crate) struct HostPlan {
    /// Group indices per bay.
    bays: Vec<Vec<usize>>,
    pub rack_u: u32,
    pub tower: bool,
    pub width: f32,
    pub depth: f32,
}

pub(crate) fn plan_host(h: &crate::scene::Host) -> HostPlan {
    if h.groups.is_empty() {
        return HostPlan { bays: vec![], rack_u: 0, tower: true, width: 0.26, depth: 0.58 };
    }
    let mut bays: Vec<Vec<usize>> = vec![vec![]];
    let mut used = vec![0u32];
    for (gi, g) in h.groups.iter().enumerate() {
        let u = group_units(g.workloads.len());
        let last = bays.len() - 1;
        if used[last] > 0 && used[last] + u > BAY_MAX_U {
            bays.push(vec![]);
            used.push(0);
        }
        let last = bays.len() - 1;
        bays[last].push(gi);
        used[last] += u;
    }
    let need = used.iter().copied().max().unwrap_or(0) + 2;
    let rack_u = RACK_SIZES.iter().copied().find(|&s| s >= need).unwrap_or(need);
    HostPlan { width: RACK_W * bays.len() as f32, depth: RACK_D, bays, rack_u, tower: false }
}

pub fn rack_height(rack_u: u32) -> f32 {
    PLINTH + rack_u as f32 * U + TOPCAP
}

pub(crate) struct Builder<'a> {
    pub l: &'a mut Layout,
    /// Walk mode: no floor dimensioning, hanging host signs instead.
    pub walk: bool,
}

impl Builder<'_> {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn add(
        &mut self,
        id: &str,
        kind: Kind,
        kind_name: &str,
        parent: Option<usize>,
        body: Aabb,
        lod: Lod,
        label_at: Vec3,
        anchor: Anchor,
        label_size: f32,
        status: &Status,
        text: String,
        sub: String,
    ) -> usize {
        let idx = self.l.nodes.len();
        let g = &self.l.geo;
        let (body, label_at, lod) = (g.tb(&body), g.t(label_at), g.tl(lod));
        let facing = if g.flip.is_some() { Vec3::NEG_Z } else { Vec3::Z };
        let depth = parent.map(|p| self.l.nodes[p].depth + 1).unwrap_or(0);
        // Ids must be unique for lookup; a duplicate keeps the first mapping.
        let id = if id.is_empty() { format!("anon:{idx}") } else { id.to_string() };
        self.l.index.entry(id.clone()).or_insert(idx);
        self.l.has_stale |= status.stale;
        self.l.nodes.push(Node {
            id,
            kind,
            kind_name: kind_name.to_string(),
            depth,
            parent,
            children: vec![],
            body,
            pick: body,
            lod,
            label_at,
            anchor,
            label_size,
            facing,
            level: status.level,
            stale: status.stale,
            text,
            sub,
        });
        match parent {
            Some(p) => self.l.nodes[p].children.push(idx),
            None => self.l.roots.push(idx),
        }
        idx
    }

    pub(crate) fn grow_pick(&mut self, idx: usize) {
        let mut cur = idx;
        while let Some(p) = self.l.nodes[cur].parent {
            let pk = self.l.nodes[cur].pick;
            let pp = self.l.nodes[p].pick.union(&pk);
            self.l.nodes[p].pick = pp;
            cur = p;
        }
    }
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

pub fn build(scene: &Scene) -> Layout {
    let mut layout = Layout::default();
    let plans: Vec<HostPlan> = scene.hosts.iter().map(plan_host).collect();

    // Host grid: one row up to 4 hosts, otherwise roughly twice as wide as deep.
    let n = plans.len();
    let cols = if n <= 4 { n.max(1) } else { ((n as f32 * 2.0).sqrt().ceil() as usize).clamp(1, n) };
    let gap_x = 1.15;
    let gap_z = 1.9;
    let rows: Vec<&[HostPlan]> = plans.chunks(cols).collect();
    let row_w: Vec<f32> = rows.iter().map(|r| r.iter().map(|p| p.width).sum::<f32>() + gap_x * (r.len().max(1) - 1) as f32).collect();
    let max_w = row_w.iter().copied().fold(0.0, f32::max);
    let mut placements = Vec::with_capacity(n); // (x0, zc)
    for (ri, r) in rows.iter().enumerate() {
        let mut x = -row_w[ri] * 0.5;
        let zc = -(ri as f32) * (RACK_D + gap_z);
        for p in r.iter() {
            placements.push((x, zc));
            x += p.width + gap_x;
        }
    }
    let _ = max_w;

    {
        let mut b = Builder { l: &mut layout, walk: false };
        for (hi, h) in scene.hosts.iter().enumerate() {
            let (x0, zc) = placements[hi];
            build_host(&mut b, h, &plans[hi], x0, zc);
        }
    }

    let host_bounds = layout.roots.iter().map(|&i| layout.nodes[i].pick).reduce(|a, b| a.union(&b));
    layout.hosts = layout.roots.clone();
    let hb = host_bounds.unwrap_or(Aabb::new(Vec3::new(-0.5, 0.0, -0.5), Vec3::new(0.5, 1.0, 0.5)));
    {
        let mut b = Builder { l: &mut layout, walk: false };
        build_externals(&mut b, scene, &hb);
    }

    layout.bounds = layout.roots.iter().map(|&i| layout.nodes[i].pick).reduce(|a, b| a.union(&b));
    if let Some(bb) = layout.bounds {
        floor_marks(&mut layout.geo, &bb);
    }
    layout
}

fn floor_marks(g: &mut Geo, bb: &Aabb) {
    // A dashed site boundary around everything, with corner registration crosses.
    let m = 0.9;
    let (x0, z0, x1, z1) = (bb.min.x - m, bb.min.z - m, bb.max.x + m, bb.max.z + m);
    let c = rgba(DIM, 0.35);
    g.rect_y(x0, z0, x1, z1, 0.001, c, 0.8, Lod::ALWAYS, DASH | NO_HIDDEN);
    for (x, z) in [(x0, z0), (x1, z0), (x1, z1), (x0, z1)] {
        g.line(Vec3::new(x - 0.12, 0.001, z), Vec3::new(x + 0.12, 0.001, z), rgba(CYAN, 0.6), 1.0, Lod::ALWAYS, NO_HIDDEN);
        g.line(Vec3::new(x, 0.001, z - 0.12), Vec3::new(x, 0.001, z + 0.12), rgba(CYAN, 0.6), 1.0, Lod::ALWAYS, NO_HIDDEN);
    }
}

pub(crate) fn build_host(b: &mut Builder, h: &crate::scene::Host, plan: &HostPlan, x0: f32, zc: f32) {
    let name = if h.name.is_empty() { h.id.clone() } else { h.name.clone() };
    let zf = zc + plan.depth * 0.5;
    let zb = zc - plan.depth * 0.5;
    let x1 = x0 + plan.width;
    let height = if plan.tower { 0.50 } else { rack_height(plan.rack_u) };
    let body = Aabb::new(Vec3::new(x0, 0.0, zb), Vec3::new(x1, height, zf));
    let walk = b.walk;
    let sign_y = height.max(1.6) + 0.30;
    let (label_at, anchor) = if walk {
        (Vec3::new((x0 + x1) * 0.5, sign_y, zf + 0.06), Anchor::Center)
    } else {
        (Vec3::new((x0 + x1) * 0.5, height + 0.06, zf), Anchor::Above)
    };
    let hi = b.add(&h.id, Kind::Host, "host", None, body, Lod::ALWAYS, label_at, anchor, 0.0, &h.status, name, h.subtitle.clone());
    let g = &mut b.l.geo;
    let a = Lod::ALWAYS;
    let det = Lod::far(body.center(), DETAIL_FAR);
    let hf = st_flags(&h.status);
    if walk {
        // Hanging sign above the rack front, suspended from the cable tray.
        let cx = (x0 + x1) * 0.5;
        let sw = (plan.width * 0.5).clamp(0.26, 0.5);
        let sign = Aabb::new(Vec3::new(cx - sw, sign_y - 0.09, zf + 0.02), Vec3::new(cx + sw, sign_y + 0.09, zf + 0.05));
        g.box_edges(&sign, rgba(PALE, 0.9), 1.2, a, 0);
        g.face(&sign, rgba(CYAN, 0.10), a, OCCLUDER);
        g.rect_z(sign.min.x + 0.015, sign.min.y + 0.015, sign.max.x - 0.015, sign.max.y - 0.015, sign.max.z + 0.0005, rgba(CYAN, 0.45), 0.7, a, 0);
        for x in [sign.min.x + 0.04, sign.max.x - 0.04] {
            g.line(Vec3::new(x, sign.max.y, zf + 0.035), Vec3::new(x, crate::room::TRAY_Y, zf + 0.035), rgba(DIM, 0.6), 0.8, a, 0);
        }
        g.led(Vec3::new(sign.min.x + 0.035, sign_y, sign.max.z + 0.003), 0.014, h.status.level.rgb(), a, hf, 3.0);
    }

    // Floor footprint, front service zone and dimension lines (drawn the same for racks and towers).
    let dc = rgba(DIM, 0.75);
    let hx = x0 - 0.14;
    if !walk {
    g.rect_y(x0 - 0.05, zb - 0.05, x1 + 0.05, zf + 0.05, 0.001, rgba(DIM, 0.45), 0.8, a, DASH | NO_HIDDEN);
    g.rect_y(x0, zf + 0.05, x1, zf + 0.75, 0.001, rgba(DIM, 0.22), 0.8, det, DASH | NO_HIDDEN);
    // Width (front, on the floor).
    let wz = zf + 0.2;
    g.line(Vec3::new(x0, 0.001, zf + 0.02), Vec3::new(x0, 0.001, wz + 0.04), dc, 0.7, det, NO_HIDDEN);
    g.line(Vec3::new(x1, 0.001, zf + 0.02), Vec3::new(x1, 0.001, wz + 0.04), dc, 0.7, det, NO_HIDDEN);
    g.dimension(Vec3::new(x0, 0.001, wz), Vec3::new(x1, 0.001, wz), Vec3::Z, dc, det);
    // Depth (right side, on the floor).
    let dx = x1 + 0.2;
    g.line(Vec3::new(x1 + 0.02, 0.001, zf), Vec3::new(dx + 0.04, 0.001, zf), dc, 0.7, det, NO_HIDDEN);
    g.line(Vec3::new(x1 + 0.02, 0.001, zb), Vec3::new(dx + 0.04, 0.001, zb), dc, 0.7, det, NO_HIDDEN);
    g.dimension(Vec3::new(dx, 0.001, zf), Vec3::new(dx, 0.001, zb), Vec3::X, dc, det);
    // Height (left of the front face).
    g.line(Vec3::new(x0 - 0.02, 0.0, zf), Vec3::new(hx - 0.04, 0.0, zf), dc, 0.7, det, NO_HIDDEN);
    g.line(Vec3::new(x0 - 0.02, height, zf), Vec3::new(hx - 0.04, height, zf), dc, 0.7, det, NO_HIDDEN);
    g.dimension(Vec3::new(hx, 0.0, zf), Vec3::new(hx, height, zf), Vec3::X, dc, det);
    }

    let frame = rgba(CYAN, 0.95);
    if plan.tower {
        g.box_edges(&body, frame, 1.4, a, 0);
        g.face(&body, tint(h.status.level, CYAN, 0.06), a, OCCLUDER);
        // Bezel: drive bays and a vent grille.
        let zfr = zf + 0.0005;
        for i in 0..3 {
            let y = 0.40 - i as f32 * 0.045;
            g.rect_z(x0 + 0.03, y - 0.03, x1 - 0.03, y, zfr, rgba(PALE, 0.7), 0.8, det, 0);
        }
        for i in 0..6 {
            let y = 0.06 + i as f32 * 0.025;
            g.line(Vec3::new(x0 + 0.05, y, zfr), Vec3::new(x1 - 0.05, y, zfr), rgba(DIM, 0.6), 0.7, det, 0);
        }
        g.led(Vec3::new(x1 - 0.04, 0.46, zf + 0.004), 0.02, h.status.level.rgb(), a, hf, 4.5);
        return;
    }

    let rack_u = plan.rack_u;
    let top = height - TOPCAP;
    g.box_edges(&body, frame, 1.4, a, 0);
    g.face(&body, rgba(CYAN, 0.022), a, 0);
    g.face(&Aabb::new(Vec3::new(x0, 0.0, zb), Vec3::new(x1, PLINTH, zf)), rgba(CYAN, 0.07), a, OCCLUDER);
    g.face(&Aabb::new(Vec3::new(x0, top, zb), Vec3::new(x1, height, zf)), rgba(CYAN, 0.05), a, OCCLUDER);
    g.face(&Aabb::new(Vec3::new(x0, PLINTH, zb), Vec3::new(x1, top, zb + 0.01)), rgba(CYAN, 0.03), a, OCCLUDER);
    // Plinth and top-cap seams all the way round.
    g.rect_y(x0, zb, x1, zf, PLINTH, rgba(CYAN, 0.6), 1.0, a, 0);
    g.rect_y(x0, zb, x1, zf, top, rgba(CYAN, 0.6), 1.0, a, 0);
    // Plinth vent slots.
    for i in 0..5 {
        let x = x0 + 0.08 + i as f32 * ((plan.width - 0.16) / 4.0);
        g.line(Vec3::new(x, 0.025, zf), Vec3::new(x, PLINTH - 0.025, zf), rgba(DIM, 0.7), 0.8, det, 0);
    }
    // Host status lamp in the top cap.
    let cx = (x0 + x1) * 0.5;
    g.rect_z(cx - 0.08, top + 0.012, cx + 0.08, height - 0.012, zf + 0.0005, rgba(PALE, 0.6), 0.8, det, 0);
    g.led(Vec3::new(cx - 0.055, top + TOPCAP * 0.5, zf + 0.004), 0.016, h.status.level.rgb(), a, hf, 5.0);

    let zr = zf - FRONT_INSET + 0.01; // mounting rail plane
    let tick_lod = Lod::far(body.center(), 4.5);
    for (bi, bay) in plan.bays.iter().enumerate() {
        let bx0 = x0 + bi as f32 * RACK_W;
        let bcx = bx0 + RACK_W * 0.5;
        let (rl, rr) = (bcx - PANEL_W * 0.5, bcx + PANEL_W * 0.5);
        if bi > 0 {
            for z in [zf, zb] {
                b.l.geo.line(Vec3::new(bx0, 0.0, z), Vec3::new(bx0, height, z), frame, 1.2, a, 0);
            }
            b.l.geo.line(Vec3::new(bx0, height, zb), Vec3::new(bx0, height, zf), frame, 1.2, a, 0);
        }
        // Front and rear mounting rails with U ticks (every 5U longer).
        let g = &mut b.l.geo;
        for x in [rl, rr] {
            g.line(Vec3::new(x, PLINTH, zr), Vec3::new(x, top, zr), rgba(CYAN, 0.55), 1.0, a, 0);
            g.line(Vec3::new(x, PLINTH, zb + 0.04), Vec3::new(x, top, zb + 0.04), rgba(DIM, 0.35), 0.8, a, 0);
        }
        for u in 0..=rack_u {
            let y = PLINTH + u as f32 * U;
            let len = if u % 5 == 0 { 0.018 } else { 0.008 };
            g.line(Vec3::new(rl - len, y, zr), Vec3::new(rl, y, zr), rgba(DIM, 0.8), 0.7, tick_lod, NO_HIDDEN);
            g.line(Vec3::new(rr, y, zr), Vec3::new(rr + len, y, zr), rgba(DIM, 0.8), 0.7, tick_lod, NO_HIDDEN);
        }

        // Groups top-down, leaving 1U of air under the top cap.
        let mut y_top = top - U;
        let mut filled_u = 0u32;
        for &gi in bay {
            let grp = &h.groups[gi];
            let gu = group_units(grp.workloads.len());
            let y0 = y_top - gu as f32 * U;
            filled_u += gu;
            build_group(b, hi, grp, bcx, y0, y_top, zf);
            y_top = y0;
        }
        // Blank panels for the unused space.
        let g = &mut b.l.geo;
        let free_u = rack_u.saturating_sub(filled_u + 1);
        if free_u > 0 {
            let y1 = y_top;
            let y0 = PLINTH;
            let zp = zf - FRONT_INSET;
            g.rect_z(rl + 0.004, y0 + 0.002, rr - 0.004, y1 - 0.002, zp, rgba(DIM, 0.45), 0.8, a, DASH);
            let mut y = y0 + U;
            while y < y1 - 0.01 {
                g.line(Vec3::new(rl + 0.03, y, zp), Vec3::new(rr - 0.03, y, zp), rgba(DIM, 0.18), 0.7, det, 0);
                y += U;
            }
        }
        // Group boundary ticks on the height dimension.
        let _ = filled_u;
    }
    let tick_c = rgba(DIM, 0.75);
    let children = if walk { vec![] } else { b.l.nodes[hi].children.clone() };
    for c in children {
        let y = b.l.nodes[c].body.max.y;
        b.l.geo.line(Vec3::new(hx - 0.015, y, zf), Vec3::new(hx + 0.015, y, zf), tick_c, 0.7, det, NO_HIDDEN);
    }
}

fn build_group(b: &mut Builder, hi: usize, grp: &crate::scene::Group, bcx: f32, y0: f32, y1: f32, zf: f32) {
    let zp = zf - FRONT_INSET; // panel front plane
    let (pl, pr) = (bcx - PANEL_W * 0.5, bcx + PANEL_W * 0.5);
    let body = Aabb::new(Vec3::new(pl, y0, zp - MODULE_D - 0.02), Vec3::new(pr, y1, zp + 0.004));
    let center = body.center();
    let lod_slab = Lod::far(center, GROUP_FAR);
    let name = if grp.name.is_empty() { grp.id.clone() } else { grp.name.clone() };
    let sub = plural(grp.workloads.len(), "workload", "workloads");
    let gi = b.add(
        &grp.id,
        Kind::Group,
        "group",
        Some(hi),
        body,
        lod_slab,
        Vec3::new(bcx + RACK_W * 0.5 + 0.03, (y0 + y1) * 0.5, zf),
        Anchor::Right,
        y1 - y0,
        &grp.status,
        name,
        sub,
    );
    b.grow_pick(gi);
    let gf = st_flags(&grp.status);
    let g = &mut b.l.geo;
    let wl = Lod::far(center, WORKLOAD_FAR);
    // Front panel outline (always readable as a rack unit).
    g.rect_z(pl + 0.002, y0 + 0.0015, pr - 0.002, y1 - 0.0015, zp + 0.004, rgba(CYAN, 0.75), 1.0, lod_slab, 0);
    // Coarse stand-in: a solid slab while modules are hidden; a tray once they appear.
    g.face(&body, tint(grp.status.level, CYAN, 0.06), Lod { anchor: center, near: WORKLOAD_FAR, far: 0.0 }, OCCLUDER | gf & BLINK);
    g.box_edges(&body, rgba(DIM, 0.3), 0.7, wl, 0);
    g.face(&Aabb::new(Vec3::new(pl, y0, body.min.z), Vec3::new(pr, y0 + 0.003, zp)), rgba(CYAN, 0.08), wl, 0);
    // Rack ears with the group status LED.
    for (ex0, ex1) in [(pl, pl + 0.016), (pr - 0.016, pr)] {
        g.rect_z(ex0, y0 + 0.003, ex1, y1 - 0.003, zp + 0.005, rgba(DIM, 0.5), 0.7, Lod::far(center, DETAIL_FAR), 0);
    }
    g.led(Vec3::new(pl + 0.008, (y0 + y1) * 0.5, zp + 0.008), 0.0065, grp.status.level.rgb(), Lod::ALWAYS, gf, 3.5);

    let n = grp.workloads.len();
    if n == 0 {
        return;
    }
    let gap = 0.006;
    let mw = (INNER_W - gap * (PER_ROW - 1) as f32) / PER_ROW as f32;
    let mh = 2.0 * U - 0.006;
    let left = bcx - INNER_W * 0.5;
    for (wi, w) in grp.workloads.iter().enumerate() {
        let row = wi / PER_ROW;
        let col = wi % PER_ROW;
        let x = left + col as f32 * (mw + gap);
        let ytop = y1 - row as f32 * 2.0 * U - 0.003;
        build_workload(b, gi, w, x, ytop - mh, mw, mh, zp, center);
    }
}

#[allow(clippy::too_many_arguments)]
fn build_workload(b: &mut Builder, gi: usize, w: &crate::scene::Workload, x: f32, y: f32, mw: f32, mh: f32, zp: f32, gcenter: Vec3) {
    let zm = zp - 0.002; // module front
    let body = Aabb::new(Vec3::new(x, y, zm - MODULE_D), Vec3::new(x + mw, y + mh, zm));
    let lod = Lod::far(gcenter, WORKLOAD_FAR);
    let ready = w.ready.max(0.0).round() as u32;
    let desired = w.desired.max(0.0).round() as u32;
    let degraded = ready < desired;
    let kind = if w.kind.is_empty() { "Workload".to_string() } else { w.kind.clone() };
    let sub = format!("{kind} · {ready}/{desired} ready");
    let name = if w.name.is_empty() { w.id.clone() } else { w.name.clone() };
    let wi = b.add(
        &w.id,
        Kind::Workload,
        "workload",
        Some(gi),
        body,
        lod,
        Vec3::new(x + 0.02, y + mh - 0.004, zm),
        Anchor::Inside,
        mw,
        &w.status,
        name,
        sub,
    );
    b.grow_pick(wi);
    let wf = st_flags(&w.status);
    let g = &mut b.l.geo;
    let scaled_down = desired == 0;
    let edge_a = if scaled_down { 0.45 } else { 0.85 };
    g.box_edges(&body, rgba(PALE, edge_a), 1.0, lod, if degraded || scaled_down { DASH } else { 0 });
    g.face(&body, tint(w.status.level, CYAN, if scaled_down { 0.02 } else { 0.05 }), lod, OCCLUDER);
    if degraded {
        g.face(&Aabb::new(Vec3::new(x, y, zm - 0.001), Vec3::new(x + mw, y + mh, zm + 0.0005)), rgba(Level::P2.rgb(), 1.0), lod, HATCH);
    }
    let det = Lod::far(gcenter, WORKLOAD_FAR * 0.75);
    let zf = zm + 0.0006;
    // Handle and vents on the right edge.
    g.line(Vec3::new(x + mw - 0.006, y + 0.008, zf), Vec3::new(x + mw - 0.006, y + mh - 0.008, zf), rgba(PALE, 0.5), 0.8, det, 0);
    for i in 0..3 {
        let yy = y + mh * 0.5 + (i as f32 - 1.0) * 0.006;
        g.line(Vec3::new(x + mw - 0.022, yy, zf), Vec3::new(x + mw - 0.012, yy, zf), rgba(DIM, 0.7), 0.7, det, 0);
    }
    g.led(Vec3::new(x + 0.009, y + mh - 0.009, zm + 0.003), 0.0055, w.status.level.rgb(), lod, wf, 2.4);

    // Replica bar: one cell per desired replica (max 8); missing replicas are amber outlines.
    let cells = desired.clamp(1, 8);
    let (bx0, bx1, by0, by1) = (x + 0.006, x + mw - 0.03, y + 0.005, y + 0.011);
    let cw = (bx1 - bx0 - 0.0015 * (cells - 1) as f32) / cells as f32;
    for i in 0..cells {
        let cx0 = bx0 + i as f32 * (cw + 0.0015);
        let filled = !scaled_down && (i < ready || (desired > 8 && ready * 8 / desired > i));
        let cbox = Aabb::new(Vec3::new(cx0, by0, zm), Vec3::new(cx0 + cw, by1, zm + 0.0015));
        if filled {
            g.face(&cbox, rgba(CYAN, 0.32), det, 0);
        } else {
            let c = if scaled_down { rgba(DIM, 0.6) } else { rgba(Level::P2.rgb(), 0.9) };
            g.rect_z(cbox.min.x, cbox.min.y, cbox.max.x, cbox.max.y, zm + 0.0015, c, 0.7, det, 0);
        }
    }

    // Image chips in the middle band.
    let n = w.images.len();
    if n == 0 {
        return;
    }
    let (ax0, ax1, ay0, ay1) = (x + 0.022, x + mw - 0.03, y + 0.016, y + mh - 0.006);
    let (aw, ah) = (ax1 - ax0, ay1 - ay0);
    let mut cs = 0.017f32;
    let sp = 0.005f32;
    let (cols, rows) = loop {
        let cols = (((aw + sp) / (cs + sp)).floor() as usize).max(1);
        let rows = n.div_ceil(cols);
        if rows as f32 * (cs + sp) - sp <= ah || cs < 0.004 {
            break (cols, rows);
        }
        cs *= 0.85;
    };
    let total_h = rows as f32 * (cs + sp) - sp;
    let wcenter = body.center();
    for (ii, im) in w.images.iter().enumerate() {
        let r = ii / cols;
        let c = ii % cols;
        let cx0 = ax0 + c as f32 * (cs + sp);
        let cy1 = ay0 + (ah + total_h) * 0.5 - r as f32 * (cs + sp);
        build_image(b, wi, im, cx0, cy1 - cs, cs, zm, wcenter);
    }
}

#[allow(clippy::too_many_arguments)]
fn build_image(b: &mut Builder, wi: usize, im: &crate::scene::Image, x: f32, y: f32, cs: f32, zm: f32, wcenter: Vec3) {
    let body = Aabb::new(Vec3::new(x, y, zm), Vec3::new(x + cs, y + cs, zm + 0.004));
    let lod = Lod::far(wcenter, IMAGE_FAR);
    let name = if !im.name.is_empty() {
        im.name.clone()
    } else if !im.reference.is_empty() {
        im.reference.clone()
    } else {
        im.id.clone()
    };
    let ii = b.add(&im.id, Kind::Image, "image", Some(wi), body, lod, Vec3::new(x + cs * 0.5, y, zm + 0.004), Anchor::Below, cs, &im.status, name, im.reference.clone());
    b.grow_pick(ii);
    let f = st_flags(&im.status);
    let g = &mut b.l.geo;
    g.box_edges(&body, rgba(CYAN, 0.95), 0.9, lod, 0);
    g.face(&body, tint(im.status.level, CYAN, 0.10), lod, OCCLUDER);
    let zf = body.max.z + 0.0003;
    let inset = cs * 0.22;
    g.rect_z(x + inset, y + inset, x + cs - inset, y + cs - inset, zf, rgba(DIM, 0.9), 0.6, lod, 0);
    // Pins.
    for i in 0..4 {
        let px = x + cs * (0.2 + 0.2 * i as f32);
        g.line(Vec3::new(px, y + cs, zf - 0.002), Vec3::new(px, y + cs + cs * 0.12, zf - 0.002), rgba(DIM, 0.9), 0.6, lod, 0);
        g.line(Vec3::new(px, y, zf - 0.002), Vec3::new(px, y - cs * 0.12, zf - 0.002), rgba(DIM, 0.9), 0.6, lod, 0);
    }
    g.led(Vec3::new(x + cs * 0.5, y + cs * 0.5, zf + 0.0005), cs * 0.2, im.status.level.rgb(), lod, f, 1.8);
}

fn build_externals(b: &mut Builder, scene: &Scene, hb: &Aabb) {
    let mut repos = vec![];
    let mut domains = vec![];
    let mut others = vec![];
    for e in &scene.external {
        match e.kind.as_str() {
            "repository" => repos.push(e),
            "domain" => domains.push(e),
            _ => others.push(e),
        }
    }
    let zc = (hb.min.z + hb.max.z) * 0.5;

    // Repositories: document stacks on drafting tables left of the racks (12 per table).
    let per_table = 12usize;
    let (dw, dd) = (0.15f32, 0.20f32);
    let (cols, pad) = (4usize, 0.05f32);
    for (ti, chunk) in repos.chunks(per_table).enumerate() {
        let trows = chunk.len().div_ceil(cols).max(1);
        let tw = cols as f32 * (dw + pad) + pad;
        let td = trows as f32 * (dd + pad) + pad;
        let tx1 = hb.min.x - 1.0;
        let tx0 = tx1 - tw;
        let tz1 = zc + 0.6 - ti as f32 * 1.3 + 0.0;
        let tz0 = tz1 - td;
        let th = 0.74;
        let top = Aabb::new(Vec3::new(tx0, th - 0.03, tz0), Vec3::new(tx1, th, tz1));
        let g = &mut b.l.geo;
        g.box_edges(&top, rgba(CYAN, 0.8), 1.1, Lod::ALWAYS, 0);
        g.face(&top, rgba(CYAN, 0.06), Lod::ALWAYS, OCCLUDER);
        for (lx, lz) in [(tx0 + 0.04, tz0 + 0.04), (tx1 - 0.04, tz0 + 0.04), (tx1 - 0.04, tz1 - 0.04), (tx0 + 0.04, tz1 - 0.04)] {
            g.line(Vec3::new(lx, 0.0, lz), Vec3::new(lx, th - 0.03, lz), rgba(CYAN, 0.7), 1.0, Lod::ALWAYS, 0);
        }
        g.line(Vec3::new(tx0 + 0.04, 0.12, tz0 + 0.04), Vec3::new(tx1 - 0.04, 0.12, tz0 + 0.04), rgba(DIM, 0.5), 0.8, Lod::ALWAYS, 0);
        g.line(Vec3::new(tx0 + 0.04, 0.12, tz1 - 0.04), Vec3::new(tx1 - 0.04, 0.12, tz1 - 0.04), rgba(DIM, 0.5), 0.8, Lod::ALWAYS, 0);
        g.rect_y(tx0 - 0.05, tz0 - 0.05, tx1 + 0.05, tz1 + 0.05, 0.001, rgba(DIM, 0.4), 0.8, Lod::ALWAYS, DASH | NO_HIDDEN);
        for (i, e) in chunk.iter().enumerate() {
            let (r, c) = (i / cols, i % cols);
            let x = tx0 + pad + c as f32 * (dw + pad);
            let z = tz1 - pad - dd - r as f32 * (dd + pad);
            let body = Aabb::new(Vec3::new(x, th, z), Vec3::new(x + dw, th + 0.024, z + dd));
            let name = if e.name.is_empty() { e.id.clone() } else { e.name.clone() };
            let idx = b.add(&e.id, Kind::External, "repository", None, body, Lod::ALWAYS, Vec3::new(x + dw * 0.5, th + 0.03, z + dd), Anchor::Above, dw + pad, &e.status, name, "repository".into());
            let _ = idx;
            let g = &mut b.l.geo;
            let lod = Lod::far(body.center(), DETAIL_FAR);
            for s in 0..3 {
                let o = s as f32 * 0.006;
                let sheet = Aabb::new(Vec3::new(x + o, th + s as f32 * 0.008, z - o), Vec3::new(x + dw + o - 0.012, th + s as f32 * 0.008 + 0.004, z + dd - o));
                g.box_edges(&sheet, rgba(PALE, if s == 2 { 0.9 } else { 0.5 }), 0.8, Lod::ALWAYS, 0);
                g.face(&sheet, rgba(CYAN, 0.07), Lod::ALWAYS, OCCLUDER);
            }
            // Text lines and a title block on the top sheet.
            let ty = th + 0.0205;
            let (sx0, sz0, sx1, sz1) = (x + 0.012, z - 0.012, x + dw, z + dd - 0.012);
            for k in 0..5 {
                let zz = sz0 + 0.03 + k as f32 * 0.022;
                let len = if k % 2 == 0 { 0.7 } else { 0.5 };
                g.line(Vec3::new(sx0 + 0.015, ty, zz), Vec3::new(sx0 + 0.015 + (sx1 - sx0 - 0.03) * len, ty, zz), rgba(DIM, 0.8), 0.6, lod, 0);
            }
            g.rect_y(sx1 - 0.06, sz1 - 0.035, sx1 - 0.008, sz1 - 0.008, ty, rgba(DIM, 0.8), 0.6, lod, 0);
            g.led(Vec3::new(sx0 + 0.016, ty + 0.002, sz1 - 0.018), 0.01, e.status.level.rgb(), Lod::ALWAYS, st_flags(&e.status), 3.0);
        }
    }

    // Domains: antenna masts right of the racks, in columns of four.
    for (i, e) in domains.iter().enumerate() {
        let (c, r) = (i / 4, i % 4);
        let x = hb.max.x + 1.0 + c as f32 * 0.7;
        let z = zc + 0.9 - r as f32 * 0.7;
        let hgt = 1.55 + ((i * 7) % 3) as f32 * 0.12;
        let body = Aabb::new(Vec3::new(x - 0.12, 0.0, z - 0.12), Vec3::new(x + 0.12, hgt + 0.04, z + 0.12));
        let name = if e.name.is_empty() { e.id.clone() } else { e.name.clone() };
        b.add(&e.id, Kind::External, "domain", None, body, Lod::ALWAYS, Vec3::new(x, hgt + 0.08, z), Anchor::Above, 0.7, &e.status, name, "domain".into());
        let g = &mut b.l.geo;
        let a = Lod::ALWAYS;
        let base = Aabb::new(Vec3::new(x - 0.09, 0.0, z - 0.09), Vec3::new(x + 0.09, 0.025, z + 0.09));
        g.box_edges(&base, rgba(CYAN, 0.8), 1.0, a, 0);
        g.face(&base, rgba(CYAN, 0.08), a, OCCLUDER);
        // Lattice mast: two rails with zig-zag bracing.
        let (r0, r1) = (Vec3::new(x - 0.02, 0.025, z), Vec3::new(x + 0.02, 0.025, z));
        g.line(r0, r0 + Vec3::Y * (hgt - 0.025), rgba(CYAN, 0.9), 1.1, a, 0);
        g.line(r1, r1 + Vec3::Y * (hgt - 0.025), rgba(CYAN, 0.9), 1.1, a, 0);
        let det = Lod::far(body.center(), DETAIL_FAR);
        let steps = 12;
        for s in 0..steps {
            let y0 = 0.025 + (hgt - 0.025) * s as f32 / steps as f32;
            let y1 = 0.025 + (hgt - 0.025) * (s + 1) as f32 / steps as f32;
            let (xa, xb) = if s % 2 == 0 { (x - 0.02, x + 0.02) } else { (x + 0.02, x - 0.02) };
            g.line(Vec3::new(xa, y0, z), Vec3::new(xb, y1, z), rgba(DIM, 0.6), 0.7, det, 0);
        }
        // Dipoles and a ring at the top.
        for (k, yy) in [hgt - 0.30, hgt - 0.15].iter().enumerate() {
            let l = 0.16 - k as f32 * 0.04;
            g.line(Vec3::new(x - l, *yy, z), Vec3::new(x + l, *yy, z), rgba(PALE, 0.85), 1.0, a, 0);
            g.line(Vec3::new(x, *yy, z - l), Vec3::new(x, *yy, z + l), rgba(PALE, 0.85), 1.0, a, 0);
        }
        let ring: Vec<Vec3> = (0..12)
            .map(|k| {
                let t = k as f32 / 12.0 * std::f32::consts::TAU;
                Vec3::new(x + t.cos() * 0.07, hgt - 0.04, z + t.sin() * 0.07)
            })
            .collect();
        g.polyline(&ring, true, rgba(CYAN, 0.8), 0.9, a, 0);
        // Guy wires.
        for k in 0..3 {
            let t = k as f32 / 3.0 * std::f32::consts::TAU + 0.5;
            let foot = Vec3::new(x + t.cos() * 0.45, 0.0, z + t.sin() * 0.45);
            g.line(Vec3::new(x, hgt * 0.62, z), foot, rgba(DIM, 0.45), 0.7, det, DASH);
        }
        g.led(Vec3::new(x, hgt + 0.015, z), 0.022, e.status.level.rgb(), a, st_flags(&e.status), 4.5);
    }

    // Registries and anything else: storage cabinets in a row behind the racks.
    for (i, e) in others.iter().enumerate() {
        // In front of the drafting tables, so they read next to the repositories.
        let x = hb.min.x - 2.4 + (i % 4) as f32 * 0.75;
        let z = zc + 1.9 + (i / 4) as f32 * 0.9;
        let name = if e.name.is_empty() { e.id.clone() } else { e.name.clone() };
        let kind = if e.kind.is_empty() { "external".to_string() } else { e.kind.clone() };
        let (w, h, d) = match kind.as_str() {
            "registry" => (0.5, 0.42, 0.55),
            "host" => (0.26, 0.5, 0.58),
            "cluster" => (0.6, 0.62, 0.8),
            _ => (0.32, 0.32, 0.32),
        };
        let body = Aabb::new(Vec3::new(x, 0.0, z - d), Vec3::new(x + w, h, z));
        b.add(&e.id, Kind::External, &kind, None, body, Lod::ALWAYS, Vec3::new(x + w * 0.5, h + 0.05, z), Anchor::Above, 0.75, &e.status, name, kind.clone());
        let g = &mut b.l.geo;
        let a = Lod::ALWAYS;
        let det = Lod::far(body.center(), DETAIL_FAR);
        g.box_edges(&body, rgba(CYAN, 0.85), 1.2, a, 0);
        g.face(&body, tint(e.status.level, CYAN, 0.05), a, OCCLUDER);
        let zf = z + 0.0005;
        match kind.as_str() {
            "registry" | "cluster" => {
                let slots = if kind == "registry" { 6 } else { 8 };
                for s in 0..slots {
                    let y = 0.04 + s as f32 * ((h - 0.08) / slots as f32);
                    g.rect_z(x + 0.03, y, x + w - 0.08, y + (h - 0.08) / slots as f32 - 0.008, zf, rgba(PALE, 0.55), 0.7, det, 0);
                }
            }
            _ => {
                g.line(Vec3::new(x, 0.0, zf), Vec3::new(x + w, h, zf), rgba(DIM, 0.6), 0.8, det, 0);
                g.line(Vec3::new(x + w, 0.0, zf), Vec3::new(x, h, zf), rgba(DIM, 0.6), 0.8, det, 0);
            }
        }
        g.led(Vec3::new(x + w - 0.035, h - 0.035, z + 0.004), 0.016, e.status.level.rgb(), a, st_flags(&e.status), 4.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Scene {
        Scene::parse(include_str!("../fixtures/scene.json")).unwrap()
    }

    #[test]
    fn fixture_layout_is_consistent() {
        let s = fixture();
        let l = build(&s);
        let n_groups: usize = s.hosts.iter().map(|h| h.groups.len()).sum();
        let n_wl: usize = s.hosts.iter().flat_map(|h| &h.groups).map(|g| g.workloads.len()).sum();
        assert_eq!(l.nodes.iter().filter(|n| n.kind == Kind::Group).count(), n_groups);
        assert_eq!(l.nodes.iter().filter(|n| n.kind == Kind::Workload).count(), n_wl);
        assert_eq!(l.roots.len(), s.hosts.len() + s.external.len());
        // Every child sits inside its parent's pick box; every body is above the floor.
        for n in &l.nodes {
            assert!(n.body.min.y >= -1e-4, "{} below floor", n.id);
            if let Some(p) = n.parent {
                let pp = &l.nodes[p].pick;
                assert!(pp.contains(n.body.min) && pp.contains(n.body.max), "{} escapes parent", n.id);
            }
        }
        // Modules stay within the 19" opening of their rack.
        for n in l.nodes.iter().filter(|n| n.kind == Kind::Workload) {
            let host = &l.nodes[l.nodes[n.parent.unwrap()].parent.unwrap()];
            assert!(n.body.min.x >= host.body.min.x && n.body.max.x <= host.body.max.x);
            assert!(n.body.max.y <= host.body.max.y);
        }
        assert!(l.has_stale);
        assert!(l.geo.line_count() > 500);
    }

    #[test]
    fn workloads_do_not_overlap() {
        let l = build(&fixture());
        let w: Vec<&Node> = l.nodes.iter().filter(|n| n.kind == Kind::Workload).collect();
        for i in 0..w.len() {
            for j in i + 1..w.len() {
                let (a, b) = (&w[i].body, &w[j].body);
                let overlap = a.min.x < b.max.x - 1e-5 && b.min.x < a.max.x - 1e-5 && a.min.y < b.max.y - 1e-5 && b.min.y < a.max.y - 1e-5;
                assert!(!overlap, "{} overlaps {}", w[i].id, w[j].id);
            }
        }
    }

    #[test]
    fn many_hosts_and_odd_input() {
        let mut hosts = String::new();
        for i in 0..20 {
            if i > 0 {
                hosts.push(',');
            }
            // Every third host has no groups (tower), one has a huge namespace.
            let groups = match i % 3 {
                0 => "[]".to_string(),
                1 => r#"[{"id":"g","name":"x","workloads":[]}]"#.to_string(),
                _ => {
                    let wl: Vec<String> = (0..90).map(|k| format!(r#"{{"id":"w{i}-{k}","name":"w","ready":1,"desired":2,"images":[{{"id":"i{k}"}}]}}"#)).collect();
                    format!(r#"[{{"id":"g{i}","name":"big","workloads":[{}]}}]"#, wl.join(","))
                }
            };
            hosts.push_str(&format!(r#"{{"id":"h{i}","name":"h{i}","groups":{groups}}}"#));
        }
        let s = Scene::parse(&format!(r#"{{"hosts":[{hosts}],"external":[{{"id":"e","kind":"weird"}}]}}"#)).unwrap();
        let l = build(&s);
        assert_eq!(l.hosts.len(), 20);
        // Hosts must not overlap on the floor.
        for i in 0..20 {
            for j in i + 1..20 {
                let (a, b) = (&l.nodes[l.hosts[i]].body, &l.nodes[l.hosts[j]].body);
                let overlap = a.min.x < b.max.x && b.min.x < a.max.x && a.min.z < b.max.z && b.min.z < a.max.z;
                assert!(!overlap);
            }
        }
        let empty = build(&Scene::default());
        assert!(empty.nodes.is_empty() && empty.bounds.is_none());
    }
}
