//! Everything on the first step that moves from frame to frame: the soft
//! glyphs of the name, the pointer, and the rings on the water. It's the
//! iPhone's `LiquidMotion` with the pointer for a finger: the glyphs lean
//! toward the pointer from a distance and part around it up close, and
//! the pointer leaves a trail of faint rings over the lines, with a
//! stronger one where it clicks.

use std::time::{Duration, Instant};

use gpui::{Bounds, Pixels, Point, point, px};

use super::water::Ring;

/// Rings on the water at once; a new one replaces the oldest.
const RING_SLOTS: usize = 4;
/// How far the pointer goes before it leaves another ring.
const TRAIL_SPACING: f32 = 56.;
const TRAIL_STRENGTH: f32 = 0.35;
const PRESS_STRENGTH: f32 = 1.;
const GREET_STRENGTH: f32 = 0.7;
/// The longest step a spring takes, so a late frame doesn't fling it.
const LONGEST_STEP: f32 = 1. / 30.;

/// One soft glyph: an offset from its place that a spring pulls back,
/// and a squash that rings like jelly after it has moved. Every length
/// scales with the glyph's size.
#[derive(Clone, Debug)]
pub struct SoftGlyph {
    phase: f32,
    offset: Point<f32>,
    velocity: Point<f32>,
    squash: f32,
    squash_velocity: f32,
    angle: f32,
}

impl SoftGlyph {
    const SPRING: f32 = 90.;
    const DAMPING: f32 = 6.6;
    const SQUASH_SPRING: f32 = 480.;
    const SQUASH_DAMPING: f32 = 7.5;
    const PUSH: f32 = 0.34;
    const PULL: f32 = 0.07;
    const REACH: f32 = 0.45;
    const SQUASH_PER_SPEED: f32 = 0.07;
    const MOST_SQUASH: f32 = 0.32;

    fn new(phase: f32) -> Self {
        SoftGlyph {
            phase,
            offset: point(0., 0.),
            velocity: point(0., 0.),
            squash: 0.,
            squash_velocity: 0.,
            angle: 0.,
        }
    }

    fn near(size: f32) -> f32 {
        size * 0.55 + 36.
    }

    pub fn offset(&self) -> Point<f32> {
        self.offset
    }

    /// The way the glyph is moving, which it stretches along.
    pub fn angle(&self) -> f32 {
        self.angle
    }

    fn step(&mut self, home: Point<f32>, pointer: Option<Point<f32>>, size: f32, elapsed: f32) {
        let want = self.wanted(home, pointer, size);
        self.velocity.x +=
            ((want.x - self.offset.x) * Self::SPRING - self.velocity.x * Self::DAMPING) * elapsed;
        self.velocity.y +=
            ((want.y - self.offset.y) * Self::SPRING - self.velocity.y * Self::DAMPING) * elapsed;
        self.offset.x += self.velocity.x * elapsed;
        self.offset.y += self.velocity.y * elapsed;
        self.keep_within_reach(size);
        self.step_squash(size, elapsed);
    }

    /// Where the pointer wants the glyph, as an offset from home: away
    /// inside `near`, a little towards it out to three times that.
    fn wanted(&self, home: Point<f32>, pointer: Option<Point<f32>>, size: f32) -> Point<f32> {
        let Some(pointer) = pointer else {
            return point(0., 0.);
        };
        let across = pointer.x - (home.x + self.offset.x);
        let down = pointer.y - (home.y + self.offset.y);
        let distance = across.hypot(down).max(1.);
        let near = Self::near(size);
        let far = near * 3.;
        if distance > far {
            return point(0., 0.);
        }
        let strength = if distance < near {
            -Self::PUSH * (1. - distance / near).powi(2)
        } else {
            Self::PULL * (std::f32::consts::PI * (distance - near) / (far - near)).sin()
        };
        point(
            across / distance * strength * size,
            down / distance * strength * size,
        )
    }

    fn keep_within_reach(&mut self, size: f32) {
        let reach = size * Self::REACH;
        let out = self.offset.x.hypot(self.offset.y);
        if out > reach {
            self.offset.x *= reach / out;
            self.offset.y *= reach / out;
        }
    }

