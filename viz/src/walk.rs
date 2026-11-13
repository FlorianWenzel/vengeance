//! First-person walker: input state, smooth acceleration, collision against room rectangles.
//!
//! Heading convention (also used by `minimap()`): forward = (sin h, 0, -cos h), so h = 0 looks
//! down -z and positive h turns right (clockwise seen from above, like canvas rotation).

use glam::{Vec2, Vec3};

use crate::city::Rect;

pub const EYE: f32 = 1.68;
pub const RADIUS: f32 = 0.28;
pub const WALK: f32 = 1.4;
pub const RUN: f32 = 3.0;
pub const REACH: f32 = 90.0;
pub const MAX_PITCH: f32 = 1.35;
pub const TURN_RATE: f32 = 1.9; // rad/s for arrow-key turning
pub const LOOK_RATE: f32 = 0.0022; // rad per CSS pixel of mouse movement

pub const FWD: u32 = 1;
pub const BACK: u32 = 2;
pub const LEFT: u32 = 4;
pub const RIGHT: u32 = 8;
pub const TURN_L: u32 = 16;
pub const TURN_R: u32 = 32;
pub const RUN_KEY: u32 = 64;

pub fn key_bit(code: &str) -> u32 {
    match code {
        "KeyW" | "ArrowUp" => FWD,
        "KeyS" | "ArrowDown" => BACK,
        "KeyA" => LEFT,
        "KeyD" => RIGHT,
        "ArrowLeft" => TURN_L,
        "ArrowRight" => TURN_R,
        "ShiftLeft" | "ShiftRight" => RUN_KEY,
        _ => 0,
    }
}

#[derive(Clone, Copy, Debug)]
struct Glide {
    from: (Vec2, f32, f32),
    to: (Vec2, f32, f32),
    t0: f64,
    dur: f64,
}

#[derive(Clone, Debug, Default)]
pub struct Walker {
    /// Feet position on the floor (x, z).
    pub pos: Vec2,
    pub heading: f32,
    pub pitch: f32,
    pub vel: Vec2,
    pub keys: u32,
    bob_phase: f32,
    bob_amp: f32,
    glide: Option<Glide>,
}

pub fn forward(heading: f32, pitch: f32) -> Vec3 {
    let (sh, ch) = heading.sin_cos();
    let (sp, cp) = pitch.sin_cos();
    Vec3::new(sh * cp, sp, -ch * cp)
}

fn wrap(a: f32) -> f32 {
    let tau = std::f32::consts::TAU;
    let mut a = a % tau;
    if a > std::f32::consts::PI {
        a -= tau;
    } else if a < -std::f32::consts::PI {
        a += tau;
    }
    a
}

pub fn blocked(p: Vec2, colliders: &[Rect]) -> bool {
    colliders.iter().any(|c| c.hits(p, RADIUS))
}

impl Walker {
    pub fn spawn(pos: Vec3, heading: f32) -> Walker {
        Walker {
            pos: Vec2::new(pos.x, pos.z),
            heading,
            pitch: -0.06,
            ..Default::default()
        }
    }

    pub fn eye(&self, reduced_motion: bool) -> Vec3 {
        let bob = if reduced_motion {
            0.0
        } else {
            self.bob_amp * (self.bob_phase * 2.0).sin() * 0.028
        };
        let sway = if reduced_motion {
            0.0
        } else {
            self.bob_amp * self.bob_phase.sin() * 0.012
        };
        let right = Vec2::new(self.heading.cos(), self.heading.sin());
        Vec3::new(
            self.pos.x + right.x * sway,
            EYE + bob,
            self.pos.y + right.y * sway,
        )
    }

    pub fn forward(&self) -> Vec3 {
        forward(self.heading, self.pitch)
    }

    pub fn look(&mut self, dx: f32, dy: f32) {
        if !dx.is_finite() || !dy.is_finite() {
            return;
        }
        self.glide = None;
        self.heading = wrap(self.heading + dx.clamp(-500.0, 500.0) * LOOK_RATE);
        self.pitch =
            (self.pitch - dy.clamp(-500.0, 500.0) * LOOK_RATE).clamp(-MAX_PITCH, MAX_PITCH);
    }

    pub fn glide_to(&mut self, pos: Vec2, heading: f32, pitch: f32, now: f64, dur: f64) {
        self.vel = Vec2::ZERO;
        let heading = self.heading + wrap(heading - self.heading);
        if dur <= 0.0 {
            self.pos = pos;
            self.heading = wrap(heading);
            self.pitch = pitch;
            self.glide = None;
        } else {
            self.glide = Some(Glide {
                from: (self.pos, self.heading, self.pitch),
                to: (pos, heading, pitch),
                t0: now,
                dur,
            });
        }
    }

    pub fn gliding(&self) -> bool {
        self.glide.is_some()
    }

