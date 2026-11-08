//! Orbit camera with eased fly-to.

use glam::{Mat4, Vec3, Vec4Swizzles};

use crate::geo::Aabb;

pub const MIN_PITCH: f32 = 0.06; // ~3.5 degrees: the floor is never seen from below
pub const MAX_PITCH: f32 = 1.45;
pub const MIN_DIST: f32 = 0.12;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    pub target: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub dist: f32,
}

impl Pose {
    pub fn eye(&self) -> Vec3 {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        self.target + Vec3::new(cp * sy, sp, cp * cy) * self.dist
    }
}

#[derive(Clone, Debug)]
pub struct Camera {
    pub pose: Pose,
    pub fovy: f32,
    pub aspect: f32,
    pub max_dist: f32,
    /// First-person override (eye, forward). The orbit pose is kept untouched underneath.
    pub fps: Option<(Vec3, Vec3)>,
    flight: Option<(Pose, Pose, f64, f64)>,
}

fn ease(t: f32) -> f32 {
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(3) * 0.5
    }
}

fn wrap_angle(a: f32) -> f32 {
    let tau = std::f32::consts::TAU;
    let mut a = a % tau;
    if a > std::f32::consts::PI {
        a -= tau;
    } else if a < -std::f32::consts::PI {
        a += tau;
    }
    a
}

impl Default for Camera {
    fn default() -> Self {
        Camera {
            pose: Pose { target: Vec3::new(0.0, 1.0, 0.0), yaw: 0.6, pitch: 0.45, dist: 6.0 },
            fovy: 38f32.to_radians(),
            aspect: 1.5,
            max_dist: 60.0,
            fps: None,
            flight: None,
        }
    }
}

impl Camera {
    pub fn eye(&self) -> Vec3 {
        match self.fps {
            Some((e, _)) => e,
            None => self.pose.eye(),
        }
    }

    /// Vertical field of view in use (wider when walking).
    pub fn fov(&self) -> f32 {
        if self.fps.is_some() {
            64f32.to_radians()
        } else {
            self.fovy
        }
    }

    /// Fog start/end distances.
    pub fn fog(&self, scene_radius: f32) -> (f32, f32) {
        match self.fps {
            Some(_) => (10.0, 150.0),
            None => {
                let d = self.pose.dist;
                (d * 0.9 + 0.5, d * 2.6 + scene_radius * 1.5 + 2.0)
            }
        }
    }

    pub fn near_far(&self) -> (f32, f32) {
        if self.fps.is_some() {
            return (0.05, 400.0);
        }
        let near = (self.pose.dist * 0.02).clamp(0.005, 0.5);
        (near, self.pose.dist * 6.0 + self.max_dist * 2.0 + 20.0)
    }

    pub fn view(&self) -> Mat4 {
        match self.fps {
            Some((e, f)) => Mat4::look_at_rh(e, e + f, Vec3::Y),
            None => Mat4::look_at_rh(self.eye(), self.pose.target, Vec3::Y),
        }
    }

    pub fn proj(&self) -> Mat4 {
        let (n, f) = self.near_far();
        Mat4::perspective_rh_gl(self.fov(), self.aspect.max(0.01), n, f)
    }

    pub fn view_proj(&self) -> Mat4 {
        self.proj() * self.view()
    }

    /// World ray through a CSS-pixel position.
    pub fn ray(&self, x: f32, y: f32, w: f32, h: f32) -> (Vec3, Vec3) {
        let nx = x / w.max(1.0) * 2.0 - 1.0;
        let ny = 1.0 - y / h.max(1.0) * 2.0;
        let inv = self.view_proj().inverse();
        let a = inv * glam::Vec4::new(nx, ny, -1.0, 1.0);
        let b = inv * glam::Vec4::new(nx, ny, 1.0, 1.0);
        let a = a.xyz() / a.w;
        let b = b.xyz() / b.w;
        (a, (b - a).normalize_or(Vec3::NEG_Z))
    }

    /// Distance at which a sphere of `radius` fits the view.
    pub fn fit_distance(&self, radius: f32) -> f32 {
        let half_v = self.fovy * 0.5;
        let half_h = (half_v.tan() * self.aspect.max(0.2)).atan();
        (radius / half_v.min(half_h).sin()).max(MIN_DIST)
    }

    pub fn clamp(p: &mut Pose, max_dist: f32) {
        p.pitch = p.pitch.clamp(MIN_PITCH, MAX_PITCH);
        p.dist = p.dist.clamp(MIN_DIST, max_dist.max(MIN_DIST * 2.0));
        p.target.y = p.target.y.max(0.0);
        p.yaw = wrap_angle(p.yaw);
    }