    fn step_squash(&mut self, size: f32, elapsed: f32) {
        let speed = self.velocity.x.hypot(self.velocity.y);
        if speed > size * 0.2 {
            self.angle = self.velocity.y.atan2(self.velocity.x);
        }
        let target = (speed / size * Self::SQUASH_PER_SPEED).min(Self::MOST_SQUASH);
        self.squash_velocity += ((target - self.squash) * Self::SQUASH_SPRING
            - self.squash_velocity * Self::SQUASH_DAMPING)
            * elapsed;
        self.squash += self.squash_velocity * elapsed;
    }

    /// How far the glyph stretches along the way it's moving, breathing a
    /// little while it's still.
    pub fn stretch(&self, time: f32) -> f32 {
        1. + self.squash + 0.022 * (time * 1.7 + self.phase).sin()
    }

    /// A slow bob up and down while it's still.
    pub fn bob(&self, time: f32, size: f32) -> f32 {
        size * 0.014 * (time * 1.1 + self.phase * 2.).sin()
    }
}

/// The first step's moving parts, stepped once a frame from its render.
pub struct LiquidMotion {
    glyphs: Vec<SoftGlyph>,
    rings: [Option<Ring>; RING_SLOTS],
    next_ring: usize,
    pointer: Option<Point<Pixels>>,
    last_trail: Option<Point<Pixels>>,
    last_step: Option<Instant>,
    greeted: bool,
    started: Instant,
    /// Where the water is in the window, from the last frame drawn.
    sea: Option<Bounds<Pixels>>,
}

impl LiquidMotion {
    pub fn new(glyph_count: usize, started: Instant) -> Self {
        LiquidMotion {
            glyphs: (0..glyph_count)
                .map(|index| SoftGlyph::new(index as f32 * 1.37))
                .collect(),
            rings: [None; RING_SLOTS],
            next_ring: 0,
            pointer: None,
            last_trail: None,
            last_step: None,
            greeted: false,
            started,
            sea: None,
        }
    }

    pub fn glyphs(&self) -> &[SoftGlyph] {
        &self.glyphs
    }

    /// Seconds since the step first showed, for the water and the glyphs'
    /// breathing.
    pub fn time(&self, now: Instant) -> f32 {
        now.saturating_duration_since(self.started).as_secs_f32()
    }

    pub fn elapsed(&self, now: Instant) -> Duration {
        now.saturating_duration_since(self.started)
    }

    /// Steps the glyphs on to `now`, each pulled to its place in `homes`.
    pub fn advance(&mut self, now: Instant, homes: &[Point<Pixels>], size: Pixels) {
        let elapsed = self
            .last_step
            .map_or(0., |last| now.saturating_duration_since(last).as_secs_f32())
            .min(LONGEST_STEP);
        self.last_step = Some(now);
        let pointer = self
            .pointer
            .map(|at| point(f32::from(at.x), f32::from(at.y)));
        for (glyph, home) in self.glyphs.iter_mut().zip(homes) {
            glyph.step(
                point(f32::from(home.x), f32::from(home.y)),
                pointer,
                f32::from(size),
                elapsed,
            );
        }
        let rings = &mut self.rings;
        for slot in rings.iter_mut() {
            if slot.is_some_and(|ring| ring.is_spent(now)) {
                *slot = None;
            }
        }
    }

    /// Where the water is, so the pointer knows when it's over it.
    pub fn set_sea(&mut self, sea: Bounds<Pixels>) {
        self.sea = Some(sea);
    }

    /// Follows the pointer: over the water it leaves a ring each time it
    /// has gone far enough from the last.
    pub fn point(&mut self, at: Point<Pixels>, now: Instant) {
        self.pointer = Some(at);
        if !self.is_over_sea(at) {
            self.last_trail = None;
            return;
        }
        let far_enough = self.last_trail.is_none_or(|last| {
            f32::from(at.x - last.x).hypot(f32::from(at.y - last.y)) >= TRAIL_SPACING
        });
        if far_enough {
            self.last_trail = Some(at);
            self.drop_ring(at, TRAIL_STRENGTH, now);
        }
    }

