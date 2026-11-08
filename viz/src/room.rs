//! Walk mode: the scene as a walkable datacenter room. Racks stand in rows with alternating
//! cold/hot aisles; repositories live in a library shelf by the entrance, registries and other
//! assets in storage cabinets along the left wall, domains as uplink panels on the right wall.
//! The room surfaces themselves (floor tiles, walls, ceiling) are ray-cast in the background shader.

use glam::{Vec2, Vec3};

use crate::geo::*;
use crate::layout::{build_host, plan_host, st_flags, tint, Anchor, Builder, Kind, Layout, CYAN, DIM, PALE, RACK_D};
use crate::scene::Scene;

pub const CEIL: f32 = 3.2;
pub const TRAY_Y: f32 = 2.62;
pub const PER_ROW: usize = 6;
pub const RACK_GAP: f32 = 0.45;
pub const HOT_AISLE: f32 = 1.2;
pub const COLD_AISLE: f32 = 1.9;
pub const LIGHT_SPACING: f32 = 2.4;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub min: Vec2,
    pub max: Vec2,
}

impl Rect {
    pub fn of(b: &Aabb) -> Rect {
        Rect { min: Vec2::new(b.min.x, b.min.z), max: Vec2::new(b.max.x, b.max.z) }
    }
    /// Does a circle of radius `r` at `p` overlap this rectangle?
    pub fn hits(&self, p: Vec2, r: f32) -> bool {
        let q = p.clamp(self.min, self.max);
        q.distance_squared(p) < r * r
    }
}