    /// Advance by `dt` seconds. Returns true when the view changed.
    pub fn step(&mut self, now: f64, dt: f32, colliders: &[Rect]) -> bool {
        if let Some(g) = self.glide {
            if self.keys & (FWD | BACK | LEFT | RIGHT | TURN_L | TURN_R) != 0 {
                self.glide = None;
            } else {
                let t = (((now - g.t0) / g.dur) as f32).clamp(0.0, 1.0);
                let e = t * t * (3.0 - 2.0 * t);
                self.pos = g.from.0.lerp(g.to.0, e);
                self.heading = g.from.1 + (g.to.1 - g.from.1) * e;
                self.pitch = g.from.2 + (g.to.2 - g.from.2) * e;
                if t >= 1.0 {
                    self.heading = wrap(self.heading);
                    self.glide = None;
                }
                return true;
            }
        }
        let dt = dt.clamp(0.0, 0.1);
        let k = self.keys;
        let mut changed = false;
        let turn = (k & TURN_R != 0) as i32 as f32 - (k & TURN_L != 0) as i32 as f32;
        if turn != 0.0 {
            self.heading = wrap(self.heading + turn * TURN_RATE * dt);
            changed = true;
        }
        let fwd_in = (k & FWD != 0) as i32 as f32 - (k & BACK != 0) as i32 as f32;
        let side_in = (k & RIGHT != 0) as i32 as f32 - (k & LEFT != 0) as i32 as f32;
        let (sh, ch) = self.heading.sin_cos();
        let fwd = Vec2::new(sh, -ch);
        let right = Vec2::new(ch, sh);
        let wish = (fwd * fwd_in + right * side_in).normalize_or_zero();
        let speed = if k & RUN_KEY != 0 { RUN } else { WALK };
        let target = wish * speed;
        // Exponential approach: ~0.25 s to reach walking speed, quicker to stop.
        let rate = if wish == Vec2::ZERO { 10.0 } else { 6.0 };
        self.vel += (target - self.vel) * (1.0 - (-rate * dt).exp());
        if self.vel.length_squared() < 2.5e-3 && wish == Vec2::ZERO {
            if self.vel != Vec2::ZERO || self.bob_amp > 0.0 {
                self.vel = Vec2::ZERO;
                self.bob_amp = 0.0;
                return true;
            }
            return changed;
        }
        // Move one axis at a time so the walker slides along obstacles.
        let start = self.pos;
        let mut p = self.pos;
        let nx = Vec2::new(p.x + self.vel.x * dt, p.y);
        if !blocked(nx, colliders) || blocked(p, colliders) {
            p = nx;
        } else {
            self.vel.x = 0.0;
        }
        let nz = Vec2::new(p.x, p.y + self.vel.y * dt);
        if !blocked(nz, colliders) || blocked(p, colliders) {
            p = nz;
        } else {
            self.vel.y = 0.0;
        }
        self.pos = p;
        let moved = p.distance(start);
        // Head bob: one step per ~0.7 m.
        self.bob_phase =
            (self.bob_phase + moved / 0.7 * std::f32::consts::PI) % (std::f32::consts::TAU * 64.0);
        let amp = (self.vel.length() / WALK).min(1.6);
        self.bob_amp += (amp - self.bob_amp) * (1.0 - (-8.0 * dt).exp());
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wall() -> Vec<Rect> {
        vec![Rect {
            min: Vec2::new(-5.0, -3.0),
            max: Vec2::new(5.0, -2.0),
        }]
    }

    fn run(w: &mut Walker, secs: f32, c: &[Rect]) {
        let mut t = 0.0;
        while t < secs {
            w.step(t as f64 * 1000.0, 1.0 / 60.0, c);
            t += 1.0 / 60.0;
        }
    }

    #[test]
    fn accelerates_smoothly_to_walking_speed() {
        let mut w = Walker::spawn(Vec3::ZERO, 0.0);
        w.keys = FWD;
        w.step(0.0, 1.0 / 60.0, &[]);
        assert!(w.vel.length() > 0.0 && w.vel.length() < WALK * 0.5);
        run(&mut w, 1.5, &[]);
        assert!((w.vel.length() - WALK).abs() < 0.05);
        assert!(w.pos.y < -1.0, "moved towards -z: {}", w.pos);
        w.keys |= RUN_KEY;
        run(&mut w, 1.5, &[]);
        assert!((w.vel.length() - RUN).abs() < 0.1);
        w.keys = 0;
        run(&mut w, 1.0, &[]);
        assert_eq!(w.vel, Vec2::ZERO);
    }

    #[test]
    fn cannot_walk_through_walls_but_slides() {
        let mut w = Walker::spawn(Vec3::ZERO, 0.3); // heading slightly right of -z
        w.keys = FWD | RUN_KEY;
        run(&mut w, 4.0, &wall());
        assert!(w.pos.y > -2.0 + RADIUS - 1e-3, "went through: {}", w.pos);
        assert!(w.pos.x > 1.0, "slid along the wall: {}", w.pos);
    }

    #[test]
    fn look_clamps_pitch_and_heading_wraps() {
        let mut w = Walker::spawn(Vec3::ZERO, 0.0);
        w.look(0.0, -100000.0);
        assert!(w.pitch <= MAX_PITCH);
        for _ in 0..100 {
            w.look(400.0, 0.0);
        }
        assert!(w.heading.abs() <= std::f32::consts::PI + 1e-4);
        assert!((w.forward().length() - 1.0).abs() < 1e-4);
    }

    #[test]
    fn strafe_and_turn() {
        let mut w = Walker::spawn(Vec3::ZERO, 0.0);
        w.keys = RIGHT;
        run(&mut w, 1.0, &[]);
        assert!(w.pos.x > 0.5 && w.pos.y.abs() < 1e-3);
        w.keys = TURN_L;
        run(&mut w, 0.5, &[]);
        assert!(w.heading < -0.5);
    }
}