    /// A click on the water sets off a stronger ring.
    pub fn press(&mut self, at: Point<Pixels>, now: Instant) {
        if self.is_over_sea(at) {
            self.last_trail = Some(at);
            self.drop_ring(at, PRESS_STRENGTH, now);
        }
    }

    pub fn leave(&mut self) {
        self.pointer = None;
        self.last_trail = None;
    }

    /// Sets off the one ring that greets the reader at `at`, the first
    /// time it's asked once `delay` has passed.
    pub fn greet(&mut self, at: Point<Pixels>, delay: Duration, now: Instant) {
        if self.greeted || self.elapsed(now) < delay {
            return;
        }
        self.greeted = true;
        self.drop_ring(at, GREET_STRENGTH, now);
    }

    pub fn rings(&self) -> impl Iterator<Item = &Ring> {
        self.rings.iter().flatten()
    }

    fn is_over_sea(&self, at: Point<Pixels>) -> bool {
        self.sea.is_some_and(|sea| sea.contains(&at))
    }

    fn drop_ring(&mut self, center: Point<Pixels>, strength: f32, now: Instant) {
        self.rings[self.next_ring] = Some(Ring {
            center,
            born: now,
            strength,
        });
        self.next_ring = (self.next_ring + 1) % RING_SLOTS;
    }
}

/// How far the water lifts a line at `at`: the rings' lifts together.
pub fn lift_at<'a>(
    rings: impl Iterator<Item = &'a Ring>,
    at: Point<Pixels>,
    now: Instant,
) -> Pixels {
    px(rings.map(|ring| ring.lift(at, now)).sum())
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::size;

    fn settle(
        motion: &mut LiquidMotion,
        homes: &[Point<Pixels>],
        start: Instant,
        frames: u32,
    ) -> Instant {
        let frame = Duration::from_millis(16);
        let mut now = start;
        for _ in 0..frames {
            now += frame;
            motion.advance(now, homes, px(200.));
        }
        now
    }

    #[test]
    fn glyphs_part_around_the_pointer_and_spring_home() {
        let start = Instant::now();
        let mut motion = LiquidMotion::new(1, start);
        let home = [point(px(100.), px(100.))];
        motion.point(point(px(130.), px(100.)), start);
        let now = settle(&mut motion, &home, start, 60);
        assert!(
            motion.glyphs()[0].offset().x < -10.,
            "pushed away from the pointer"
        );
        motion.leave();
        settle(&mut motion, &home, now, 240);
        assert!(motion.glyphs()[0].offset().x.abs() < 0.5, "back home");
    }

    #[test]
    fn the_pointer_rings_only_the_water() {
        let start = Instant::now();
        let mut motion = LiquidMotion::new(0, start);
        motion.set_sea(Bounds::new(
            point(px(0.), px(200.)),
            size(px(400.), px(100.)),
        ));
        motion.point(point(px(10.), px(10.)), start);
        assert_eq!(motion.rings().count(), 0);
        motion.point(point(px(10.), px(250.)), start);
        motion.point(point(px(20.), px(250.)), start);
        assert_eq!(motion.rings().count(), 1, "the trail is spaced out");
        motion.press(point(px(20.), px(250.)), start);
        assert_eq!(motion.rings().count(), 2);
        motion.advance(start + Duration::from_secs(5), &[], px(200.));
        assert_eq!(motion.rings().count(), 0, "spent rings go");
    }

    #[test]
    fn the_greeting_comes_once_after_its_delay() {
        let start = Instant::now();
        let mut motion = LiquidMotion::new(0, start);
        let delay = Duration::from_millis(700);
        let head = point(px(0.), px(0.));
        motion.greet(head, delay, start);
        assert_eq!(motion.rings().count(), 0);
        motion.greet(head, delay, start + delay);
        motion.greet(head, delay, start + delay * 2);
        assert_eq!(motion.rings().count(), 1);
    }
}
