//! Petits outils de géométrie 3D (Z vers le haut, comme dans GTA V).

use serde::{Deserialize, Serialize};
use std::ops::{Add, Mul, Neg, Sub};

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct V3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

pub const fn v3(x: f32, y: f32, z: f32) -> V3 {
    V3 { x, y, z }
}

impl V3 {
    pub const ZERO: V3 = v3(0.0, 0.0, 0.0);
    #[allow(dead_code)]
    pub const ONE: V3 = v3(1.0, 1.0, 1.0);
    pub fn dot(self, o: V3) -> f32 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }
    pub fn cross(self, o: V3) -> V3 {
        v3(
            self.y * o.z - self.z * o.y,
            self.z * o.x - self.x * o.z,
            self.x * o.y - self.y * o.x,
        )
    }
    pub fn len(self) -> f32 {
        self.dot(self).sqrt()
    }
    pub fn norm(self) -> V3 {
        let l = self.len();
        if l > 1e-9 {
            self * (1.0 / l)
        } else {
            self
        }
    }
    pub fn mulv(self, o: V3) -> V3 {
        v3(self.x * o.x, self.y * o.y, self.z * o.z)
    }
    pub fn min(self, o: V3) -> V3 {
        v3(self.x.min(o.x), self.y.min(o.y), self.z.min(o.z))
    }
    pub fn max(self, o: V3) -> V3 {
        v3(self.x.max(o.x), self.y.max(o.y), self.z.max(o.z))
    }
    pub fn dist2d(self, o: V3) -> f32 {
        ((self.x - o.x).powi(2) + (self.y - o.y).powi(2)).sqrt()
    }
    pub fn lerp(self, o: V3, t: f32) -> V3 {
        self + (o - self) * t
    }
}

impl Add for V3 {
    type Output = V3;
    fn add(self, o: V3) -> V3 {
        v3(self.x + o.x, self.y + o.y, self.z + o.z)
    }
}
impl Sub for V3 {
    type Output = V3;
    fn sub(self, o: V3) -> V3 {
        v3(self.x - o.x, self.y - o.y, self.z - o.z)
    }
}
impl Mul<f32> for V3 {
    type Output = V3;
    fn mul(self, s: f32) -> V3 {
        v3(self.x * s, self.y * s, self.z * s)
    }
}
impl Neg for V3 {
    type Output = V3;
    fn neg(self) -> V3 {
        v3(-self.x, -self.y, -self.z)
    }
}

/// Quaternion (x, y, z, w).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Quat {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}

impl Default for Quat {
    fn default() -> Self {
        Quat::IDENTITY
    }
}

impl Quat {
    pub const IDENTITY: Quat = Quat { x: 0.0, y: 0.0, z: 0.0, w: 1.0 };

    pub fn from_z(angle_rad: f32) -> Quat {
        let h = angle_rad * 0.5;
        Quat { x: 0.0, y: 0.0, z: h.sin(), w: h.cos() }
    }
    pub fn is_identity(&self) -> bool {
        self.x.abs() < 1e-6 && self.y.abs() < 1e-6 && self.z.abs() < 1e-6 && (self.w.abs() - 1.0).abs() < 1e-6
    }
    pub fn inverse(self) -> Quat {
        let n = self.x * self.x + self.y * self.y + self.z * self.z + self.w * self.w;
        if n < 1e-12 {
            return Quat::IDENTITY;
        }
        Quat { x: -self.x / n, y: -self.y / n, z: -self.z / n, w: self.w / n }
    }
    pub fn normalized(self) -> Quat {
        let n = (self.x * self.x + self.y * self.y + self.z * self.z + self.w * self.w).sqrt();
        if n < 1e-9 {
            return Quat::IDENTITY;
        }
        Quat { x: self.x / n, y: self.y / n, z: self.z / n, w: self.w / n }
    }
    /// self * o
    pub fn mul(self, o: Quat) -> Quat {
        Quat {
            w: self.w * o.w - self.x * o.x - self.y * o.y - self.z * o.z,
            x: self.w * o.x + self.x * o.w + self.y * o.z - self.z * o.y,
            y: self.w * o.y - self.x * o.z + self.y * o.w + self.z * o.x,
            z: self.w * o.z + self.x * o.y - self.y * o.x + self.z * o.w,
        }
    }
    pub fn rotate(self, v: V3) -> V3 {
        let q = v3(self.x, self.y, self.z);
        let t = q.cross(v) * 2.0;
        v + t * self.w + q.cross(t)
    }
    /// Cap autour de Z (en degrés, 0..360).
    pub fn heading_deg(self) -> f32 {
        let f = self.rotate(v3(0.0, 1.0, 0.0));
        let a = (-f.x).atan2(f.y).to_degrees();
        if a < 0.0 {
            a + 360.0
        } else {
            a
        }
    }
    /// Même rotation avec un autre cap : tourne autour de l'axe Z du monde de l'écart, l'inclinaison X/Y est gardée.
    pub fn with_heading_deg(self, deg: f32) -> Quat {
        Quat::from_z((deg - self.heading_deg()).to_radians()).mul(self).normalized()
    }
}

/// Petit générateur pseudo-aléatoire déterministe.
#[derive(Clone)]
pub struct Rng(u64);
impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407) | 1)
    }
    pub fn next_u32(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 16) as u32
    }
    pub fn f(&mut self) -> f32 {
        (self.next_u32() % 1_000_000) as f32 / 1_000_000.0
    }
    pub fn range(&mut self, a: f32, b: f32) -> f32 {
        a + (b - a) * self.f()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changer_le_cap_garde_l_inclinaison() {
        // objet penché de 20° autour de X, cap 30°
        let h = (20f32.to_radians() * 0.5, 0.0);
        let tilt = Quat { x: h.0.sin(), y: 0.0, z: 0.0, w: h.0.cos() };
        let q = Quat::from_z(30f32.to_radians()).mul(tilt).normalized();
        let r = q.with_heading_deg(100.0);
        assert!((r.heading_deg() - 100.0).abs() < 0.01);
        // l'angle entre le haut de l'objet et la verticale ne change pas
        let up = |q: Quat| q.rotate(v3(0.0, 0.0, 1.0)).z;
        assert!((up(r) - up(q)).abs() < 1e-4);
        assert!((up(r) - 20f32.to_radians().cos()).abs() < 1e-4);
    }
}
