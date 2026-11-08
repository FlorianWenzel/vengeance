//! Math primitives and the GPU instance buffers (plain `Vec<f32>`s, uploaded by `gl.rs`).

use glam::Vec3;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}

impl Aabb {
    pub fn new(a: Vec3, b: Vec3) -> Aabb {
        Aabb { min: a.min(b), max: a.max(b) }
    }
    pub fn center(&self) -> Vec3 {
        (self.min + self.max) * 0.5
    }
    pub fn size(&self) -> Vec3 {
        self.max - self.min
    }
    pub fn radius(&self) -> f32 {
        self.size().length() * 0.5
    }
    pub fn union(&self, o: &Aabb) -> Aabb {
        Aabb { min: self.min.min(o.min), max: self.max.max(o.max) }
    }
    pub fn grow(&self, d: f32) -> Aabb {
        Aabb { min: self.min - Vec3::splat(d), max: self.max + Vec3::splat(d) }
    }
    pub fn contains(&self, p: Vec3) -> bool {
        p.cmpge(self.min).all() && p.cmple(self.max).all()
    }
    /// Slab test. Returns the entry distance (0 when the origin is inside).
    pub fn ray_hit(&self, origin: Vec3, dir: Vec3) -> Option<f32> {
        let mut tmin = 0.0f32;
        let mut tmax = f32::INFINITY;
        for i in 0..3 {
            let o = origin[i];
            let d = dir[i];
            if d.abs() < 1e-9 {
                if o < self.min[i] || o > self.max[i] {
                    return None;
                }
            } else {
                let inv = 1.0 / d;
                let mut t0 = (self.min[i] - o) * inv;
                let mut t1 = (self.max[i] - o) * inv;
                if t0 > t1 {
                    std::mem::swap(&mut t0, &mut t1);
                }
                tmin = tmin.max(t0);
                tmax = tmax.min(t1);
                if tmin > tmax {
                    return None;
                }
            }
        }
        Some(tmin)
    }
}

pub fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Level-of-detail rule shared by Rust (picking, labels) and the shaders: an element is visible
/// while the eye is closer than `far` to its anchor (fading over the last 20 %), and, when
/// `near > 0`, only while it is farther than `near` (used to cross-fade a coarse stand-in).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lod {
    pub anchor: Vec3,
    pub near: f32,
    pub far: f32,
}

impl Lod {
    pub const ALWAYS: Lod = Lod { anchor: Vec3::ZERO, near: 0.0, far: 0.0 };
    pub fn far(anchor: Vec3, far: f32) -> Lod {
        Lod { anchor, near: 0.0, far }
    }
    pub fn vis(&self, eye: Vec3) -> f32 {
        let d = eye.distance(self.anchor);
        let o = if self.far <= 0.0 { 1.0 } else { 1.0 - smoothstep(self.far * 0.8, self.far, d) };
        let i = if self.near <= 0.0 { 1.0 } else { smoothstep(self.near * 0.8, self.near, d) };
        o * i
    }
}

// Instance flags (decoded in the shaders, keep in sync with gl.rs).
pub const BLINK: u32 = 1;
pub const DASH: u32 = 2;
pub const OCCLUDER: u32 = 4;
pub const HATCH: u32 = 8;
pub const NO_HIDDEN: u32 = 16;
pub const ALERT: u32 = 32;

pub const LINE_STRIDE: usize = 17; // p0 3, p1 3, color 4, anchor 3, near, far, flags, width
pub const FACE_STRIDE: usize = 16; // min 3, size 3, color 4, anchor 3, near, far, flags
pub const LED_STRIDE: usize = 16; // center 3, size, color 4, anchor 3, near, far, flags, min px, pad

pub type Rgba = [f32; 4];

pub fn rgba(rgb: [f32; 3], a: f32) -> Rgba {
    [rgb[0], rgb[1], rgb[2], a]
}

#[derive(Default, Clone, Debug)]
pub struct Geo {
    pub lines: Vec<f32>,
    pub faces: Vec<f32>,
    pub leds: Vec<f32>,
}

impl Geo {
    pub fn clear(&mut self) {
        self.lines.clear();
        self.faces.clear();
        self.leds.clear();
    }
    pub fn line_count(&self) -> usize {
        self.lines.len() / LINE_STRIDE
    }
    pub fn face_count(&self) -> usize {
        self.faces.len() / FACE_STRIDE
    }
    pub fn led_count(&self) -> usize {
        self.leds.len() / LED_STRIDE
    }

    pub fn line(&mut self, a: Vec3, b: Vec3, c: Rgba, width: f32, lod: Lod, flags: u32) {
        self.lines.extend_from_slice(&[
            a.x, a.y, a.z, b.x, b.y, b.z, c[0], c[1], c[2], c[3], lod.anchor.x, lod.anchor.y, lod.anchor.z, lod.near,
            lod.far, flags as f32, width,
        ]);
    }

