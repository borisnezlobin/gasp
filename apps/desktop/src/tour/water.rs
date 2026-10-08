//! The website's moving water, as the iPhone's `Water.metal` has it, for
//! the first step's note lines: smooth value noise drifting with time
//! bends them a little, and rings set off by the pointer spread across
//! them and lift each line they cross, as the site's ripple lines do.

use std::time::{Duration, Instant};

use gpui::{Pixels, Point, point};

/// How fast a ring spreads, in points a second.
const RING_SPEED: f32 = 300.;
const RING_WAVELENGTH: f32 = 56.;
/// How wide the band of waves travelling with a ring's front is.
const RING_PACKET: f32 = 70.;
/// How quickly a ring dies down, and when it has gone, in seconds.
const RING_FADE: f32 = 0.8;
const RING_LIFE: Duration = Duration::from_millis(2600);
/// The most a ring lifts a line, in points.
const RING_LIFT: f32 = 9.;

fn cell_hash(x: f32, y: f32) -> f32 {
    let mut across = (x * 123.34).rem_euclid(1.);
    let mut down = (y * 456.21).rem_euclid(1.);
    let spread = across * (across + 45.32) + down * (down + 45.32);
    across += spread;
    down += spread;
    (across * down).rem_euclid(1.)
}

fn value_noise(x: f32, y: f32) -> f32 {
    let (cell_x, cell_y) = (x.floor(), y.floor());
    let ease = |t: f32| t * t * (3. - 2. * t);
    let (ease_x, ease_y) = (ease(x - cell_x), ease(y - cell_y));
    let mix = |from: f32, to: f32, t: f32| from + (to - from) * t;
    let bottom = mix(
        cell_hash(cell_x, cell_y),
        cell_hash(cell_x + 1., cell_y),
        ease_x,
    );
    let top = mix(
        cell_hash(cell_x, cell_y + 1.),
        cell_hash(cell_x + 1., cell_y + 1.),
        ease_x,
    );
    mix(bottom, top, ease_y)
}

fn fractal_noise(x: f32, y: f32) -> f32 {
    value_noise(x, y) * 0.65 + value_noise(x * 2.03 + 17.1, y * 2.03 + 17.1) * 0.35
}

/// How far the water bends the point at `x`, `y` (in points) `time`
/// seconds in, each axis from -1 to 1. `frequency` stretches the waves:
/// long across and short down.
pub fn drift(x: f32, y: f32, time: f32, frequency: (f32, f32)) -> Point<f32> {
    let (x, y) = (x * frequency.0, y * frequency.1);
    let across = fractal_noise(x + time * 0.13, y + time * 0.31) - 0.5;
    let down = fractal_noise(x * 1.27 + 7.3 - time * 0.19, y * 1.27 + 2.1 + time * 0.23) - 0.5;
    point(across * 2., down * 2.)
}

/// A ring on the water where the pointer touched it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ring {
    pub center: Point<Pixels>,
    pub born: Instant,
    pub strength: f32,
}

impl Ring {
    /// How far the ring lifts the water at `at`, in points: up for less
    /// than zero.
    pub fn lift(&self, at: Point<Pixels>, now: Instant) -> f32 {
        let age = now.saturating_duration_since(self.born).as_secs_f32();
        let distance = f32::from(at.x - self.center.x).hypot(f32::from(at.y - self.center.y));
        let behind_front = distance - RING_SPEED * age;
        if !(-RING_PACKET * 3. ..=RING_PACKET * 2.5).contains(&behind_front) {
            return 0.;
        }
        let envelope = (-(behind_front / RING_PACKET).powi(2)).exp() * (-age / RING_FADE).exp();
        let wave = (std::f32::consts::TAU * behind_front / RING_WAVELENGTH).sin();
        RING_LIFT * self.strength * envelope * wave
    }

    pub fn is_spent(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.born) > RING_LIFE
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::px;

    #[test]
    fn the_water_bends_gently_and_keeps_moving() {
        let frequency = (0.008, 0.05);
        let samples: Vec<Point<f32>> = (0..200)
            .map(|step| drift(step as f32 * 7., 30., step as f32 * 0.05, frequency))
            .collect();
        assert!(
            samples
                .iter()
                .all(|bend| bend.x.abs() <= 1. && bend.y.abs() <= 1.)
        );
        let still = drift(10., 10., 0., frequency);
        assert_ne!(still, drift(10., 10., 1., frequency), "the water drifts");
    }

    #[test]
    fn a_ring_spreads_and_dies_down() {
        let born = Instant::now();
        let ring = Ring {
            center: point(px(0.), px(0.)),
            born,
            strength: 1.,
        };
        let far = point(px(300.), px(0.));
        let soon = born + Duration::from_millis(50);
        assert_eq!(ring.lift(far, soon), 0., "the front hasn't reached it");
        let reaching: f32 = (0..20)
            .map(|step| {
                ring.lift(far, born + Duration::from_millis(900 + step * 10))
                    .abs()
            })
            .fold(0., f32::max);
        assert!(reaching > 0.5, "{reaching}");
        assert!(ring.lift(far, born + Duration::from_secs(10)).abs() < 1e-3);
        assert!(ring.is_spent(born + Duration::from_secs(3)));
    }
}