    pub fn flying(&self) -> bool {
        self.flight.is_some()
    }

    pub fn fly_to(&mut self, mut to: Pose, now: f64, duration_ms: f64) {
        Camera::clamp(&mut to, self.max_dist);
        if duration_ms <= 0.0 {
            self.pose = to;
            self.flight = None;
            return;
        }
        // Take the short way round.
        to.yaw = self.pose.yaw + wrap_angle(to.yaw - self.pose.yaw);
        self.flight = Some((self.pose, to, now, duration_ms));
    }

    pub fn stop(&mut self) {
        self.flight = None;
    }

    /// The pose a flight ends in (or the current pose).
    pub fn destination(&self) -> Pose {
        self.flight.map(|f| f.1).unwrap_or(self.pose)
    }

    /// Advance a running flight. Returns true when the pose changed.
    pub fn update(&mut self, now: f64) -> bool {
        let Some((from, to, t0, dur)) = self.flight else { return false };
        let t = (((now - t0) / dur) as f32).clamp(0.0, 1.0);
        let e = ease(t);
        // Arc out slightly on long flights so the motion reads as a camera move, not a zoom.
        let span = from.target.distance(to.target);
        let lift = 1.0 + (span / (from.dist + to.dist + 1e-3)).min(1.0) * 0.35 * (std::f32::consts::PI * e).sin();
        self.pose = Pose {
            target: from.target.lerp(to.target, e),
            yaw: from.yaw + (to.yaw - from.yaw) * e,
            pitch: from.pitch + (to.pitch - from.pitch) * e,
            dist: (from.dist.ln() + (to.dist.ln() - from.dist.ln()) * e).exp() * lift,
        };
        if t >= 1.0 {
            self.pose = to;
            self.pose.yaw = wrap_angle(self.pose.yaw);
            self.flight = None;
        }
        true
    }

    /// Pose looking at the box centre from (yaw, pitch), as close as possible while every corner
    /// stays within `fill` of the viewport (in NDC, 1.0 = edge).
    pub fn frame_box(&self, b: &Aabb, yaw: f32, pitch: f32, fill: f32) -> Pose {
        let mut pose = Pose { target: b.center(), yaw, pitch, dist: 1.0 };
        let fits = |dist: f32| -> bool {
            let mut cam = self.clone();
            cam.flight = None;
            cam.fps = None;
            cam.pose = Pose { dist, ..pose };
            let vp = cam.view_proj();
            (0..8u8).all(|i| {
                let c = Vec3::new(
                    if i & 1 != 0 { b.max.x } else { b.min.x },
                    if i & 2 != 0 { b.max.y } else { b.min.y },
                    if i & 4 != 0 { b.max.z } else { b.min.z },
                );
                let p = vp * c.extend(1.0);
                p.w > 1e-4 && (p.x / p.w).abs() <= fill && (p.y / p.w).abs() <= fill
            })
        };
        let (mut lo, mut hi) = (MIN_DIST, self.fit_distance(b.radius()) * 1.05 + MIN_DIST);
        if fits(lo) {
            hi = lo;
        }
        for _ in 0..24 {
            let mid = (lo + hi) * 0.5;
            if fits(mid) {
                hi = mid;
            } else {
                lo = mid;
            }
        }
        pose.dist = hi;
        pose
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ray_through_center_hits_target() {
        let c = Camera { aspect: 1.0, ..Default::default() };
        let (o, d) = c.ray(50.0, 50.0, 100.0, 100.0);
        let to_target = (c.pose.target - o).normalize();
        assert!(d.dot(to_target) > 0.9999);
    }

    #[test]
    fn flight_ends_at_destination() {
        let mut c = Camera::default();
        let to = Pose { target: Vec3::new(3.0, 1.0, -2.0), yaw: -2.9, pitch: 0.3, dist: 2.0 };
        c.fly_to(to, 0.0, 800.0);
        assert!(c.update(400.0));
        assert!(c.flying());
        c.update(900.0);
        assert!(!c.flying());
        assert!((c.pose.target - to.target).length() < 1e-5);
        assert!((c.pose.dist - 2.0).abs() < 1e-4);
    }

    #[test]
    fn pitch_is_clamped_above_floor() {
        let mut p = Pose { target: Vec3::new(0.0, -3.0, 0.0), yaw: 0.0, pitch: -1.0, dist: 1.0 };
        Camera::clamp(&mut p, 50.0);
        assert!(p.pitch >= MIN_PITCH && p.target.y >= 0.0 && p.eye().y > 0.0);
    }
}