    pub fn polyline(&mut self, pts: &[Vec3], closed: bool, c: Rgba, width: f32, lod: Lod, flags: u32) {
        for w in pts.windows(2) {
            self.line(w[0], w[1], c, width, lod, flags);
        }
        if closed && pts.len() > 2 {
            self.line(pts[pts.len() - 1], pts[0], c, width, lod, flags);
        }
    }

    pub fn box_edges(&mut self, b: &Aabb, c: Rgba, width: f32, lod: Lod, flags: u32) {
        let (n, x) = (b.min, b.max);
        let p = |i: u8| Vec3::new(if i & 1 != 0 { x.x } else { n.x }, if i & 2 != 0 { x.y } else { n.y }, if i & 4 != 0 { x.z } else { n.z });
        const E: [(u8, u8); 12] = [(0, 1), (2, 3), (4, 5), (6, 7), (0, 2), (1, 3), (4, 6), (5, 7), (0, 4), (1, 5), (2, 6), (3, 7)];
        for (i, j) in E {
            self.line(p(i), p(j), c, width, lod, flags);
        }
    }

    /// Rectangle in the plane z = const (front panels).
    pub fn rect_z(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, z: f32, c: Rgba, width: f32, lod: Lod, flags: u32) {
        let pts = [Vec3::new(x0, y0, z), Vec3::new(x1, y0, z), Vec3::new(x1, y1, z), Vec3::new(x0, y1, z)];
        self.polyline(&pts, true, c, width, lod, flags);
    }

    /// Rectangle in the plane y = const (floor marks, table tops).
    pub fn rect_y(&mut self, x0: f32, z0: f32, x1: f32, z1: f32, y: f32, c: Rgba, width: f32, lod: Lod, flags: u32) {
        let pts = [Vec3::new(x0, y, z0), Vec3::new(x1, y, z0), Vec3::new(x1, y, z1), Vec3::new(x0, y, z1)];
        self.polyline(&pts, true, c, width, lod, flags);
    }

    pub fn face(&mut self, b: &Aabb, c: Rgba, lod: Lod, flags: u32) {
        let s = b.size();
        self.faces.extend_from_slice(&[
            b.min.x, b.min.y, b.min.z, s.x, s.y, s.z, c[0], c[1], c[2], c[3], lod.anchor.x, lod.anchor.y, lod.anchor.z,
            lod.near, lod.far, flags as f32,
        ]);
    }

    pub fn led(&mut self, p: Vec3, size: f32, rgb: [f32; 3], lod: Lod, flags: u32, min_px: f32) {
        self.leds.extend_from_slice(&[
            p.x, p.y, p.z, size, rgb[0], rgb[1], rgb[2], 1.0, lod.anchor.x, lod.anchor.y, lod.anchor.z, lod.near, lod.far,
            flags as f32, min_px, 0.0,
        ]);
    }

    /// A CAD dimension line from `a` to `b` with arrowheads and end ticks, `n` = tick direction.
    pub fn dimension(&mut self, a: Vec3, b: Vec3, n: Vec3, c: Rgba, lod: Lod) {
        let f = NO_HIDDEN;
        self.line(a, b, c, 0.8, lod, f);
        let d = (b - a).normalize_or_zero();
        let ah = 0.035f32.min((b - a).length() * 0.2);
        for (p, s) in [(a, 1.0f32), (b, -1.0f32)] {
            self.line(p, p + (d * s * ah) + n * ah * 0.35, c, 0.8, lod, f);
            self.line(p, p + (d * s * ah) - n * ah * 0.35, c, 0.8, lod, f);
            self.line(p - n * 0.025, p + n * 0.025, c, 0.8, lod, f);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ray_box() {
        let b = Aabb::new(Vec3::ZERO, Vec3::ONE);
        assert_eq!(b.ray_hit(Vec3::new(0.5, 0.5, 5.0), Vec3::new(0.0, 0.0, -1.0)), Some(4.0));
        assert_eq!(b.ray_hit(Vec3::new(2.0, 0.5, 5.0), Vec3::new(0.0, 0.0, -1.0)), None);
        assert_eq!(b.ray_hit(Vec3::new(0.5, 0.5, 0.5), Vec3::new(0.0, 0.0, -1.0)), Some(0.0));
        assert_eq!(b.ray_hit(Vec3::new(0.5, 0.5, 5.0), Vec3::new(0.0, 0.0, 1.0)), None);
    }

    #[test]
    fn lod_fades() {
        let l = Lod::far(Vec3::ZERO, 10.0);
        assert_eq!(l.vis(Vec3::new(0.0, 0.0, 5.0)), 1.0);
        assert_eq!(l.vis(Vec3::new(0.0, 0.0, 11.0)), 0.0);
        let m = l.vis(Vec3::new(0.0, 0.0, 9.0));
        assert!(m > 0.0 && m < 1.0);
        let n = Lod { anchor: Vec3::ZERO, near: 4.0, far: 0.0 };
        assert_eq!(n.vis(Vec3::new(0.0, 0.0, 1.0)), 0.0);
        assert_eq!(n.vis(Vec3::new(0.0, 0.0, 50.0)), 1.0);
    }
}