#[derive(Clone, Debug)]
pub struct Room {
    /// Interior: floor y = 0 to ceiling y = CEIL.
    pub bounds: Aabb,
    pub colliders: Vec<Rect>,
    /// Unnamed furniture for the minimap: (kind, footprint).
    pub furniture: Vec<(&'static str, Aabb)>,
    pub spawn: Vec3,
    pub spawn_heading: f32,
}

pub fn build_walk(scene: &Scene) -> Layout {
    let mut layout = Layout::default();
    let plans: Vec<_> = scene.hosts.iter().map(plan_host).collect();

    // Rows of up to six racks; even rows face the entrance (+z), odd rows stand back to back
    // with the previous one (hot aisle between the backs), the next pair faces across a cold aisle.
    let rows: Vec<Vec<usize>> = (0..plans.len()).collect::<Vec<_>>().chunks(PER_ROW).map(|c| c.to_vec()).collect();
    let mut zc = 0.0f32;
    let mut row_z = Vec::new();
    for ri in 0..rows.len() {
        if ri > 0 {
            zc -= RACK_D + if ri % 2 == 1 { HOT_AISLE } else { COLD_AISLE };
        }
        row_z.push(zc);
    }
    let mut row_extents = Vec::new();
    {
        let mut b = Builder { l: &mut layout, walk: true };
        for (ri, row) in rows.iter().enumerate() {
            let w: f32 = row.iter().map(|&i| plans[i].width).sum::<f32>() + RACK_GAP * (row.len().max(1) - 1) as f32;
            let mut x = -w * 0.5;
            row_extents.push((x, x + w, row_z[ri]));
            for &hi in row {
                let p = &plans[hi];
                let flipped = ri % 2 == 1;
                b.l.geo.flip = if flipped { Some(Vec3::new(x + p.width * 0.5, 0.0, row_z[ri])) } else { None };
                build_host(&mut b, &scene.hosts[hi], p, x, row_z[ri]);
                b.l.geo.flip = None;
                x += p.width + RACK_GAP;
            }
        }
    }
    layout.hosts = layout.roots.clone();

    // Room bounds around the rows, with space for furniture and the entrance aisle.
    let hb = layout
        .hosts
        .iter()
        .map(|&i| layout.nodes[i].body)
        .reduce(|a, b| a.union(&b))
        .unwrap_or(Aabb::new(Vec3::new(-0.3, 0.0, -0.5), Vec3::new(0.3, 2.0, 0.5)));
    let mut min = Vec3::new(hb.min.x - 2.7, 0.0, hb.min.z - 1.5);
    let mut max = Vec3::new(hb.max.x + 2.3, CEIL, hb.max.z + 3.3);
    for (lo, hi, want) in [(0usize, 0usize, 7.5f32), (2, 2, 7.5)] {
        let _ = hi;
        let have = max[lo] - min[lo];
        if have < want {
            min[lo] -= (want - have) * 0.5;
            max[lo] += (want - have) * 0.5;
        }
    }
    // Externals take wall space; lengthen the room when they need it.
    let repos = scene.external.iter().filter(|e| e.kind == "repository").count();
    let domains = scene.external.iter().filter(|e| e.kind == "domain").count();
    let others = scene.external.len() - repos - domains;
    let shelves = repos.div_ceil(BINDERS_PER_UNIT);
    let left_need = 1.0 + shelves as f32 * (SHELF_LEN + 0.1) + others as f32 * 0.75 + 0.5;
    let right_need = 1.2 + domains.div_ceil(2) as f32 * 0.7 + 0.5;
    let need = left_need.max(right_need);
    if max.z - min.z < need {
        min.z = max.z - need;
    }
    let bounds = Aabb::new(min, max);

    let mut colliders: Vec<Rect> = layout.hosts.iter().map(|&i| Rect::of(&layout.nodes[i].body)).collect();
    // Walls (thick slabs just outside the room).
    let t = 1.0;
    colliders.push(Rect { min: Vec2::new(min.x - t, min.z - t), max: Vec2::new(min.x, max.z + t) });
    colliders.push(Rect { min: Vec2::new(max.x, min.z - t), max: Vec2::new(max.x + t, max.z + t) });
    colliders.push(Rect { min: Vec2::new(min.x - t, min.z - t), max: Vec2::new(max.x + t, min.z) });
    colliders.push(Rect { min: Vec2::new(min.x - t, max.z), max: Vec2::new(max.x + t, max.z + t) });

    let mut room = Room { bounds, colliders, furniture: vec![], spawn: Vec3::new(0.0, 0.0, max.z - 0.9), spawn_heading: 0.0 };
    {
        let mut b = Builder { l: &mut layout, walk: true };
        structure(&mut b, &mut room, &row_extents);
        furniture(&mut b, &mut room, scene);
    }
    layout.bounds = Some(bounds);
    layout.room = Some(room);
    layout
}

fn structure(b: &mut Builder, room: &mut Room, rows: &[(f32, f32, f32)]) {
    let g = &mut b.l.geo;
    let a = Lod::ALWAYS;
    let r = room.bounds;
    g.box_edges(&r, rgba(CYAN, 0.35), 1.0, a, NO_HIDDEN);
    // Skirting and a dado line along the walls.
    for y in [0.12, 1.05] {
        g.rect_y(r.min.x + 0.001, r.min.z + 0.001, r.max.x - 0.001, r.max.z - 0.001, y, rgba(DIM, 0.35), 0.8, a, NO_HIDDEN);
    }

    // Entrance door, centred on the racks.
    let dz = r.max.z - 0.002;
    let (dx0, dx1) = (-0.55, 0.55);
    g.rect_z(dx0, 0.0, dx1, 2.2, dz, rgba(PALE, 0.8), 1.3, a, NO_HIDDEN);
    g.rect_z(dx0 + 0.05, 0.0, dx1 - 0.05, 2.15, dz, rgba(CYAN, 0.4), 0.8, a, NO_HIDDEN);
    g.line(Vec3::new(0.0, 0.0, dz), Vec3::new(0.0, 2.15, dz), rgba(CYAN, 0.4), 0.8, a, NO_HIDDEN);
    for x in [-0.12, 0.12] {
        g.line(Vec3::new(x, 1.0, dz), Vec3::new(x, 1.12, dz), rgba(PALE, 0.8), 1.4, a, NO_HIDDEN);
    }
    g.face(&Aabb::new(Vec3::new(dx0, 0.0, dz - 0.01), Vec3::new(dx1, 2.2, dz)), rgba(CYAN, 0.05), a, 0);
    g.led(Vec3::new(0.0, 2.32, dz - 0.01), 0.03, crate::scene::Level::Ok.rgb(), a, 0, 3.0);
    room.furniture.push(("door", Aabb::new(Vec3::new(dx0, 0.0, r.max.z - 0.08), Vec3::new(dx1, 2.2, r.max.z))));

    // Cold-aisle floor markings (dashed) in front of rows that face +z or -z.
    for (ri, &(x0, x1, zc)) in rows.iter().enumerate() {
        let front = if ri % 2 == 0 { zc + RACK_D * 0.5 } else { zc - RACK_D * 0.5 };
        let s = if ri % 2 == 0 { 1.0 } else { -1.0 };
        let (z0, z1) = (front + s * 0.15, front + s * 1.05);
        g.rect_y(x0 - 0.1, z0.min(z1), x1 + 0.1, z0.max(z1), 0.002, rgba(CYAN, 0.3), 0.9, a, DASH | NO_HIDDEN);
        // Perforated tiles: a dot field in front of each row.
        let mut x = x0 + 0.15;
        while x < x1 - 0.05 {
            let mut z = z0.min(z1) + 0.15;
            while z < z0.max(z1) - 0.1 {
                g.line(Vec3::new(x - 0.012, 0.002, z), Vec3::new(x + 0.012, 0.002, z), rgba(DIM, 0.4), 0.8, Lod::far(Vec3::new(x, 0.0, z), 7.0), NO_HIDDEN);
                z += 0.1;
            }
            x += 0.1;
        }

        // Cable tray above the row with rungs, hangers and drops into the racks.
        let tray = Aabb::new(Vec3::new(x0 - 0.3, TRAY_Y, zc - 0.17), Vec3::new(x1 + 0.3, TRAY_Y + 0.08, zc + 0.17));
        g.box_edges(&tray, rgba(CYAN, 0.6), 1.0, a, 0);
        g.face(&tray, rgba(CYAN, 0.04), a, 0);
        let mut x = tray.min.x + 0.15;
        while x < tray.max.x {
            g.line(Vec3::new(x, TRAY_Y, tray.min.z), Vec3::new(x, TRAY_Y, tray.max.z), rgba(DIM, 0.5), 0.7, Lod::far(Vec3::new(x, TRAY_Y, zc), 9.0), 0);
            x += 0.3;
        }
        let mut x = tray.min.x + 0.3;
        while x < tray.max.x {
            for z in [tray.min.z, tray.max.z] {
                g.line(Vec3::new(x, TRAY_Y + 0.08, z), Vec3::new(x, CEIL, z), rgba(DIM, 0.45), 0.7, a, 0);
            }
            x += 1.2;
        }
        // Bundles running along the tray.
        for k in 0..3 {
            let z = zc - 0.08 + k as f32 * 0.08;
            g.line(Vec3::new(tray.min.x, TRAY_Y + 0.03, z), Vec3::new(tray.max.x, TRAY_Y + 0.03, z), rgba(PALE, 0.25), 1.2, a, 0);
        }
    }

    // Light fixtures on the ceiling grid (the floor light pools in the shader use the same grid).
    let mut x = r.min.x + LIGHT_SPACING * 0.5;
    while x < r.max.x {
        let mut z = r.min.z + LIGHT_SPACING * 0.5;
        while z < r.max.z {
            let f = Aabb::new(Vec3::new(x - 0.6, CEIL - 0.03, z - 0.15), Vec3::new(x + 0.6, CEIL - 0.005, z + 0.15));
            g.box_edges(&f, rgba(PALE, 0.5), 0.9, a, 0);
            g.face(&f, rgba(PALE, 0.22), a, 0);
            g.line(Vec3::new(f.min.x + 0.05, f.min.y - 0.001, z), Vec3::new(f.max.x - 0.05, f.min.y - 0.001, z), rgba([1.0, 1.0, 1.0], 0.6), 2.0, a, 0);
            z += LIGHT_SPACING;
        }
        x += LIGHT_SPACING;
    }
}

const SHELF_LEN: f32 = 1.2;
const SHELF_LEVELS: usize = 4;
const BINDER_STEP: f32 = 0.066;
const BINDERS_PER_LEVEL: usize = 16;
const BINDERS_PER_UNIT: usize = SHELF_LEVELS * BINDERS_PER_LEVEL;

fn furniture(b: &mut Builder, room: &mut Room, scene: &Scene) {
    let r = room.bounds;
    let a = Lod::ALWAYS;
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
    let name_of = |e: &crate::scene::External| if e.name.is_empty() { e.id.clone() } else { e.name.clone() };

    // Library shelves against the left wall, starting next to the entrance, fronts facing +x.
    let x0 = r.min.x + 0.02;
    let depth = 0.38;
    let mut zt = r.max.z - 1.0; // walking towards the back
    for chunk in repos.chunks(BINDERS_PER_UNIT) {
        let (z0, z1) = (zt - SHELF_LEN, zt);
        let unit = Aabb::new(Vec3::new(x0, 0.0, z0), Vec3::new(x0 + depth, 1.95, z1));
        {
            let g = &mut b.l.geo;
            g.box_edges(&unit, rgba(CYAN, 0.85), 1.2, a, 0);
            g.face(&Aabb::new(unit.min, Vec3::new(x0 + 0.02, 1.95, z1)), rgba(CYAN, 0.06), a, OCCLUDER);
            for k in 0..=SHELF_LEVELS {
                let y = 0.08 + k as f32 * 0.45;
                let board = Aabb::new(Vec3::new(x0, y - 0.02, z0), Vec3::new(x0 + depth, y, z1));
                g.box_edges(&board, rgba(CYAN, 0.6), 0.9, a, 0);
                g.face(&board, rgba(CYAN, 0.08), a, OCCLUDER);
            }
        }
        room.colliders.push(Rect::of(&unit));
        room.furniture.push(("shelf", unit));
        for (i, e) in chunk.iter().enumerate() {
            // Fill the eye-level shelves first.
            let level = [2usize, 1, 3, 0][(i / BINDERS_PER_LEVEL) % SHELF_LEVELS];
            let slot = i % BINDERS_PER_LEVEL;
            let y = 0.08 + level as f32 * 0.45;
            let z = z1 - 0.06 - slot as f32 * BINDER_STEP;
            let h = 0.30 + ((i * 5) % 4) as f32 * 0.015;
            let body = Aabb::new(Vec3::new(x0 + 0.06, y, z - 0.05), Vec3::new(x0 + 0.33, y + h, z));
            let spine_x = body.max.x;
            let idx = b.add(&e.id, Kind::External, "repository", None, body, a, Vec3::new(spine_x, y + h + 0.02, z - 0.025), Anchor::Above, BINDER_STEP, &e.status, name_of(e), "repository".into());
            b.l.nodes[idx].facing = Vec3::X;
            let g = &mut b.l.geo;
            let lod = Lod::far(body.center(), 5.0);
            g.box_edges(&body, rgba(PALE, 0.75), 0.8, a, 0);
            g.face(&body, tint(e.status.level, CYAN, 0.07), a, OCCLUDER);
            // Spine label field and ring holes.
            g.rect_x(spine_x + 0.0005, y + h * 0.45, z - 0.04, y + h * 0.8, z - 0.01, rgba(DIM, 0.8), 0.6, lod, 0);
            g.line(Vec3::new(spine_x + 0.0005, y + 0.05, z - 0.025), Vec3::new(spine_x + 0.0005, y + 0.07, z - 0.025), rgba(DIM, 0.8), 0.8, lod, 0);
            g.led(Vec3::new(spine_x + 0.003, y + h - 0.03, z - 0.025), 0.011, e.status.level.rgb(), a, st_flags(&e.status), 2.5);
        }
        zt = z0 - 0.1;
    }

    // Storage cabinets (registries and anything else) further along the left wall.
    for e in others.iter() {
        let (w, h) = match e.kind.as_str() {
            "registry" => (0.6, 1.25),
            "cluster" => (0.6, 1.9),
            "host" => (0.3, 0.55),
            _ => (0.45, 0.8),
        };
        let (z0, z1) = (zt - w, zt);
        let body = Aabb::new(Vec3::new(x0, 0.0, z0), Vec3::new(x0 + 0.6, h, z1));
        let kind = if e.kind.is_empty() { "external".to_string() } else { e.kind.clone() };
        let idx = b.add(&e.id, Kind::External, &kind, None, body, a, Vec3::new(body.max.x, h + 0.06, (z0 + z1) * 0.5), Anchor::Above, 0.75, &e.status, name_of(e), kind.clone());
        b.l.nodes[idx].facing = Vec3::X;
        let g = &mut b.l.geo;
        let det = Lod::far(body.center(), 7.0);
        g.box_edges(&body, rgba(CYAN, 0.85), 1.2, a, 0);
        g.face(&body, tint(e.status.level, CYAN, 0.05), a, OCCLUDER);
        let xf = body.max.x + 0.0005;
        let slots = ((h - 0.15) / 0.09).floor().max(1.0) as usize;
        for s in 0..slots {
            let y = 0.08 + s as f32 * 0.09;
            g.rect_x(xf, y, z0 + 0.04, y + 0.065, z1 - 0.1, rgba(PALE, 0.5), 0.7, det, 0);
            g.line(Vec3::new(xf, y + 0.032, z1 - 0.08), Vec3::new(xf, y + 0.032, z1 - 0.06), rgba(DIM, 0.8), 1.0, det, 0);
        }
        g.led(Vec3::new(xf + 0.003, h - 0.04, z1 - 0.05), 0.016, e.status.level.rgb(), a, st_flags(&e.status), 3.5);
        room.colliders.push(Rect::of(&body));
        zt = z0 - 0.15;
    }

    // Uplink panels on the right wall, fronts facing -x, two per column.
    let xw = r.max.x;
    for (i, e) in domains.iter().enumerate() {
        let col = i / 2;
        let y0 = if i % 2 == 0 { 1.25 } else { 1.65 };
        let z1 = r.max.z - 1.2 - col as f32 * 0.7;
        let z0 = z1 - 0.5;
        let body = Aabb::new(Vec3::new(xw - 0.08, y0, z0), Vec3::new(xw, y0 + 0.22, z1));
        let idx = b.add(&e.id, Kind::External, "domain", None, body, a, Vec3::new(xw - 0.08, y0 + 0.27, (z0 + z1) * 0.5), Anchor::Above, 0.7, &e.status, name_of(e), "domain".into());
        b.l.nodes[idx].facing = Vec3::NEG_X;
        let g = &mut b.l.geo;
        let det = Lod::far(body.center(), 6.0);
        g.box_edges(&body, rgba(CYAN, 0.9), 1.1, a, 0);
        g.face(&body, tint(e.status.level, CYAN, 0.08), a, OCCLUDER);
        let xf = xw - 0.0805;
        for p in 0..8 {
            let z = z1 - 0.08 - p as f32 * 0.045;
            g.rect_x(xf, y0 + 0.05, z - 0.03, y0 + 0.09, z, rgba(PALE, 0.7), 0.6, det, 0);
        }
        g.rect_x(xf, y0 + 0.13, z0 + 0.04, y0 + 0.18, z1 - 0.12, rgba(DIM, 0.6), 0.6, det, 0);
        // Antenna stub and conduit to the ceiling.
        let zc = (z0 + z1) * 0.5;
        g.line(Vec3::new(xw - 0.04, y0 + 0.22, z0 + 0.06), Vec3::new(xw - 0.04, y0 + 0.5, z0 + 0.06), rgba(PALE, 0.8), 1.0, a, 0);
        g.line(Vec3::new(xw - 0.04, y0 + 0.45, z0 + 0.01), Vec3::new(xw - 0.04, y0 + 0.45, z0 + 0.11), rgba(PALE, 0.8), 1.0, a, 0);
        g.line(Vec3::new(xw - 0.02, y0 + 0.22, zc + 0.15), Vec3::new(xw - 0.02, CEIL, zc + 0.15), rgba(DIM, 0.45), 1.0, a, 0);
        g.led(Vec3::new(xf - 0.003, y0 + 0.155, z1 - 0.05), 0.016, e.status.level.rgb(), a, st_flags(&e.status), 3.5);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Scene {
        Scene::parse(include_str!("../fixtures/scene.json")).unwrap()
    }

    #[test]
    fn room_contains_everything_and_spawn_is_free() {
        let l = build_walk(&fixture());
        let room = l.room.as_ref().unwrap();
        for n in &l.nodes {
            assert!(room.bounds.grow(1e-3).contains(n.body.min) && room.bounds.grow(1e-3).contains(n.body.max), "{} outside the room", n.id);
        }
        let s = Vec2::new(room.spawn.x, room.spawn.z);
        assert!(!room.colliders.iter().any(|c| c.hits(s, 0.3)));
        assert_eq!(l.roots.len(), fixture().hosts.len() + fixture().external.len());
    }

    #[test]
    fn rows_alternate_and_face_their_aisles() {
        let mut hosts = Vec::new();
        for i in 0..20 {
            hosts.push(format!(r#"{{"id":"h{i}","name":"h{i}","groups":[{{"id":"g{i}","name":"n","workloads":[{{"id":"w{i}","ready":1,"desired":1}}]}}]}}"#));
        }
        let s = Scene::parse(&format!(r#"{{"hosts":[{}],"external":[]}}"#, hosts.join(","))).unwrap();
        let l = build_walk(&s);
        let room = l.room.as_ref().unwrap();
        // Row 0 faces +z, row 1 faces -z; children inherit their host's facing.
        let h0 = l.index["h0"];
        let h6 = l.index["h6"];
        assert_eq!(l.nodes[h0].facing, Vec3::Z);
        assert_eq!(l.nodes[h6].facing, Vec3::NEG_Z);
        let w6 = l.index["w6"];
        assert_eq!(l.nodes[w6].facing, Vec3::NEG_Z);
        // A flipped rack's modules sit at its -z face.
        assert!(l.nodes[w6].body.min.z < l.nodes[h6].body.center().z);
        // No two racks overlap and all are in the room.
        for i in 0..20 {
            for j in i + 1..20 {
                let (a, b) = (Rect::of(&l.nodes[l.hosts[i]].body), Rect::of(&l.nodes[l.hosts[j]].body));
                assert!(!(a.min.x < b.max.x && b.min.x < a.max.x && a.min.y < b.max.y && b.min.y < a.max.y));
            }
        }
        assert!(room.bounds.size().x > 7.0);
    }
}
