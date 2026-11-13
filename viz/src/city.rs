//! Walk mode as a night city (docs/topology.md section 5). Every object stands for something:
//!
//! - host = a building on the main street, one floor per app (namespace), lobby at street level
//! - workload = a window on its floor (lit panes = ready replicas), images = silhouettes behind it
//! - domain = a street door with a neon sign (unmatched ones on the door in the square)
//! - repository = a book in the public library on the square
//! - image inventory = crates in the harbour warehouse (stamped = running, dusty = idle)
//! - registry = a crane on the quay, scan source = a rooftop searchlight (off = stale)
//! - P1 fire/smoke, P2 broken glass, P3/P4 cracks and grime, misconfig = left open,
//!   secret = key in the lock, stale = street lamps out.
//!
//! Buildings on the north side face +z (the street); south-side ones are built the same way and
//! rotated with `Geo::flip`.

use glam::{Vec2, Vec3};

use crate::geo::*;
use crate::layout::{Anchor, Builder, Kind, Layout};
use crate::scene::{epoch_days, Group, Host, Level, Scene, Status, Workload};

pub const FRONT: f32 = 6.5; // building fronts at z = -FRONT (north) / +FRONT (south)
pub const STREET: f32 = 4.0; // asphalt half width
pub const LOBBY_H: f32 = 4.6;
pub const FLOOR_H: f32 = 3.2;
pub const DEPTH: f32 = 10.0;
pub const WIN_W: f32 = 1.3;
pub const WIN_H: f32 = 1.75;
pub const WIN_SP: f32 = 2.1;
pub const LEFT: f32 = 4.2; // facade space for the painted floor name
pub const RIGHT: f32 = 1.6;
pub const DOOR_SP: f32 = 2.7;
pub const ALLEY: f32 = 3.5;
pub const CROSS_STREET: f32 = 12.0;
pub const MAX_LIGHTS: usize = 48;

const WALL: [f32; 3] = [0.12, 0.125, 0.145];
const LEDGE: [f32; 3] = [0.12, 0.125, 0.145];
const FRAME: [f32; 3] = [0.15, 0.15, 0.17];
const LIT: [f32; 3] = [1.0, 0.72, 0.40];
const DARK_GLASS: [f32; 3] = [0.03, 0.04, 0.058];
const SODIUM: [f32; 3] = [1.0, 0.6, 0.24];
const GOLD: [f32; 3] = [1.0, 0.8, 0.32];
const FLAME: [f32; 3] = [1.0, 0.45, 0.12];
const PAVING: [f32; 3] = [0.10, 0.10, 0.115];
const SIDEWALK: [f32; 3] = [0.115, 0.115, 0.13];
const INK: Rgba = [0.0, 0.0, 0.0, 0.85];
const PALE_INK: Rgba = [0.75, 0.8, 0.9, 0.55];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub min: Vec2,
    pub max: Vec2,
}

impl Rect {
    pub fn of(b: &Aabb) -> Rect {
        Rect {
            min: Vec2::new(b.min.x, b.min.z),
            max: Vec2::new(b.max.x, b.max.z),
        }
    }
    /// Does a circle of radius `r` at `p` overlap this rectangle?
    pub fn hits(&self, p: Vec2, r: f32) -> bool {
        let q = p.clamp(self.min, self.max);
        q.distance_squared(p) < r * r
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Beam {
    pub node: usize,
    pub origin: Vec3,
    pub phase: f32,
    pub on: bool,
}

#[derive(Clone, Debug)]
pub struct City {
    pub bounds: Aabb,
    pub colliders: Vec<Rect>,
    /// Solid walls that stop the crosshair (library and warehouse shells).
    pub blockers: Vec<Aabb>,
    /// Minimap shapes that are not scene elements: (kind, footprint).
    pub furniture: Vec<(&'static str, Aabb)>,
    pub spawn: Vec3,
    pub spawn_heading: f32,
    /// Point lights: position + radius, colour + strength.
    pub lights: Vec<[f32; 8]>,
    pub beams: Vec<Beam>,
    pub water_x: f32,
}

impl City {
    fn light(&mut self, g: &Geo, p: Vec3, radius: f32, rgb: [f32; 3], strength: f32) {
        if self.lights.len() < MAX_LIGHTS {
            let p = g.t(p);
            self.lights
                .push([p.x, p.y, p.z, radius, rgb[0], rgb[1], rgb[2], strength]);
        }
    }
    fn collide(&mut self, g: &Geo, b: &Aabb) {
        self.colliders.push(Rect::of(&g.tb(b)));
    }
}

// ---- meanings ---------------------------------------------------------------------------

fn n_of(n: u32, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// Problem phrases shared by all objects, worst first.
fn problems(
    s: &Status,
    fire: &str,
    broken: &str,
    open: Option<&str>,
    key: &str,
    misconfig_extra: u32,
) -> Vec<String> {
    let mut p = Vec::new();
    let (p1, p2, minor) = (
        s.count(Level::P1),
        s.count(Level::P2),
        s.count(Level::P3) + s.count(Level::P4),
    );
    if p1 > 0 {
        p.push(format!(
            "{fire} — {}",
            n_of(p1, "P1 finding", "P1 findings")
        ));
    }
    if p2 > 0 {
        p.push(format!(
            "{broken} — {}",
            n_of(p2, "P2 finding", "P2 findings")
        ));
    }
    let mis = s.misconfigs().max(misconfig_extra);
    if let (Some(open), true) = (open, mis > 0) {
        p.push(format!(
            "{open} — {}",
            n_of(mis, "misconfiguration", "misconfigurations")
        ));
    }
    let sec = s.secrets();
    if sec > 0 {
        p.push(format!(
            "{key} — {}",
            n_of(sec, "leaked secret", "leaked secrets")
        ));
    }
    if minor > 0 {
        p.push(format!(
            "cracks and grime — {}",
            n_of(minor, "P3/P4 finding", "P3/P4 findings")
        ));
    }
    if s.stale {
        p.push("scan overdue".into());
    }
    p
}

fn summary(s: &Status) -> String {
    let total = s.open.total.max(0.0).round() as u32;
    match (s.level, total) {
        (Level::Unknown, 0) => "never scanned".into(),
        (_, 0) => "no open findings".into(),
        (l, t) => format!("{} open, worst {}", t, l.as_str().to_uppercase()),
    }
}

fn join(parts: Vec<String>, clean: &str) -> String {
    if parts.is_empty() {
        clean.to_string()
    } else {
        parts.join("; ")
    }
}

fn window_meaning(w: &Workload) -> String {
    let ready = w.ready.max(0.0).round() as u32;
    let desired = w.desired.max(0.0).round() as u32;
    let mis = w.misconfig.max(0.0).round() as u32;
    let mut p = problems(
        &w.status,
        "on fire",
        "window broken",
        Some("window left open"),
        "key left in the latch",
        mis,
    );
    if desired == 0 {
        p.push("window dark — scaled to zero".into());
    } else if ready < desired {
        p.push(format!(
            "{} dark — {ready}/{desired} ready",
            n_of(desired - ready, "pane", "panes")
        ));
    }
    join(
        p,
        &format!("warm light, intact glass — {ready}/{desired} ready, no open findings"),
    )
}

fn hash(i: u32) -> f32 {
    let mut x = i.wrapping_mul(0x9E37_79B9) ^ 0x85EB_CA6B;
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^= x >> 12;
    (x & 0xFFFF) as f32 / 65536.0
}

fn neon(l: Level) -> [f32; 3] {
    match l {
        Level::Ok => [0.25, 1.0, 0.78],
        Level::P1 => [1.0, 0.16, 0.3],
        Level::P2 => [1.0, 0.45, 0.15],
        Level::P3 => [1.0, 0.85, 0.25],
        Level::P4 => [0.45, 0.65, 1.0],
        Level::Unknown => [0.6, 0.42, 0.95],
    }
}

// ---- drawing helpers ----------------------------------------------------------------------

fn solid(g: &mut Geo, b: &Aabb, rgb: [f32; 3], lod: Lod) {
    g.face(b, rgba(rgb, 1.0), lod, OCCLUDER);
}

fn glow_rect_z(
    g: &mut Geo,
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    z: f32,
    rgb: [f32; 3],
    w: f32,
    lod: Lod,
    flags: u32,
) {
    g.rect_z(x0, y0, x1, y1, z, rgba(rgb, 0.95), w, lod, GLOW | flags);
}

fn fire(g: &mut Geo, city: &mut City, p: Vec3, scale: f32, seed: u32, lod: Lod) {
    for i in 0..9 {
        let j = Vec3::new(
            (hash(seed * 31 + i) - 0.5) * scale * 0.7,
            0.0,
            (hash(seed * 17 + i) - 0.5) * scale * 0.2,
        );
        g.particle(
            p + j,
            scale * (0.45 + hash(seed + i) * 0.3),
            FLAME,
            lod,
            FIRE,
            2.0,
            i as f32 / 9.0,
        );
    }
    for i in 0..7 {
        let j = Vec3::new((hash(seed * 13 + i) - 0.5) * scale * 0.6, scale * 0.6, 0.0);
        g.particle(
            p + j,
            scale * 0.8,
            [0.1, 0.1, 0.11],
            lod,
            SMOKE,
            2.0,
            i as f32 / 7.0,
        );
    }
    city.light(g, p + Vec3::new(0.0, 0.3, 0.6), 6.0 * scale, FLAME, 0.9);
}

fn key(g: &mut Geo, p: Vec3, s: f32, lod: Lod) {
    // A key hanging from a lock: ring, shaft, two teeth.
    let c = rgba(GOLD, 1.0);
    let ring: Vec<Vec3> = (0..10)
        .map(|k| {
            let t = k as f32 / 10.0 * std::f32::consts::TAU;
            p + Vec3::new(t.cos() * s * 0.35, -s * 0.35 + t.sin() * s * 0.35, 0.0)
        })
        .collect();
    g.polyline(&ring, true, c, 1.6, lod, GLOW);
    let top = p - Vec3::Y * s * 0.7;
    let tip = p - Vec3::Y * s * 1.7;
    g.line(top, tip, c, 2.0, lod, GLOW);
    for k in 0..2 {
        let y = tip.y + s * (0.12 + k as f32 * 0.25);
        g.line(
            Vec3::new(tip.x, y, p.z),
            Vec3::new(tip.x + s * 0.3, y, p.z),
            c,
            1.8,
            lod,
            GLOW,
        );
    }
    g.led(p - Vec3::Y * s * 0.4, s * 0.5, GOLD, lod, 0, 2.5);
}

fn cracks(g: &mut Geo, origin: Vec3, n: u32, seed: u32, lod: Lod) {
    // Zig-zag hairline cracks in the plaster, more and longer with more findings.
    for k in 0..n.min(8) {
        let mut p = origin
            + Vec3::new(
                (hash(seed + k * 7) - 0.5) * 1.2,
                (hash(seed + k * 11) - 0.5) * 0.6,
                0.0,
            );
        let dir = if hash(seed + k * 3) > 0.5 { 1.0 } else { -1.0 };
        for s in 0..4 {
            let q = p + Vec3::new(
                dir * (0.12 + hash(seed + k * 5 + s) * 0.2),
                -(0.15 + hash(seed + k + s * 13) * 0.25),
                0.0,
            );
            g.line(p, q, INK, 1.1, lod, 0);
            p = q;
        }
    }
}

fn grime(g: &mut Geo, x: f32, y_top: f32, z: f32, n: u32, seed: u32, lod: Lod) {
    for k in 0..n.min(6) {
        let xx = x + (hash(seed + k * 19) - 0.5) * 1.4;
        let h = 0.5 + hash(seed + k * 23) * 1.2;
        g.face(
            &Aabb::new(
                Vec3::new(xx, y_top - h, z),
                Vec3::new(xx + 0.12 + hash(seed + k) * 0.12, y_top, z + 0.012),
            ),
            [0.0, 0.0, 0.0, 0.55],
            lod,
            0,
        );
    }
}

/// Seven-segment-ish stroke font for the two public signs.
fn glyph(c: char) -> &'static [(f32, f32, f32, f32)] {
    match c {
        'L' => &[(0., 1., 0., 0.), (0., 0., 0.7, 0.)],
        'I' => &[(0.35, 0., 0.35, 1.), (0.1, 1., 0.6, 1.), (0.1, 0., 0.6, 0.)],
        'B' => &[
            (0., 0., 0., 1.),
            (0., 1., 0.55, 1.),
            (0.55, 1., 0.65, 0.75),
            (0.65, 0.75, 0.55, 0.5),
            (0., 0.5, 0.55, 0.5),
            (0.55, 0.5, 0.7, 0.25),
            (0.7, 0.25, 0.55, 0.),
            (0.55, 0., 0., 0.),
        ],
        'R' => &[
            (0., 0., 0., 1.),
            (0., 1., 0.55, 1.),
            (0.55, 1., 0.68, 0.75),
            (0.68, 0.75, 0.55, 0.5),
            (0.55, 0.5, 0., 0.5),
            (0.3, 0.5, 0.7, 0.),
        ],
        'A' => &[
            (0., 0., 0.35, 1.),
            (0.35, 1., 0.7, 0.),
            (0.13, 0.4, 0.57, 0.4),
        ],
        'Y' => &[
            (0., 1., 0.35, 0.5),
            (0.7, 1., 0.35, 0.5),
            (0.35, 0.5, 0.35, 0.),
        ],
        'H' => &[(0., 0., 0., 1.), (0.7, 0., 0.7, 1.), (0., 0.5, 0.7, 0.5)],
        'O' => &[
            (0., 0., 0., 1.),
            (0., 1., 0.7, 1.),
            (0.7, 1., 0.7, 0.),
            (0.7, 0., 0., 0.),
        ],
        'U' => &[(0., 1., 0., 0.), (0., 0., 0.7, 0.), (0.7, 0., 0.7, 1.)],
        _ => &[],
    }
}

fn sign_text(g: &mut Geo, text: &str, x0: f32, y0: f32, z: f32, h: f32, rgb: [f32; 3], lod: Lod) {
    let mut x = x0;
    for c in text.chars() {
        for &(ax, ay, bx, by) in glyph(c) {
            g.line(
                Vec3::new(x + ax * h, y0 + ay * h, z),
                Vec3::new(x + bx * h, y0 + by * h, z),
                rgba(rgb, 0.95),
                2.2,
                lod,
                GLOW,
            );
        }
        x += h;
    }
}

fn sign_width(text: &str, h: f32) -> f32 {
    text.chars().count() as f32 * h - 0.3 * h
}

// ---- the city ----------------------------------------------------------------------------

struct Plan {
    width: f32,
    height: f32,
    side: i32, // 0 north (faces +z), 1 south (faces -z)
    x0: f32,
}

fn domains_of(h: &Host) -> Vec<(usize, String)> {
    let mut v = Vec::new();
    for (gi, g) in h.groups.iter().enumerate() {
        for d in &g.domains {
            v.push((gi, d.clone()));
        }
    }
    v
}

fn plan_building(h: &Host) -> (f32, f32) {
    let max_w = h
        .groups
        .iter()
        .map(|g| g.workloads.len())
        .max()
        .unwrap_or(0)
        .max(2);
    let doors = domains_of(h).len() + 1;
    let width = (LEFT + max_w as f32 * WIN_SP + RIGHT)
        .max(LEFT + doors as f32 * DOOR_SP + RIGHT)
        .max(9.0);
    let height = LOBBY_H + h.groups.len() as f32 * FLOOR_H + 0.9;
    (width, height)
}

pub fn build_city(scene: &Scene) -> Layout {
    let mut layout = Layout::default();
    let mut order: Vec<usize> = (0..scene.hosts.len()).collect();
    order.sort_by(|&a, &b| scene.hosts[a].cluster.cmp(&scene.hosts[b].cluster)); // stable: districts stay together

    // Buildings alternate north/south along the main street; a cross street separates clusters.
    let mut plans: Vec<Option<Plan>> = (0..scene.hosts.len()).map(|_| None).collect();
    let mut cursor = [0.0f32, 0.0f32];
    let mut last_cluster: Option<&str> = None;
    for (k, &hi) in order.iter().enumerate() {
        let h = &scene.hosts[hi];
        if last_cluster.is_some_and(|c| c != h.cluster) {
            let m = cursor[0].max(cursor[1]) + CROSS_STREET;
            cursor = [m, m];
        }
        last_cluster = Some(&h.cluster);
        let side = if cursor[1] + 0.5 < cursor[0] {
            1
        } else if k == 0 {
            0
        } else {
            (k % 2) as i32
        };
        let (width, height) = plan_building(h);
        let x0 = cursor[side as usize];
        cursor[side as usize] += width + ALLEY;
        plans[hi] = Some(Plan {
            width,
            height,
            side,
            x0,
        });
    }
    let street_end = cursor[0].max(cursor[1]).max(14.0) + 2.0;

    let images: Vec<crate::scene::InventoryImage> = if scene.images.is_empty() {
        // Older servers: the warehouse holds the running images.
        let mut seen = std::collections::HashSet::new();
        let mut v = Vec::new();
        for g in scene.hosts.iter().flat_map(|h| &h.groups) {
            for im in g.workloads.iter().flat_map(|w| &w.images) {
                if seen.insert(im.id.clone()) {
                    v.push(crate::scene::InventoryImage {
                        id: im.id.clone(),
                        name: im.name.clone(),
                        status: im.status.clone(),
                        running: true,
                    });
                }
            }
        }
        v
    } else {
        scene.images.clone()
    };
    let crate_cols = images.len().div_ceil(2).max(1);
    let per_row = 8usize;
    let crate_rows = crate_cols.div_ceil(per_row).max(1);
    let wh_w = per_row.min(crate_cols) as f32 * 1.8 + 4.0;
    let wh_w = wh_w.max(12.0);
    let wh_d = crate_rows as f32 * 3.0 + 4.0;
    let wh_x0 = street_end + 3.0;
    let water_x = wh_x0 + wh_w + 7.0;

    let bounds = Aabb::new(
        Vec3::new(-40.0, 0.0, -34.0),
        Vec3::new(water_x + 60.0, 120.0, 26.0),
    );
    let mut city = City {
        bounds,
        colliders: vec![],
        blockers: vec![],
        furniture: vec![],
        spawn: Vec3::new(-7.0, 0.0, 2.2),
        spawn_heading: std::f32::consts::FRAC_PI_2 - 0.16,
        lights: vec![],
        beams: vec![],
        water_x,
    };
    // Edges of the walkable city: the square's back, the quay edge, north/south limits.
    let t = 2.0;
    for r in [
        Rect {
            min: Vec2::new(-40.0 - t, -34.0),
            max: Vec2::new(-40.0, 26.0),
        },
        Rect {
            min: Vec2::new(water_x - 0.4, -34.0),
            max: Vec2::new(water_x + 60.0, 26.0),
        },
        Rect {
            min: Vec2::new(-40.0, -34.0 - t),
            max: Vec2::new(water_x, -34.0),
        },
        Rect {
            min: Vec2::new(-40.0, 26.0),
            max: Vec2::new(water_x, 26.0 + t),
        },
    ] {
        city.colliders.push(r);
    }

    let mut b = Builder {
        l: &mut layout,
        walk: true,
    };
    ground(&mut b, &mut city, street_end, water_x);
    for (hi, h) in scene.hosts.iter().enumerate() {
        let p = plans[hi].as_ref().unwrap();
        b.l.geo.flip = if p.side == 1 {
            Some(Vec3::new(p.x0 + p.width * 0.5, 0.0, 0.0))
        } else {
            None
        };
        building(&mut b, &mut city, scene, h, p);
        b.l.geo.flip = None;
    }
    b.l.hosts =
        b.l.nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| n.kind == Kind::Host)
            .map(|(i, _)| i)
            .collect();
    library(&mut b, &mut city, scene);
    square(&mut b, &mut city, scene);
    harbour(
        &mut b, &mut city, scene, &images, wh_x0, wh_w, wh_d, water_x,
    );
    searchlights(&mut b, &mut city, scene, &plans);
    city.furniture.push((
        "street",
        Aabb::new(
            Vec3::new(-4.0, 0.0, -STREET),
            Vec3::new(street_end, 0.0, STREET),
        ),
    ));
    layout.animated = !city.beams.is_empty()
        || layout
            .geo
            .leds
            .chunks(LED_STRIDE)
            .any(|l| (l[13] as u32) & (FIRE | SMOKE) != 0);
    layout.bounds = Some(bounds);
    layout.room = Some(city);
    layout
}

fn ground(b: &mut Builder, city: &mut City, street_end: f32, water_x: f32) {
    let g = &mut b.l.geo;
    let a = Lod::ALWAYS;
    // Sidewalks with curbs along the street.
    for (z0, z1) in [(-FRONT - 0.5, -STREET), (STREET, FRONT + 0.5)] {
        let s = Aabb::new(Vec3::new(-4.0, 0.0, z0), Vec3::new(street_end, 0.14, z1));
        solid(g, &s, SIDEWALK, a);
        let zc = if z0 < 0.0 { z1 } else { z0 };
        g.line(
            Vec3::new(-4.0, 0.14, zc),
            Vec3::new(street_end, 0.14, zc),
            [0.4, 0.42, 0.5, 0.5],
            1.0,
            a,
            0,
        );
    }
    // The square (west) and the quay (east) are paved.
    let sq = Aabb::new(Vec3::new(-40.0, 0.0, -34.0), Vec3::new(-4.0, 0.08, 26.0));
    solid(g, &sq, PAVING, a);
    let quay = Aabb::new(
        Vec3::new(street_end, 0.0, -34.0),
        Vec3::new(water_x, 0.08, 26.0),
    );
    solid(g, &quay, PAVING, a);
    // Quay edge with bollards.
    g.line(
        Vec3::new(water_x, 0.08, -34.0),
        Vec3::new(water_x, 0.08, 26.0),
        [0.9, 0.75, 0.3, 0.5],
        1.4,
        a,
        0,
    );
    let mut z = -30.0;
    while z < 24.0 {
        let bo = Aabb::new(
            Vec3::new(water_x - 0.5, 0.0, z),
            Vec3::new(water_x - 0.2, 0.55, z + 0.3),
        );
        solid(g, &bo, [0.12, 0.12, 0.13], a);
        z += 6.0;
    }
    city.furniture.push(("square", sq));
    city.furniture.push(("quay", quay));
}

fn building(b: &mut Builder, city: &mut City, scene: &Scene, h: &Host, p: &Plan) {
    let x0 = p.x0;
    let x1 = x0 + p.width;
    let zf = -FRONT;
    let zb = zf - DEPTH;
    let height = p.height;
    let body = Aabb::new(Vec3::new(x0, 0.0, zb), Vec3::new(x1, height, zf));
    let name = if h.name.is_empty() {
        h.id.clone()
    } else {
        h.name.clone()
    };
    let floors = h.groups.len();
    let lamps_on = !h.status.stale && h.status.level != Level::Unknown;
    let hi = b.add(
        &h.id,
        Kind::Host,
        "host",
        None,
        body,
        Lod::ALWAYS,
        Vec3::new((x0 + x1) * 0.5, LOBBY_H - 0.55, zf + 0.3),
        Anchor::Center,
        2.0,
        &h.status,
        name.clone(),
        h.subtitle.clone(),
    );
    {
        let mut parts = problems(
            &h.status,
            "smoke from the roof",
            "lobby glass broken",
            Some("service door left open"),
            "key left in the service door",
            0,
        );
        if !lamps_on {
            parts.push(if h.status.level == Level::Unknown {
                "street lamps out — never scanned".into()
            } else {
                "street lamps out — scan overdue".into()
            });
        }
        let n = &mut b.l.nodes[hi];
        n.title = name.clone();
        n.meaning = format!(
            "building of {} = {}; host itself: {}",
            n_of(floors as u32, "floor", "floors"),
            n_of(floors as u32, "app", "apps"),
            join(parts, "lamps on, clean lobby — no open findings")
        );
    }
    let a = Lod::ALWAYS;
    {
        let g = &mut b.l.geo;
        solid(g, &body, WALL, a);
        g.box_edges(&body, INK, 1.4, a, 0);
        // Cornice and parapet.
        let cap = Aabb::new(
            Vec3::new(x0 - 0.15, height - 0.9, zf - 0.1),
            Vec3::new(x1 + 0.15, height - 0.6, zf + 0.25),
        );
        solid(g, &cap, LEDGE, a);
        g.box_edges(&cap, INK, 1.0, a, 0);
        // Lobby band.
        let lob = Aabb::new(Vec3::new(x0, 0.0, zf), Vec3::new(x1, LOBBY_H, zf + 0.12));
        solid(g, &lob, [0.06, 0.062, 0.075], a);
        let lt = Aabb::new(
            Vec3::new(x0 - 0.1, LOBBY_H - 0.25, zf),
            Vec3::new(x1 + 0.1, LOBBY_H, zf + 0.4),
        );
        solid(g, &lt, LEDGE, a);
        g.box_edges(&lt, INK, 1.0, a, 0);
        // Name plaque over the lobby (the label sits on it).
        let pw = (name.chars().count() as f32 * 0.32).clamp(3.0, p.width - 1.0);
        let pl = Aabb::new(
            Vec3::new((x0 + x1) * 0.5 - pw * 0.5, LOBBY_H - 0.95, zf + 0.12),
            Vec3::new((x0 + x1) * 0.5 + pw * 0.5, LOBBY_H - 0.3, zf + 0.2),
        );
        g.face(&pl, rgba([0.55, 0.5, 0.42], 1.0), a, OCCLUDER | EMISSIVE);
        g.box_edges(&pl, INK, 1.0, a, 0);
    }
    city.collide(&b.l.geo, &body);

    // Host's own state on the lobby: service door (misconfig / secret), glass, grime, roof smoke.
    let det = Lod::far(Vec3::new((x0 + x1) * 0.5, 2.0, zf), 60.0);
    {
        let sd_x = x0 + 1.0;
        let door = Aabb::new(
            Vec3::new(sd_x, 0.14, zf + 0.12),
            Vec3::new(sd_x + 1.1, 2.5, zf + 0.2),
        );
        let open = h.status.misconfigs() > 0;
        let g = &mut b.l.geo;
        if open {
            let leaf = Aabb::new(
                Vec3::new(sd_x, 0.14, zf + 0.2),
                Vec3::new(sd_x + 0.06, 2.5, zf + 1.2),
            );
            solid(g, &leaf, [0.2, 0.18, 0.16], a);
            g.box_edges(&leaf, INK, 1.0, a, 0);
            g.face(&door, rgba(LIT, 0.9), a, EMISSIVE);
        } else {
            solid(g, &door, [0.17, 0.15, 0.13], a);
        }
        g.box_edges(&door, INK, 1.1, a, 0);
        if h.status.secrets() > 0 {
            key(g, Vec3::new(sd_x + 0.95, 1.25, zf + 0.23), 0.12, det);
        }
        let p1 = h.status.count(Level::P1);
        if p1 > 0 {
            fire(
                g,
                city,
                Vec3::new(x1 - 2.0, height + 0.2, zf - 2.0),
                1.6,
                7 + p1,
                Lod::ALWAYS,
            );
        }
        // Shop-front glass between service door and street doors.
        let gx0 = sd_x + 1.4;
        let glass = Aabb::new(
            Vec3::new(gx0, 0.9, zf + 0.12),
            Vec3::new(gx0 + 1.5, 3.4, zf + 0.15),
        );
        let lit = h.status.level != Level::Unknown;
        g.face(
            &glass,
            rgba(if lit { [0.55, 0.42, 0.26] } else { DARK_GLASS }, 1.0),
            a,
            OCCLUDER | if lit { EMISSIVE } else { 0 },
        );
        g.box_edges(&glass, INK, 1.2, a, 0);
        if h.status.count(Level::P2) > 0 {
            let c = Vec3::new(gx0 + 0.6, 2.4, zf + 0.16);
            for k in 0..7 {
                let t = k as f32 / 7.0 * std::f32::consts::TAU + 0.3;
                g.line(
                    c,
                    c + Vec3::new(t.cos(), t.sin(), 0.0) * (0.35 + hash(k) * 0.4),
                    PALE_INK,
                    1.0,
                    det,
                    GLOW,
                );
            }
        }
        let minor = h.status.count(Level::P3) + h.status.count(Level::P4);
        if minor > 0 {
            cracks(g, Vec3::new(x1 - 1.5, 3.4, zf + 0.125), minor, 99, det);
            grime(g, x1 - 1.6, 3.8, zf + 0.125, minor, 33, det);
        }
    }

    // Street lamps in front: lit unless the host went quiet.
    for (k, lx) in [x0 + 1.2, x1 - 1.2].into_iter().enumerate() {
        let lz = -STREET - 0.45;
        let pole = Aabb::new(
            Vec3::new(lx - 0.08, 0.14, lz - 0.08),
            Vec3::new(lx + 0.08, 4.6, lz + 0.08),
        );
        let head = Aabb::new(
            Vec3::new(lx - 0.25, 4.45, lz - 0.05),
            Vec3::new(lx + 0.25, 4.65, lz + 0.75),
        );
        let li = b.add(
            &h.id,
            Kind::External,
            "lamp",
            None,
            pole.union(&head),
            a,
            head.max + Vec3::Y * 0.3,
            Anchor::Above,
            0.3,
            &h.status,
            name.clone(),
            String::new(),
        );
        {
            let n = &mut b.l.nodes[li];
            n.facing = Vec3::ZERO;
            n.title = format!("street lamps · {name}");
            n.meaning = if lamps_on {
                "street lamps on — this building is scanned and current".into()
            } else if h.status.level == Level::Unknown {
                "street lamps out — never scanned".into()
            } else {
                "street lamps out — scan overdue".into()
            };
        }
        let g = &mut b.l.geo;
        solid(g, &pole, [0.05, 0.05, 0.06], a);
        solid(
            g,
            &head,
            if lamps_on {
                [1.0, 0.75, 0.4]
            } else {
                [0.06, 0.06, 0.07]
            },
            a,
        );
        if lamps_on {
            g.face(
                &Aabb::new(
                    Vec3::new(lx - 0.2, 4.43, lz),
                    Vec3::new(lx + 0.2, 4.45, lz + 0.7),
                ),
                rgba([1.0, 0.8, 0.5], 1.0),
                a,
                EMISSIVE,
            );
            g.led(Vec3::new(lx, 4.35, lz + 0.35), 0.45, SODIUM, a, 0, 4.0);
            city.light(g, Vec3::new(lx, 4.3, lz + 0.35), 9.0, SODIUM, 1.0);
        }
        g.box_edges(&head, INK, 1.0, a, 0);
        let _ = k;
        city.colliders.push(Rect::of(&g.tb(&pole.grow(0.05))));
    }

    // Street doors for the domains routed to this building's apps.
    let mut dx = x0 + LEFT;
    for (gi, host_name) in domains_of(h) {
        let ext = scene
            .external
            .iter()
            .find(|e| e.kind == "domain" && e.name.eq_ignore_ascii_case(&host_name));
        let (id, st) = match ext {
            Some(e) => (e.id.clone(), e.status.clone()),
            None => (format!("domain:{host_name}"), Status::default()),
        };
        let ns = &h.groups[gi].name;
        street_door(
            b,
            city,
            &id,
            &host_name,
            &st,
            ext.is_some(),
            dx,
            zf,
            &format!("door to {ns}"),
        );
        dx += DOOR_SP;
    }

    // Floors bottom-up, one per app; the most troubled apps live nearest the street so their
    // fires and broken glass are seen up close (then alphabetical).
    let mut order: Vec<&Group> = h.groups.iter().collect();
    order.sort_by(|a, b| {
        a.status
            .level
            .cmp(&b.status.level)
            .then_with(|| a.name.cmp(&b.name))
    });
    for (fi, grp) in order.into_iter().enumerate() {
        floor(b, city, hi, grp, fi, floors, x0, x1, zf, &name);
    }
}

#[allow(clippy::too_many_arguments)]
fn street_door(
    b: &mut Builder,
    city: &mut City,
    id: &str,
    host_name: &str,
    st: &Status,
    known: bool,
    x: f32,
    zf: f32,
    what: &str,
) -> usize {
    let a = Lod::ALWAYS;
    let frame = Aabb::new(
        Vec3::new(x - 0.15, 0.14, zf + 0.12),
        Vec3::new(x + 1.35, 2.75, zf + 0.3),
    );
    let sign = Aabb::new(
        Vec3::new(x - 0.25, 2.95, zf + 0.15),
        Vec3::new(x + 1.45, 3.55, zf + 0.35),
    );
    let body = frame.union(&sign);
    let i = b.add(
        id,
        Kind::External,
        "domain",
        None,
        body,
        a,
        Vec3::new(x + 0.6, 3.25, zf + 0.4),
        Anchor::Center,
        1.4,
        st,
        host_name.to_string(),
        what.to_string(),
    );
    let open = st.misconfigs() > 0;
    {
        let n = &mut b.l.nodes[i];
        n.title = format!("{host_name} · {what}");
        let has_record = known;
        let parts = problems(
            st,
            "door on fire",
            "door glass broken",
            Some("door left open"),
            "key left in the lock",
            0,
        );
        n.meaning = join(
            parts,
            if has_record {
                "neon on, door shut — no open findings"
            } else {
                "neon flickering — no asset record for this hostname"
            },
        );
    }
    let g = &mut b.l.geo;
    let det = Lod::far(frame.center(), 40.0);
    solid(
        g,
        &Aabb::new(
            frame.min,
            Vec3::new(frame.max.x, frame.max.y, frame.min.z + 0.06),
        ),
        [0.03, 0.03, 0.035],
        a,
    );
    g.box_edges(&frame, INK, 1.2, a, 0);
    let leaf = Aabb::new(
        Vec3::new(x, 0.14, zf + 0.2),
        Vec3::new(x + 1.2, 2.6, zf + 0.26),
    );
    if open {
        let swung = Aabb::new(
            Vec3::new(x + 1.14, 0.14, zf + 0.26),
            Vec3::new(x + 1.2, 2.6, zf + 1.4),
        );
        solid(g, &swung, [0.22, 0.16, 0.12], a);
        g.box_edges(&swung, INK, 1.0, a, 0);
        g.face(&leaf, rgba(LIT, 0.85), a, EMISSIVE);
        city.light(g, Vec3::new(x + 0.6, 1.2, zf + 1.0), 4.0, LIT, 0.6);
    } else {
        solid(g, &leaf, [0.2, 0.15, 0.11], a);
        g.box_edges(&leaf, INK, 1.0, a, 0);
        let win = Aabb::new(
            Vec3::new(x + 0.3, 1.6, zf + 0.26),
            Vec3::new(x + 0.9, 2.3, zf + 0.27),
        );
        g.face(&win, rgba([0.5, 0.38, 0.24], 1.0), a, EMISSIVE);
        if st.count(Level::P2) > 0 {
            let c = win.center() + Vec3::Z * 0.01;
            for k in 0..6 {
                let t = k as f32 * 1.05;
                g.line(
                    c,
                    c + Vec3::new(t.cos() * 0.3, t.sin() * 0.33, 0.0),
                    PALE_INK,
                    1.0,
                    det,
                    GLOW,
                );
            }
        }
    }
    if st.secrets() > 0 {
        key(g, Vec3::new(x + 1.05, 1.3, zf + 0.3), 0.11, det);
    }
    if st.count(Level::P1) > 0 {
        fire(
            g,
            city,
            Vec3::new(x + 0.6, 2.6, zf + 0.5),
            0.9,
            41,
            Lod::ALWAYS,
        );
    }
    let minor = st.count(Level::P3) + st.count(Level::P4);
    if minor > 0 {
        grime(g, x + 0.6, 2.7, zf + 0.31, minor, 77, det);
    }
    // Neon sign: tube outline in the status colour, flickering when stale or unknown.
    let col = neon(st.level);
    let fl = if st.stale || st.level == Level::Unknown {
        BLINK
    } else {
        0
    };
    solid(g, &sign, [0.02, 0.02, 0.025], a);
    glow_rect_z(
        g,
        sign.min.x + 0.06,
        sign.min.y + 0.06,
        sign.max.x - 0.06,
        sign.max.y - 0.06,
        sign.max.z + 0.01,
        col,
        3.0,
        a,
        fl,
    );
    g.led(
        Vec3::new(sign.center().x, sign.center().y, sign.max.z + 0.05),
        0.28,
        col,
        a,
        fl,
        3.0,
    );
    city.light(
        g,
        Vec3::new(sign.center().x, 3.2, sign.max.z + 0.3),
        6.0,
        col,
        0.9,
    );
    i
}

#[allow(clippy::too_many_arguments)]
fn floor(
    b: &mut Builder,
    city: &mut City,
    hi: usize,
    grp: &Group,
    fi: usize,
    floors: usize,
    x0: f32,
    x1: f32,
    zf: f32,
    host_name: &str,
) {
    let a = Lod::ALWAYS;
    let y0 = LOBBY_H + fi as f32 * FLOOR_H;
    let y1 = y0 + FLOOR_H;
    let body = Aabb::new(Vec3::new(x0, y0, zf - 0.05), Vec3::new(x1, y1, zf + 0.3));
    let name = if grp.name.is_empty() {
        grp.id.clone()
    } else {
        grp.name.clone()
    };
    let n_w = grp.workloads.len();
    let gi = b.add(
        &grp.id,
        Kind::Group,
        "group",
        Some(hi),
        body,
        a,
        Vec3::new(x0 + 0.45, y0 + FLOOR_H * 0.55, zf + 0.2),
        Anchor::Right,
        1.2,
        &grp.status,
        name.clone(),
        format!("floor {} of {floors}", fi + 1),
    );
    b.grow_pick(gi);
    {
        let mut parts = Vec::new();
        let fires = grp
            .workloads
            .iter()
            .filter(|w| w.status.count(Level::P1) > 0)
            .count() as u32;
        let broken = grp
            .workloads
            .iter()
            .filter(|w| w.status.count(Level::P2) > 0)
            .count() as u32;
        let open = grp
            .workloads
            .iter()
            .filter(|w| w.misconfig > 0.0 || w.status.misconfigs() > 0)
            .count() as u32;
        let keys = grp
            .workloads
            .iter()
            .filter(|w| w.status.secrets() > 0)
            .count() as u32;
        if fires > 0 {
            parts.push(format!("{} on fire", n_of(fires, "window", "windows")));
        }
        if broken > 0 {
            parts.push(format!("{} broken", n_of(broken, "window", "windows")));
        }
        if open > 0 {
            parts.push(format!("{} left open", n_of(open, "window", "windows")));
        }
        if keys > 0 {
            parts.push(format!(
                "{} with a key in the latch",
                n_of(keys, "window", "windows")
            ));
        }
        let n = &mut b.l.nodes[gi];
        n.title = format!("{name} · {host_name}");
        let head = if n_w == 0 {
            "boarded-up floor — no workloads".to_string()
        } else {
            format!(
                "floor {} of {floors}, {}",
                fi + 1,
                n_of(n_w as u32, "window", "windows")
            )
        };
        n.meaning = format!("{head} — {}", join(parts, &summary(&grp.status)));
    }
    {
        let g = &mut b.l.geo;
        // Ledge at the floor line and the painted name panel on the left of the facade.
        let ledge = Aabb::new(
            Vec3::new(x0 - 0.05, y0 - 0.06, zf),
            Vec3::new(x1 + 0.05, y0 + 0.08, zf + 0.18),
        );
        solid(g, &ledge, LEDGE, a);
        g.line(
            Vec3::new(x0, y0 + 0.08, zf + 0.18),
            Vec3::new(x1, y0 + 0.08, zf + 0.18),
            INK,
            1.0,
            a,
            0,
        );
        let panel = Aabb::new(
            Vec3::new(x0 + 0.3, y0 + 1.15, zf),
            Vec3::new(x0 + LEFT - 0.4, y0 + 2.35, zf + 0.04),
        );
        g.face(&panel, rgba([0.13, 0.125, 0.12], 1.0), a, OCCLUDER);
        g.line(
            Vec3::new(panel.min.x + 0.1, panel.min.y + 0.12, panel.max.z + 0.002),
            Vec3::new(panel.max.x - 0.1, panel.min.y + 0.12, panel.max.z + 0.002),
            [0.7, 0.62, 0.45, 0.6],
            1.0,
            Lod::far(panel.center(), 70.0),
            0,
        );
        if n_w == 0 {
            // Boarded-up windows.
            for k in 0..2 {
                let bx = x0 + LEFT + k as f32 * WIN_SP;
                let bb = Aabb::new(
                    Vec3::new(bx, y0 + 0.8, zf),
                    Vec3::new(bx + WIN_W, y0 + 0.8 + WIN_H, zf + 0.05),
                );
                solid(g, &bb, [0.13, 0.1, 0.08], a);
                for s in 0..4 {
                    let yy = bb.min.y + 0.2 + s as f32 * 0.42;
                    g.line(
                        Vec3::new(bb.min.x, yy, bb.max.z + 0.002),
                        Vec3::new(bb.max.x, yy + 0.08, bb.max.z + 0.002),
                        INK,
                        1.0,
                        a,
                        0,
                    );
                }
            }
        }
    }
    for (wi, w) in grp.workloads.iter().enumerate() {
        let wx = x0 + LEFT + wi as f32 * WIN_SP;
        window(
            b,
            city,
            gi,
            w,
            wx,
            y0 + 0.8,
            zf,
            &name,
            (fi * 37 + wi) as u32,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn window(
    b: &mut Builder,
    city: &mut City,
    gi: usize,
    w: &Workload,
    wx: f32,
    wy: f32,
    zf: f32,
    floor_name: &str,
    seed: u32,
) {
    let a = Lod::ALWAYS;
    let (wx1, wy1) = (wx + WIN_W, wy + WIN_H);
    let frame = Aabb::new(
        Vec3::new(wx - 0.09, wy - 0.12, zf),
        Vec3::new(wx1 + 0.09, wy1 + 0.09, zf + 0.06),
    );
    let name = if w.name.is_empty() {
        w.id.clone()
    } else {
        w.name.clone()
    };
    let ready = w.ready.max(0.0).round() as u32;
    let desired = w.desired.max(0.0).round() as u32;
    let mis = (w.misconfig.max(0.0).round() as u32).max(w.status.misconfigs());
    let kind = if w.kind.is_empty() {
        "Workload".to_string()
    } else {
        w.kind.clone()
    };
    let wi = b.add(
        &w.id,
        Kind::Workload,
        "workload",
        Some(gi),
        frame.grow(0.02),
        a,
        Vec3::new(wx + WIN_W * 0.5, wy1 + 0.2, zf + 0.1),
        Anchor::Above,
        WIN_W,
        &w.status,
        name.clone(),
        format!("{kind} · {ready}/{desired} ready"),
    );
    b.grow_pick(wi);
    {
        let n = &mut b.l.nodes[wi];
        n.title = format!("{name} · {floor_name}");
        n.meaning = window_meaning(w);
    }
    let det = Lod::far(frame.center(), 55.0);
    let p1 = w.status.count(Level::P1);
    let p2 = w.status.count(Level::P2);
    let blink = if w.status.stale { BLINK } else { 0 };
    let panes = desired.clamp(1, 4);
    let pw = WIN_W / panes as f32;
    {
        let g = &mut b.l.geo;
        solid(g, &frame, FRAME, a);
        let sill = Aabb::new(
            Vec3::new(wx - 0.18, wy - 0.2, zf),
            Vec3::new(wx1 + 0.18, wy - 0.1, zf + 0.22),
        );
        solid(g, &sill, LEDGE, a);
        g.box_edges(&sill, INK, 0.9, det, 0);
        for k in 0..panes {
            let px0 = wx + k as f32 * pw + 0.03;
            let px1 = wx + (k + 1) as f32 * pw - 0.03;
            let lit =
                desired > 0 && k < ready.min(panes) && !(desired > 4 && ready * 4 / desired <= k);
            let open = mis > 0 && k == panes - 1;
            let z = zf + 0.065;
            let pane = Aabb::new(Vec3::new(px0, wy, zf + 0.06), Vec3::new(px1, wy1, z));
            let col = if p1 > 0 && lit {
                [1.0, 0.42, 0.14]
            } else if lit {
                let v = 0.9 + hash(seed * 7 + k) * 0.15;
                [LIT[0] * v, LIT[1] * v, LIT[2] * v]
            } else {
                DARK_GLASS
            };
            g.face(
                &pane,
                rgba(col, 1.0),
                a,
                OCCLUDER | if lit { EMISSIVE | blink } else { 0 },
            );
            if open {
                // Sash swung out into the street, a curtain blowing after it.
                let sash = Aabb::new(
                    Vec3::new(px1 - 0.02, wy + 0.02, z),
                    Vec3::new(px1 + 0.01, wy1 - 0.02, z + (px1 - px0)),
                );
                g.face(&sash, rgba([0.55, 0.6, 0.7], 0.35), a, 0);
                g.box_edges(&sash, INK, 1.0, a, 0);
                let c = [0.85, 0.82, 0.75, 0.8];
                g.polyline(
                    &[
                        Vec3::new(px0 + 0.1, wy1 - 0.05, z),
                        Vec3::new(px0 + 0.05, wy1 - 0.6, z + 0.25),
                        Vec3::new(px0 + 0.2, wy + 0.35, z + 0.45),
                    ],
                    false,
                    c,
                    1.6,
                    det,
                    0,
                );
                g.polyline(
                    &[
                        Vec3::new(px0 + 0.3, wy1 - 0.05, z),
                        Vec3::new(px0 + 0.25, wy1 - 0.7, z + 0.2),
                        Vec3::new(px0 + 0.38, wy + 0.25, z + 0.35),
                    ],
                    false,
                    c,
                    1.6,
                    det,
                    0,
                );
            }
            if k > 0 {
                g.line(
                    Vec3::new(px0 - 0.03, wy, z + 0.002),
                    Vec3::new(px0 - 0.03, wy1, z + 0.002),
                    INK,
                    1.6,
                    det,
                    0,
                );
            }
            // Broken panes: a star of cracks and a missing shard.
            if p2 > k && !open {
                let c = Vec3::new(px0 + (px1 - px0) * 0.6, wy + WIN_H * 0.62, z + 0.003);
                for r in 0..7 {
                    let t = r as f32 / 7.0 * std::f32::consts::TAU + hash(seed + r) * 0.5;
                    let len = 0.2 + hash(seed * 3 + r) * 0.35;
                    g.line(
                        c,
                        c + Vec3::new(t.cos() * len, t.sin() * len, 0.0),
                        [0.9, 0.95, 1.0, 0.85],
                        1.0,
                        det,
                        GLOW,
                    );
                }
                let shard = Aabb::new(
                    Vec3::new(c.x - 0.02, c.y - 0.04, z),
                    Vec3::new((c.x + 0.18).min(px1), (c.y + 0.25).min(wy1), z + 0.004),
                );
                g.face(&shard, rgba([0.01, 0.01, 0.015], 1.0), a, EMISSIVE);
            }
        }
        g.box_edges(&frame, INK, 1.1, a, 0);
        if p1 > 0 {
            fire(
                g,
                city,
                Vec3::new(wx + WIN_W * 0.5, wy1 - 0.1, zf + 0.35),
                1.0 + (p1.min(4) as f32) * 0.15,
                seed + 5,
                Lod::ALWAYS,
            );
        }
        if w.status.secrets() > 0 {
            key(
                g,
                Vec3::new(wx1 + 0.02, wy + WIN_H * 0.55, zf + 0.09),
                0.13,
                det,
            );
        }
        let minor = w.status.count(Level::P3) + w.status.count(Level::P4);
        if minor > 0 {
            cracks(
                g,
                Vec3::new(wx + WIN_W * 0.5, wy - 0.25, zf + 0.002),
                minor,
                seed * 13,
                det,
            );
            grime(
                g,
                wx + WIN_W * 0.5,
                wy - 0.12,
                zf + 0.002,
                minor,
                seed * 29,
                det,
            );
        }
    }
    // Images: silhouettes standing behind the glass.
    let n = w.images.len().min(5);
    for (ii, im) in w.images.iter().take(5).enumerate() {
        let sw = 0.16 + hash(seed * 5 + ii as u32) * 0.14;
        let sh = 0.5 + hash(seed * 9 + ii as u32) * 0.65;
        let cx = wx + (ii as f32 + 0.5) / n as f32 * WIN_W;
        let body = Aabb::new(
            Vec3::new(cx - sw * 0.5, wy, zf + 0.067),
            Vec3::new(cx + sw * 0.5, wy + sh, zf + 0.075),
        );
        let lod = Lod::far(frame.center(), 32.0);
        let label = if im.name.is_empty() {
            im.reference.clone()
        } else {
            im.name.clone()
        };
        let ii_node = b.add(
            &im.id,
            Kind::Image,
            "image",
            Some(wi),
            body,
            lod,
            Vec3::new(cx, wy + 0.02, zf + 0.1),
            Anchor::Below,
            0.4,
            &im.status,
            label.clone(),
            im.reference.clone(),
        );
        b.grow_pick(ii_node);
        {
            let node = &mut b.l.nodes[ii_node];
            node.title = format!("{label} · {name}");
            let parts = problems(
                &im.status,
                "smouldering",
                "cracked",
                None,
                "carrying a key",
                0,
            );
            node.meaning = format!(
                "silhouette behind the window of {name} — {}",
                join(parts, "no open findings")
            );
        }
        let g = &mut b.l.geo;
        g.face(&body, rgba([0.012, 0.01, 0.012], 1.0), lod, EMISSIVE);
        // Head / lamp on top in the image's status colour.
        g.led(
            Vec3::new(cx, wy + sh + 0.05, zf + 0.08),
            0.06,
            im.status.level.rgb(),
            lod,
            if im.status.stale { BLINK } else { 0 },
            1.5,
        );
    }
}

fn library(b: &mut Builder, city: &mut City, scene: &Scene) {
    let repos: Vec<_> = scene
        .external
        .iter()
        .filter(|e| e.kind == "repository")
        .collect();
    let a = Lod::ALWAYS;
    let (x0, x1) = (-31.0f32, -13.0f32);
    let zf = -FRONT;
    let zb = zf - 12.0;
    let h = 8.5;
    let shell = Aabb::new(Vec3::new(x0, 0.0, zb), Vec3::new(x1, h, zf));
    city.furniture.push(("library", shell));
    {
        let g = &mut b.l.geo;
        // Walls (left, right, back), roof with pediment band, steps, columns.
        let walls = [
            Aabb::new(Vec3::new(x0, 0.0, zb), Vec3::new(x0 + 0.5, h, zf)),
            Aabb::new(Vec3::new(x1 - 0.5, 0.0, zb), Vec3::new(x1, h, zf)),
            Aabb::new(Vec3::new(x0, 0.0, zb), Vec3::new(x1, h, zb + 0.5)),
            Aabb::new(Vec3::new(x0, h - 1.6, zb), Vec3::new(x1, h, zf)),
        ];
        for w in &walls {
            solid(g, w, [0.11, 0.105, 0.1], a);
            g.box_edges(w, INK, 1.2, a, 0);
            city.blockers.push(*w);
        }
        for w in &walls[..3] {
            city.colliders.push(Rect::of(w));
        }
        let ped = Aabb::new(
            Vec3::new(x0 - 0.3, h - 1.6, zf),
            Vec3::new(x1 + 0.3, h - 0.3, zf + 0.5),
        );
        solid(g, &ped, [0.15, 0.14, 0.13], a);
        g.box_edges(&ped, INK, 1.2, a, 0);
        let sw = sign_width("LIBRARY", 0.8);
        sign_text(
            g,
            "LIBRARY",
            (x0 + x1) * 0.5 - sw * 0.5,
            h - 1.35,
            zf + 0.52,
            0.8,
            [1.0, 0.85, 0.55],
            a,
        );
        for k in 0..3 {
            let st = Aabb::new(
                Vec3::new(x0 + 0.5, 0.0, zf + k as f32 * 0.35),
                Vec3::new(
                    x1 - 0.5,
                    0.45 - k as f32 * 0.15,
                    zf + 0.35 + k as f32 * 0.35,
                ),
            );
            solid(g, &st, [0.13, 0.125, 0.12], a);
            g.box_edges(&st, INK, 0.9, a, 0);
        }
        for k in 0..6 {
            let cx = x0 + 1.4 + k as f32 * ((x1 - x0 - 2.8) / 5.0);
            let col = Aabb::new(
                Vec3::new(cx - 0.3, 0.45, zf - 0.6),
                Vec3::new(cx + 0.3, h - 1.6, zf),
            );
            solid(g, &col, [0.16, 0.155, 0.15], a);
            g.box_edges(&col, INK, 1.0, a, 0);
            city.colliders.push(Rect::of(&col));
        }
        // Warm reading-room light (it is open to the public).
        city.light(
            g,
            Vec3::new((x0 + x1) * 0.5, 5.0, zb + 4.0),
            14.0,
            [1.0, 0.75, 0.45],
            1.2,
        );
        g.led(
            Vec3::new((x0 + x1) * 0.5, 6.5, zb + 5.0),
            1.0,
            [1.0, 0.8, 0.5],
            a,
            0,
            3.0,
        );
    }
    // Shelves along the back wall, books facing the hall.
    let per_level = 34usize;
    let levels = 6usize;
    let units = repos.len().div_ceil(per_level * levels).max(1);
    let unit_w = 2.5f32;
    let total = units as f32 * (unit_w + 0.3);
    let sx0 = (x0 + x1) * 0.5 - total * 0.5;
    let zs = zb + 0.5;
    for u in 0..units {
        let ux = sx0 + u as f32 * (unit_w + 0.3);
        let unit = Aabb::new(
            Vec3::new(ux, 0.0, zs),
            Vec3::new(ux + unit_w, 2.6, zs + 0.45),
        );
        let g = &mut b.l.geo;
        solid(
            g,
            &Aabb::new(unit.min, Vec3::new(unit.max.x, unit.max.y, zs + 0.05)),
            [0.16, 0.1, 0.06],
            a,
        );
        for s in 0..=levels {
            let y = 0.1 + s as f32 * 0.4;
            let board = Aabb::new(
                Vec3::new(ux, y - 0.04, zs),
                Vec3::new(ux + unit_w, y, zs + 0.45),
            );
            solid(g, &board, [0.22, 0.14, 0.08], a);
        }
        for side in [ux, ux + unit_w - 0.06] {
            solid(
                g,
                &Aabb::new(
                    Vec3::new(side, 0.0, zs),
                    Vec3::new(side + 0.06, 2.6, zs + 0.45),
                ),
                [0.22, 0.14, 0.08],
                a,
            );
        }
        g.box_edges(&unit, INK, 1.0, a, 0);
        city.colliders.push(Rect::of(&unit));
    }
    // Eye-level shelves first.
    let level_order = [3usize, 2, 4, 1, 5, 0];
    for (i, e) in repos.iter().enumerate() {
        let u = i / (per_level * levels);
        let r = i % (per_level * levels);
        let level = level_order[r / per_level];
        let slot = r % per_level;
        let ux = sx0 + u as f32 * (unit_w + 0.3);
        let y = 0.1 + level as f32 * 0.4;
        let bh = 0.27 + hash(i as u32 * 3) * 0.08;
        let bw = 0.055 + hash(i as u32 * 7) * 0.02;
        let bx = ux + 0.12 + slot as f32 * 0.068;
        let p2 = e.status.count(Level::P2) > 0;
        let pulled = if p2 { 0.08 } else { 0.0 };
        let body = Aabb::new(
            Vec3::new(bx, y, zs + 0.15 + pulled),
            Vec3::new(bx + bw, y + bh, zs + 0.4 + pulled),
        );
        let name = if e.name.is_empty() {
            e.id.clone()
        } else {
            e.name.clone()
        };
        let bi = b.add(
            &e.id,
            Kind::External,
            "repository",
            None,
            body,
            a,
            Vec3::new(bx + bw * 0.5, y + bh + 0.03, body.max.z),
            Anchor::Above,
            0.6,
            &e.status,
            name.clone(),
            "repository".into(),
        );
        {
            let n = &mut b.l.nodes[bi];
            n.facing = Vec3::Z;
            n.title = format!("{name} · public library");
            let parts = problems(
                &e.status,
                "smouldering",
                "torn spine",
                Some("left lying open"),
                "key tucked between the pages",
                0,
            );
            n.meaning = format!(
                "book (repository) — {}",
                join(parts, "spine intact — no open findings")
            );
        }
        let g = &mut b.l.geo;
        let spine = e.status.level.rgb();
        let dark = [spine[0] * 0.45, spine[1] * 0.45, spine[2] * 0.45];
        solid(g, &body, dark, a);
        // Gilded bands on the spine, in the status colour.
        for yy in [y + 0.04, y + bh - 0.05] {
            g.face(
                &Aabb::new(
                    Vec3::new(bx, yy, body.max.z),
                    Vec3::new(bx + bw, yy + 0.015, body.max.z + 0.002),
                ),
                rgba(spine, 1.0),
                a,
                EMISSIVE,
            );
        }
        g.box_edges(&body, INK, 0.8, Lod::far(body.center(), 9.0), 0);
        let det = Lod::far(body.center(), 12.0);
        if p2 {
            g.polyline(
                &[
                    Vec3::new(bx, y + bh * 0.7, body.max.z + 0.003),
                    Vec3::new(bx + bw * 0.6, y + bh * 0.55, body.max.z + 0.003),
                    Vec3::new(bx + bw * 0.2, y + bh * 0.45, body.max.z + 0.003),
                    Vec3::new(bx + bw, y + bh * 0.3, body.max.z + 0.003),
                ],
                false,
                [0.95, 0.9, 0.8, 0.9],
                1.2,
                det,
                0,
            );
        }
        if e.status.misconfigs() > 0 {
            let open = Aabb::new(
                Vec3::new(bx - 0.1, y, zs + 0.42),
                Vec3::new(bx + 0.3, y + 0.02, zs + 0.62),
            );
            g.face(&open, rgba([0.85, 0.82, 0.72], 1.0), a, EMISSIVE);
            g.line(
                Vec3::new(bx + 0.1, y + 0.021, zs + 0.42),
                Vec3::new(bx + 0.1, y + 0.021, zs + 0.62),
                INK,
                1.0,
                det,
                0,
            );
        }
        if e.status.secrets() > 0 {
            key(
                g,
                Vec3::new(bx + bw * 0.5, y + bh + 0.02, body.max.z + 0.01),
                0.07,
                det,
            );
        }
        if e.status.count(Level::P1) > 0 {
            fire(
                g,
                city,
                Vec3::new(bx + bw * 0.5, y + bh, body.max.z - 0.05),
                0.35,
                60 + i as u32,
                Lod::ALWAYS,
            );
        }
        let minor = e.status.count(Level::P3) + e.status.count(Level::P4);
        if minor > 0 {
            g.face(
                &Aabb::new(
                    Vec3::new(bx, y, body.max.z + 0.001),
                    Vec3::new(
                        bx + bw,
                        y + bh * (0.3 + 0.1 * minor.min(6) as f32),
                        body.max.z + 0.002,
                    ),
                ),
                [0.0, 0.0, 0.0, 0.45],
                det,
                0,
            );
        }
    }
}

fn square(b: &mut Builder, city: &mut City, scene: &Scene) {
    // Domains no building claims hang their neon on a plain door in the square.
    let claimed: Vec<String> = scene
        .hosts
        .iter()
        .flat_map(|h| {
            h.groups
                .iter()
                .flat_map(|g| g.domains.iter().map(|d| d.to_ascii_lowercase()))
        })
        .collect();
    let unmatched: Vec<_> = scene
        .external
        .iter()
        .filter(|e| e.kind == "domain" && !claimed.contains(&e.name.to_ascii_lowercase()))
        .collect();
    let a = Lod::ALWAYS;
    if !unmatched.is_empty() {
        // A wall on the south side of the square facing north, built facing +z and flipped.
        let x = -24.0;
        let w = 3.0;
        let n = unmatched.len();
        let hgt = 3.2 + n as f32 * 0.8;
        b.l.geo.flip = Some(Vec3::new(x + w * 0.5, 0.0, 9.0));
        let zf = 9.0 - (14.0 - 9.0); // local front so that, flipped, the door faces the square at z = 14
        let wall = Aabb::new(
            Vec3::new(x - 0.5, 0.0, zf - 0.6),
            Vec3::new(x + w + 0.5, hgt, zf),
        );
        {
            let g = &mut b.l.geo;
            solid(g, &wall, [0.09, 0.09, 0.1], a);
            g.box_edges(&wall, INK, 1.2, a, 0);
        }
        city.collide(&b.l.geo, &wall);
        city.furniture.push(("door", b.l.geo.tb(&wall)));
        for (k, e) in unmatched.iter().enumerate() {
            let sx = x + 0.8;
            let y = 3.0 + k as f32 * 0.8;
            let sign = Aabb::new(
                Vec3::new(sx - 0.25, y, zf),
                Vec3::new(sx + 1.45, y + 0.6, zf + 0.2),
            );
            let name = if e.name.is_empty() {
                e.id.clone()
            } else {
                e.name.clone()
            };
            let i = b.add(
                &e.id,
                Kind::External,
                "domain",
                None,
                sign,
                a,
                Vec3::new(sx + 0.6, y + 0.3, zf + 0.25),
                Anchor::Center,
                1.4,
                &e.status,
                name.clone(),
                "unclaimed domain".into(),
            );
            {
                let nd = &mut b.l.nodes[i];
                nd.title = format!("{name} · door in the square");
                let parts = problems(
                    &e.status,
                    "sign on fire",
                    "sign smashed",
                    Some("door left open"),
                    "key left in the lock",
                    0,
                );
                nd.meaning = format!(
                    "no app serves this hostname — {}",
                    join(parts, "neon on, no open findings")
                );
            }
            let g = &mut b.l.geo;
            let col = neon(e.status.level);
            let fl = if e.status.stale || e.status.level == Level::Unknown {
                BLINK
            } else {
                0
            };
            solid(g, &sign, [0.02, 0.02, 0.025], a);
            glow_rect_z(
                g,
                sign.min.x + 0.06,
                sign.min.y + 0.06,
                sign.max.x - 0.06,
                sign.max.y - 0.06,
                sign.max.z + 0.01,
                col,
                3.0,
                a,
                fl,
            );
            g.led(
                Vec3::new(sign.center().x, sign.center().y, sign.max.z + 0.05),
                0.28,
                col,
                a,
                fl,
                3.0,
            );
            city.light(g, sign.center() + Vec3::Z * 0.4, 5.0, col, 0.7);
        }
        let g = &mut b.l.geo;
        let open = unmatched.iter().any(|e| e.status.misconfigs() > 0);
        let door = Aabb::new(
            Vec3::new(x + 0.2, 0.08, zf),
            Vec3::new(x + 1.4, 2.5, zf + 0.08),
        );
        if open {
            g.face(&door, rgba(LIT, 0.85), a, EMISSIVE);
            let leaf = Aabb::new(
                Vec3::new(x + 1.34, 0.08, zf + 0.08),
                Vec3::new(x + 1.4, 2.5, zf + 1.2),
            );
            solid(g, &leaf, [0.2, 0.15, 0.11], a);
        } else {
            solid(g, &door, [0.2, 0.15, 0.11], a);
        }
        g.box_edges(&door, INK, 1.0, a, 0);
        if unmatched.iter().any(|e| e.status.secrets() > 0) {
            key(g, Vec3::new(x + 1.25, 1.3, zf + 0.1), 0.11, a);
        }
        b.l.geo.flip = None;
    }
    // Other external assets (hosts, clusters, unknown kinds) are kiosks along the square.
    let others: Vec<_> = scene
        .external
        .iter()
        .filter(|e| {
            !matches!(
                e.kind.as_str(),
                "repository" | "domain" | "registry" | "image"
            )
        })
        .collect();
    for (k, e) in others.iter().enumerate() {
        let x = -36.0 + (k % 6) as f32 * 4.0;
        let z = 16.0 + (k / 6) as f32 * 4.0;
        let body = Aabb::new(Vec3::new(x, 0.0, z), Vec3::new(x + 2.2, 2.8, z + 2.0));
        let name = if e.name.is_empty() {
            e.id.clone()
        } else {
            e.name.clone()
        };
        let kind = if e.kind.is_empty() {
            "external".to_string()
        } else {
            e.kind.clone()
        };
        let i = b.add(
            &e.id,
            Kind::External,
            &kind,
            None,
            body,
            a,
            Vec3::new(x + 1.1, 3.1, z),
            Anchor::Above,
            2.0,
            &e.status,
            name.clone(),
            kind.clone(),
        );
        {
            let nd = &mut b.l.nodes[i];
            nd.facing = Vec3::NEG_Z;
            nd.title = format!("{name} · kiosk");
            nd.meaning = format!(
                "kiosk ({kind}) — {}",
                join(
                    problems(
                        &e.status,
                        "on fire",
                        "window broken",
                        Some("hatch left open"),
                        "key in the lock",
                        0
                    ),
                    "lit, no open findings"
                )
            );
        }
        let g = &mut b.l.geo;
        solid(g, &body, [0.1, 0.09, 0.09], a);
        g.box_edges(&body, INK, 1.1, a, 0);
        let win = Aabb::new(
            Vec3::new(x + 0.4, 1.1, z - 0.02),
            Vec3::new(x + 1.8, 2.1, z),
        );
        g.face(
            &win,
            rgba(
                if e.status.level == Level::Unknown {
                    DARK_GLASS
                } else {
                    LIT
                },
                1.0,
            ),
            a,
            EMISSIVE,
        );
        g.led(
            Vec3::new(x + 1.1, 2.5, z - 0.05),
            0.12,
            e.status.level.rgb(),
            a,
            if e.status.stale { BLINK } else { 0 },
            2.5,
        );
        city.colliders.push(Rect::of(&body));
    }
}

#[allow(clippy::too_many_arguments)]
fn harbour(
    b: &mut Builder,
    city: &mut City,
    scene: &Scene,
    images: &[crate::scene::InventoryImage],
    x0: f32,
    w: f32,
    d: f32,
    water_x: f32,
) {
    let a = Lod::ALWAYS;
    let zf = -FRONT;
    let zb = zf - d;
    let x1 = x0 + w;
    let h = 7.0;
    city.furniture.push((
        "warehouse",
        Aabb::new(Vec3::new(x0, 0.0, zb), Vec3::new(x1, h, zf)),
    ));
    {
        let g = &mut b.l.geo;
        let walls = [
            Aabb::new(Vec3::new(x0, 0.0, zb), Vec3::new(x0 + 0.4, h, zf)),
            Aabb::new(Vec3::new(x1 - 0.4, 0.0, zb), Vec3::new(x1, h, zf)),
            Aabb::new(Vec3::new(x0, 0.0, zb), Vec3::new(x1, h, zb + 0.4)),
            Aabb::new(Vec3::new(x0, h - 0.4, zb), Vec3::new(x1, h + 0.6, zf)),
            Aabb::new(Vec3::new(x0, 4.6, zf - 0.4), Vec3::new(x1, h, zf)),
        ];
        for wl in &walls {
            solid(g, wl, [0.1, 0.085, 0.075], a);
            g.box_edges(wl, INK, 1.2, a, 0);
            city.blockers.push(*wl);
        }
        for wl in &walls[..3] {
            city.colliders.push(Rect::of(wl));
        }
        // Corrugation on the front lintel and the sign.
        let mut x = x0 + 0.3;
        while x < x1 {
            g.line(
                Vec3::new(x, 4.6, zf + 0.002),
                Vec3::new(x, h, zf + 0.002),
                [0.0, 0.0, 0.0, 0.5],
                1.0,
                Lod::far(Vec3::new(x, 5.0, zf), 40.0),
                0,
            );
            x += 0.35;
        }
        let sw = sign_width("HARBOUR", 0.7);
        sign_text(
            g,
            "HARBOUR",
            (x0 + x1) * 0.5 - sw * 0.5,
            5.3,
            zf + 0.05,
            0.7,
            [0.4, 0.85, 1.0],
            a,
        );
        // Work lights inside so the crates are readable.
        for k in 0..2 {
            let lx = x0 + w * (0.3 + 0.4 * k as f32);
            g.led(
                Vec3::new(lx, h - 1.0, zb + d * 0.5),
                0.5,
                [0.9, 0.85, 0.7],
                a,
                0,
                3.0,
            );
            city.light(
                g,
                Vec3::new(lx, h - 1.2, zb + d * 0.5),
                10.0,
                [0.9, 0.85, 0.7],
                0.7,
            );
        }
    }
    // Crates: rows from the back, stacks of two.
    for (i, im) in images.iter().enumerate() {
        let col = i / 2;
        let level = i % 2;
        let row = col / 8;
        let c = col % 8;
        let cx = x0 + 2.0 + c as f32 * 1.8;
        let cz = zb + 1.2 + row as f32 * 3.0;
        let p2 = im.status.count(Level::P2) > 0;
        let shrink = if p2 { 0.08 } else { 0.0 };
        let y = 0.08 + level as f32 * 1.02;
        let body = Aabb::new(
            Vec3::new(cx + shrink, y, cz),
            Vec3::new(cx + 1.4 - shrink, y + 1.0 - shrink, cz + 1.1),
        );
        let name = if im.name.is_empty() {
            im.id.clone()
        } else {
            im.name.clone()
        };
        let ci = b.add(
            &im.id,
            Kind::External,
            "image",
            None,
            body,
            a,
            Vec3::new(cx + 0.7, y + 1.05, cz + 1.1),
            Anchor::Above,
            1.4,
            &im.status,
            name.clone(),
            if im.running {
                "running".into()
            } else {
                "idle".into()
            },
        );
        {
            let nd = &mut b.l.nodes[ci];
            nd.facing = Vec3::Z;
            nd.title = format!("{name} · harbour");
            let head = if im.running {
                "stamped crate — image is running"
            } else {
                "dusty crate — image not running anywhere"
            };
            let parts = problems(
                &im.status,
                "crate on fire",
                "crate dented",
                Some("lid left open"),
                "key left in the padlock",
                0,
            );
            nd.meaning = format!("{head}; {}", join(parts, "no open findings"));
        }
        let g = &mut b.l.geo;
        let wood = if im.running {
            [0.38, 0.23, 0.11]
        } else {
            [0.3, 0.29, 0.27]
        };
        solid(g, &body, wood, a);
        g.box_edges(&body, INK, 1.0, a, 0);
        let det = Lod::far(body.center(), 22.0);
        let zf2 = body.max.z + 0.003;
        // Planks.
        for k in 1..4 {
            let yy = body.min.y + (body.max.y - body.min.y) * k as f32 / 4.0;
            g.line(
                Vec3::new(body.min.x, yy, zf2),
                Vec3::new(body.max.x, yy, zf2),
                [0.0, 0.0, 0.0, 0.5],
                0.8,
                det,
                0,
            );
        }
        if im.running {
            // Customs stamp in the status colour.
            let sc = im.status.level.rgb();
            g.rect_z(
                body.min.x + 0.35,
                body.min.y + 0.3,
                body.max.x - 0.35,
                body.max.y - 0.3,
                zf2 + 0.001,
                rgba(sc, 0.9),
                1.6,
                det,
                GLOW,
            );
            g.line(
                Vec3::new(body.min.x + 0.4, body.min.y + 0.35, zf2 + 0.001),
                Vec3::new(body.max.x - 0.4, body.max.y - 0.35, zf2 + 0.001),
                rgba(sc, 0.9),
                1.4,
                det,
                GLOW,
            );
        } else {
            // Dust on top.
            g.face(
                &Aabb::new(
                    Vec3::new(body.min.x, body.max.y, body.min.z),
                    Vec3::new(body.max.x, body.max.y + 0.01, body.max.z),
                ),
                rgba([0.42, 0.4, 0.38], 1.0),
                a,
                0,
            );
            g.led(
                Vec3::new(body.max.x - 0.2, body.min.y + 0.2, zf2 + 0.01),
                0.08,
                im.status.level.rgb(),
                det,
                0,
                1.5,
            );
        }
        if p2 {
            g.polyline(
                &[
                    Vec3::new(body.min.x + 0.2, body.max.y - 0.1, zf2 + 0.002),
                    Vec3::new(body.min.x + 0.5, body.max.y - 0.4, zf2 + 0.002),
                    Vec3::new(body.min.x + 0.8, body.max.y - 0.15, zf2 + 0.002),
                ],
                false,
                INK,
                2.0,
                det,
                0,
            );
        }
        if im.status.misconfigs() > 0 {
            let lid = Aabb::new(
                Vec3::new(body.min.x, body.max.y, body.min.z - 0.05),
                Vec3::new(body.max.x, body.max.y + 0.9, body.min.z + 0.03),
            );
            solid(g, &lid, wood, a);
            g.box_edges(&lid, INK, 1.0, a, 0);
        }
        if im.status.secrets() > 0 {
            let pl = Aabb::new(
                Vec3::new(body.max.x - 0.3, body.min.y + 0.4, zf2),
                Vec3::new(body.max.x - 0.18, body.min.y + 0.55, zf2 + 0.04),
            );
            solid(g, &pl, [0.5, 0.45, 0.3], a);
            key(
                g,
                Vec3::new(body.max.x - 0.24, body.min.y + 0.45, zf2 + 0.05),
                0.09,
                det,
            );
        }
        if im.status.count(Level::P1) > 0 {
            // Out of the front planks (another crate may be stacked on top).
            fire(
                g,
                city,
                Vec3::new(body.center().x, body.max.y - 0.35, body.max.z + 0.15),
                0.8,
                90 + i as u32,
                Lod::ALWAYS,
            );
        }
        let minor = im.status.count(Level::P3) + im.status.count(Level::P4);
        if minor > 0 {
            for k in 0..minor.min(5) {
                let gx = body.min.x + 0.1 + hash(i as u32 * 7 + k) * 1.0;
                let gy = body.min.y + 0.1 + hash(i as u32 * 11 + k) * 0.7;
                g.face(
                    &Aabb::new(
                        Vec3::new(gx, gy, zf2),
                        Vec3::new(gx + 0.15, gy + 0.12, zf2 + 0.002),
                    ),
                    [0.0, 0.0, 0.0, 0.5],
                    det,
                    0,
                );
            }
        }
        if level == 0 {
            city.colliders.push(Rect::of(&body));
        }
    }
    // Registries: cranes on the quay edge.
    let regs: Vec<_> = scene
        .external
        .iter()
        .filter(|e| e.kind == "registry")
        .collect();
    for (k, e) in regs.iter().enumerate() {
        let x = water_x - 2.5;
        let z = 4.0 + k as f32 * 9.0;
        let tower = Aabb::new(
            Vec3::new(x - 0.5, 0.0, z - 0.5),
            Vec3::new(x + 0.5, 14.0, z + 0.5),
        );
        let name = if e.name.is_empty() {
            e.id.clone()
        } else {
            e.name.clone()
        };
        let i = b.add(
            &e.id,
            Kind::External,
            "registry",
            None,
            tower,
            a,
            Vec3::new(x, 14.6, z),
            Anchor::Above,
            3.0,
            &e.status,
            name.clone(),
            "registry".into(),
        );
        {
            let nd = &mut b.l.nodes[i];
            nd.facing = Vec3::ZERO;
            nd.title = format!("{name} · harbour crane");
            nd.meaning = format!(
                "crane (registry) loading the crates — {}",
                join(
                    problems(
                        &e.status,
                        "crane on fire",
                        "jib bent",
                        Some("cab left open"),
                        "key left in the cab",
                        0
                    ),
                    "warning light steady, no open findings"
                )
            );
        }
        let g = &mut b.l.geo;
        solid(g, &tower, [0.35, 0.22, 0.08], a);
        g.box_edges(&tower, INK, 1.2, a, 0);
        for s in 0..14 {
            let y = s as f32;
            g.line(
                Vec3::new(x - 0.5, y, z + 0.51),
                Vec3::new(x + 0.5, y + 1.0, z + 0.51),
                [0.0, 0.0, 0.0, 0.6],
                1.0,
                Lod::far(tower.center(), 50.0),
                0,
            );
        }
        let jib = Aabb::new(
            Vec3::new(x - 6.0, 13.0, z - 0.3),
            Vec3::new(x + 12.0, 13.8, z + 0.3),
        );
        solid(g, &jib, [0.35, 0.22, 0.08], a);
        g.box_edges(&jib, INK, 1.1, a, 0);
        g.line(
            Vec3::new(x + 9.0, 13.0, z),
            Vec3::new(x + 9.0, 4.0, z),
            [0.05, 0.05, 0.05, 0.9],
            1.4,
            a,
            0,
        );
        g.led(
            Vec3::new(x, 14.3, z),
            0.35,
            e.status.level.rgb(),
            a,
            if e.status.stale {
                BLINK
            } else {
                ALERT * (e.status.level == Level::P1) as u32
            },
            4.0,
        );
        city.colliders.push(Rect::of(&tower));
        if e.status.count(Level::P1) > 0 {
            fire(
                g,
                city,
                Vec3::new(x, 13.9, z),
                1.4,
                120 + k as u32,
                Lod::ALWAYS,
            );
        }
    }
}

fn searchlights(b: &mut Builder, city: &mut City, scene: &Scene, plans: &[Option<Plan>]) {
    if scene.sources.is_empty() {
        return;
    }
    let now = epoch_days(&scene.generated_at);
    // Roofs, tallest first; without hosts the library roof carries them.
    let mut roofs: Vec<(Vec3, f32)> = Vec::new();
    let mut order: Vec<usize> = (0..plans.len()).collect();
    order.sort_by(|&a, &b| {
        plans[b]
            .as_ref()
            .unwrap()
            .height
            .total_cmp(&plans[a].as_ref().unwrap().height)
    });
    for &hi in &order {
        let p = plans[hi].as_ref().unwrap();
        let s = if p.side == 1 { 1.0 } else { -1.0 };
        let z = s * (FRONT + DEPTH * 0.5);
        for k in 0..3 {
            roofs.push((
                Vec3::new(p.x0 + p.width * (0.25 + 0.25 * k as f32), p.height, z),
                hash(hi as u32 * 3 + k),
            ));
        }
    }
    if roofs.is_empty() {
        roofs.push((Vec3::new(-22.0, 8.6, -12.0), 0.2));
        roofs.push((Vec3::new(-16.0, 8.6, -12.0), 0.6));
    }
    let a = Lod::ALWAYS;
    for (k, src) in scene.sources.iter().enumerate() {
        let (base, ph) = roofs[k % roofs.len()];
        let base = base + Vec3::new(0.0, 0.0, (k / roofs.len()) as f32 * 1.5);
        let housing = Aabb::new(
            base + Vec3::new(-0.45, 0.0, -0.45),
            base + Vec3::new(0.45, 1.2, 0.45),
        );
        let on = !src.stale && !src.last_seen_at.is_empty();
        let st = Status {
            level: if on { Level::Ok } else { Level::Unknown },
            stale: !on,
            ..Default::default()
        };
        let id = format!("source:{}", src.name);
        let i = b.add(
            &id,
            Kind::External,
            "source",
            None,
            housing,
            a,
            base + Vec3::Y * 1.6,
            Anchor::Above,
            1.0,
            &st,
            src.name.clone(),
            "scan source".into(),
        );
        let ago = match (now, epoch_days(&src.last_seen_at)) {
            (Some(n), Some(t)) => {
                let d = n - t;
                if d >= 2.0 {
                    format!("{:.0} days ago", d.floor())
                } else if d * 24.0 >= 1.0 {
                    format!("{:.0} h ago", (d * 24.0).floor())
                } else {
                    "just now".into()
                }
            }
            _ => String::new(),
        };
        {
            let nd = &mut b.l.nodes[i];
            nd.facing = Vec3::ZERO;
            nd.title = format!("{} · scan source", src.name);
            nd.meaning = if src.last_seen_at.is_empty() {
                "searchlight off — never reported".into()
            } else if on {
                format!("searchlight sweeping — last report {ago}")
            } else {
                format!("searchlight off — source quiet, last report {ago}")
            };
        }
        let g = &mut b.l.geo;
        solid(g, &housing, [0.12, 0.12, 0.13], a);
        g.box_edges(&housing, INK, 1.0, a, 0);
        if on {
            g.led(base + Vec3::Y * 1.3, 0.6, [0.85, 0.9, 1.0], a, 0, 4.0);
        }
        city.beams.push(Beam {
            node: i,
            origin: base + Vec3::Y * 1.3,
            phase: ph * std::f32::consts::TAU + k as f32 * 2.1,
            on,
        });
    }
}

/// Direction of a searchlight beam at time `now_ms`.
pub fn beam_dir(beam: &Beam, now_ms: f64, still: bool) -> Vec3 {
    let t = if still {
        0.0
    } else {
        (now_ms % 1.0e7) as f32 / 1000.0
    };
    let a = beam.phase + t * 0.22;
    let el = 1.0 + 0.18 * (t * 0.31 + beam.phase).sin();
    Vec3::new(a.cos() * el.cos(), el.sin(), a.sin() * el.cos())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Scene {
        Scene::parse(include_str!("../fixtures/scene.json")).unwrap()
    }

    #[test]
    fn every_scene_element_is_in_the_city() {
        let s = fixture();
        let l = build_city(&s);
        let city = l.room.as_ref().unwrap();
        for h in &s.hosts {
            assert!(l.index.contains_key(&h.id));
            for g in &h.groups {
                assert!(l.index.contains_key(&g.id), "{}", g.id);
                for w in &g.workloads {
                    assert!(l.index.contains_key(&w.id), "{}", w.id);
                }
            }
        }
        for e in &s.external {
            assert!(l.index.contains_key(&e.id), "{} {}", e.kind, e.id);
        }
        for im in &s.images {
            assert!(l.index.contains_key(&im.id));
        }
        assert_eq!(city.beams.len(), 3);
        assert_eq!(city.beams.iter().filter(|b| !b.on).count(), 1);
        assert!(l.animated);
        // Building height follows its apps; doors exist for the routed domains.
        let host = &l.nodes[l.index["host:12"]];
        assert!((host.body.max.y - (LOBBY_H + 15.0 * FLOOR_H + 0.9)).abs() < 1e-3);
        let door = &l.nodes[l.index["asset:7"]];
        assert_eq!(door.kind_name, "domain");
        assert!(door.body.max.y < 4.0, "slowbloom.app is a street door");
        // The spawn point is free.
        let sp = Vec2::new(city.spawn.x, city.spawn.z);
        assert!(!crate::walk::blocked(sp, &city.colliders));
    }

    #[test]
    fn meanings_explain_the_metaphor() {
        let l = build_city(&fixture());
        let m = |id: &str| l.nodes[l.index[id]].meaning.clone();
        assert!(
            m("wl:slowbloom/ghost/Deployment/ghost")
                .contains("window left open — 2 misconfigurations"),
            "{}",
            m("wl:slowbloom/ghost/Deployment/ghost")
        );
        assert!(m("wl:slowbloom/ghost/Deployment/ghost").contains("window broken — 2 P2 findings"));
        assert!(m("wl:slowbloom/slowbloom/Deployment/web").contains("key left in the latch"));
        assert!(m("wl:slowbloom/argocd/Deployment/argocd-repo-server")
            .contains("1 pane dark — 1/2 ready"));
        assert!(m("asset:5").contains("key tucked"));
        assert!(
            m("source:github-actions").contains("off")
                && m("source:github-actions").contains("9 days ago"),
            "{}",
            m("source:github-actions")
        );
        assert!(m("asset:8").contains("door left open"));
        assert!(m("wl:slowbloom/cert-manager/Deployment/cert-manager").starts_with("warm light"));
    }

    #[test]
    fn many_hosts_line_the_street_without_overlap() {
        let mut hosts = Vec::new();
        for i in 0..20 {
            let cl = if i < 12 { "a" } else { "b" };
            hosts.push(format!(r#"{{"id":"h{i}","name":"h{i}","cluster":"{cl}","status":{{"level":"ok"}},"groups":[{{"id":"g{i}","name":"n","domains":["d{i}.example"],"workloads":[{{"id":"w{i}","ready":1,"desired":1}}]}}]}}"#));
        }
        let s = Scene::parse(&format!(r#"{{"hosts":[{}],"external":[],"images":[],"sources":[{{"name":"x","lastSeenAt":null,"stale":false}}]}}"#, hosts.join(","))).unwrap();
        let l = build_city(&s);
        let b: Vec<Aabb> = l.hosts.iter().map(|&i| l.nodes[i].body).collect();
        assert_eq!(b.len(), 20);
        for i in 0..20 {
            for j in i + 1..20 {
                let (p, q) = (&b[i], &b[j]);
                assert!(
                    !(p.min.x < q.max.x
                        && q.min.x < p.max.x
                        && p.min.z < q.max.z
                        && q.min.z < p.max.z),
                    "{i} overlaps {j}"
                );
            }
            assert!(
                b[i].max.z <= -FRONT + 1e-3 || b[i].min.z >= FRONT - 1e-3,
                "building {i} stands in the street"
            );
        }
        // South-side windows face the street too.
        let w = l.index["w1"];
        assert_eq!(l.nodes[w].facing, Vec3::NEG_Z);
        let empty = build_city(&Scene::default());
        assert!(empty.room.is_some());
    }
}
