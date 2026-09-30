//! The install window's page at any moment of its animation, as plain
//! geometry: where the whale is and how it's turned, the text lines as it
//! pushes through them, the letters it throws up and the caret. Drawing
//! it is [`super::paint`]'s job; nothing here needs a window, so every
//! frame can be checked in a test.
//!
//! The breach: a shadow rises under the lines, which lift as it nears;
//! the whale bursts through the first line and arcs up to the app icon's
//! pose, each line parting where it passes and rippling out, and a few
//! letters fly off as spray. The caret glows once as it comes to rest.
//! The dive runs the other way, nose first into the lines, and then the
//! first line is written again as the copy runs.

use std::f32::consts::TAU;

use crate::theme::InstallStage;

/// A point on the page.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Pt {
    pub x: f32,
    pub y: f32,
}

const fn pt(x: f32, y: f32) -> Pt {
    Pt { x, y }
}

/// Where the whale's middle is and how far its nose points up from
/// level, in degrees.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WhalePose {
    pub center: Pt,
    pub angle: f32,
}

impl WhalePose {
    /// The tip of the nose, for a whale `length` long.
    pub fn nose(&self, length: f32) -> Pt {
        let (sin, cos) = self.angle.to_radians().sin_cos();
        pt(
            self.center.x + cos * length / 2.,
            self.center.y - sin * length / 2.,
        )
    }
}

/// A letter in flight.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Letter {
    pub ch: char,
    /// The top-left of the letter.
    pub at: Pt,
    pub alpha: f32,
}

/// The caret: its top-left, how brightly it glows and how much of it
/// shows (each 0 to 1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Caret {
    pub at: Pt,
    pub glow: f32,
    pub alpha: f32,
}

/// One moment of the page.
#[derive(Clone, Debug, PartialEq)]
pub struct Frame {
    /// None once the whale is under the page.
    pub whale: Option<WhalePose>,
    /// Everything below this is under water.
    pub surface: f32,
    /// Each text line as one or two pieces (two where it has parted),
    /// each a run of points along its middle.
    pub lines: Vec<Vec<Pt>>,
    /// How far the first line has been written again, as a point along
    /// it: the copy's progress.
    pub written_to: Option<f32>,
    pub letters: Vec<Letter>,
    pub caret: Caret,
}

/// Which animation, and how far through it (0 to 1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Moment {
    Breach(f32),
    Dive(f32),
    /// The whale is under; the first line is written this far.
    Writing(f32),
}

impl Moment {
    /// The page at rest: the app icon.
    pub const REST: Moment = Moment::Breach(1.);

    /// Reads a moment from `GASP_INSTALL_FREEZE`, for screenshots:
    /// `breach:0.3`, `dive:0.5`, `writing:0.4`, or a bare number for the
    /// breach.
    pub fn parse(text: &str) -> Option<Moment> {
        let (kind, at) = text.split_once(':').unwrap_or(("breach", text));
        let at: f32 = at.trim().parse().ok()?;
        let at = at.clamp(0., 1.);
        match kind.trim() {
            "breach" => Some(Moment::Breach(at)),
            "dive" => Some(Moment::Dive(at)),
            "writing" => Some(Moment::Writing(at)),
            _ => None,
        }
    }
}

// The breach's timing, as fractions of it.
/// When the whale reaches its resting pose.
const RISE_END: f32 = 0.72;
/// How far past the pose the rise carries before it settles back.
const RISE_OVERSHOOT: f32 = 0.9;
/// The caret's glow, as the whale comes to rest.
const GLOW_START: f32 = 0.6;
const GLOW_END: f32 = 1.;

// The whale's path, in degrees and page pixels.
const BREACH_START_ANGLE: f32 = 70.;
/// The rise leaves the water steeper than it arrives: its path bends
/// towards this heading at the resting pose.
const BREACH_ARRIVAL_HEADING: f32 = 24.;
const BREACH_BEND: f32 = 120.;
const DIVE_END_ANGLE: f32 = -66.;
const DIVE_HEADING: f32 = 10.;
const DIVE_REACH: f32 = 100.;
const DIVE_DRIFT: f32 = 96.;
/// The caret steps aside while the whale dives past it.
const DIVE_CARET_FADE: (f32, f32) = (0.2, 0.4);
/// How far below the page the dive ends.
const OFF_PAGE: f32 = 10.;

// How the lines move, in seconds and pixels.
/// Pushed apart where the whale passes: the widest gap either side.
const PART_WIDTH: f32 = 20.;
const PART_REACH: f32 = 34.;
const PART_OPEN: f32 = 0.035;
const PART_CLOSE: f32 = 0.24;
/// Ripples running out along the line.
const RIPPLE_HEIGHT: f32 = 4.5;
const RIPPLE_WAVELENGTH: f32 = 78.;
const RIPPLE_SPEED: f32 = 210.;
const RIPPLE_FADE: f32 = 0.38;
const RIPPLE_REACH: f32 = 170.;
/// Each line down moves this much less than the one above it.
const DEPTH_DAMPING: f32 = 0.78;
/// The lift ahead of the whale before it breaks through.
const SWELL_HEIGHT: f32 = 3.5;
const SWELL_REACH: f32 = 46.;
const SWELL_LEAD: f32 = 0.16;
/// How finely a line is traced.
const LINE_STEP: f32 = 3.;
/// Everything settles over the last part of the breach and the dive.
const BREACH_SETTLE: f32 = 0.8;
const DIVE_SETTLE: f32 = 0.82;
/// A line parted by less than this is drawn whole.
const MIN_GAP: f32 = 0.5;

// The spray: letters thrown out either side of where the whale broke
// the surface, arcing out and falling back into the lines.
const BREACH_SPRAY: &str = "gaspbreath";
/// How far from the crossing the letters leave the line, either side.
const SPRAY_EDGE: f32 = 24.;
const SPRAY_EDGE_JITTER: f32 = 16.;
const SPRAY_LIFT: f32 = 230.;
const SPRAY_LIFT_JITTER: f32 = 150.;
const SPRAY_THROW: f32 = 60.;
const SPRAY_THROW_JITTER: f32 = 150.;
const SPRAY_GRAVITY: f32 = 1300.;
const SPRAY_STAGGER: f32 = 0.012;
const SPRAY_LIFE: f32 = 0.62;
/// How finely the whale's path is searched for where it crosses a line.
const CROSSING_SAMPLES: usize = 240;

/// Where the whale broke through a line: the point along it, and when.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Crossing {
    x: f32,
    seconds: f32,
    /// How far down the lines this one is (0 is the surface).
    depth: usize,
}

/// The page at `moment`.
pub fn frame(stage: &InstallStage, moment: Moment) -> Frame {
    match moment {
        Moment::Breach(t) => breach(stage, t),
        Moment::Dive(t) => dive(stage, t),
        Moment::Writing(done) => writing(stage, done),
    }
}

fn breach(stage: &InstallStage, t: f32) -> Frame {
    let seconds = t * stage.breach_seconds;
    let crossings = crossings(
        stage,
        stage.breach_seconds,
        |at| breach_pose(stage, at),
        true,
    );
    let calm = 1. - smoothstep(BREACH_SETTLE, 1., t);
    let lines = disturbed_lines(stage, &crossings, seconds, calm);
    let letters = crossings
        .iter()
        .find(|crossing| crossing.depth == 0)
        .map(|surface| spray(stage, surface, seconds, BREACH_SPRAY))
        .unwrap_or_default();
    let glow = bump((t - GLOW_START) / (GLOW_END - GLOW_START));
    Frame {
        whale: Some(breach_pose(stage, t)),
        surface: stage.line_top,
        caret: caret(stage, &crossings, seconds, calm, glow),
        lines,
        written_to: None,
        letters,
    }
}

fn dive(stage: &InstallStage, t: f32) -> Frame {
    let seconds = t * stage.dive_seconds;
    let crossings = crossings(stage, stage.dive_seconds, |at| dive_pose(stage, at), false);
    let calm = 1. - smoothstep(DIVE_SETTLE, 1., t);
    let lines = disturbed_lines(stage, &crossings, seconds, calm);
    let mut caret = caret(stage, &crossings, seconds, calm, 0.);
    caret.alpha = 1. - smoothstep(DIVE_CARET_FADE.0, DIVE_CARET_FADE.1, t);
    Frame {
        whale: (t < 1.).then(|| dive_pose(stage, t)),
        surface: stage.line_top,
        caret,
        lines,
        written_to: None,
        letters: Vec::new(),
    }
}

fn writing(stage: &InstallStage, done: f32) -> Frame {
    let done = done.clamp(0., 1.);
    let lines = disturbed_lines(stage, &[], 0., 0.);
    let written_to = stage.line_left + stage.line_lengths[0] * done;
    let mut frame = Frame {
        whale: None,
        surface: stage.line_top,
        lines,
        written_to: Some(written_to),
        letters: Vec::new(),
        caret: caret(stage, &[], 0., 0., 0.),
    };
    frame.caret.at.x = written_to + stage.caret_gap;
    frame
}

/// The whale `t` of the way through the breach: along a curve from under
/// the page to the resting pose, turning from steep to the icon's angle,
/// fast out of the water and easing into the pose.
pub fn breach_pose(stage: &InstallStage, t: f32) -> WhalePose {
    let rest = rest_center(stage);
    let start_depth = start_depth(stage, BREACH_START_ANGLE);
    let start = offset(rest, BREACH_START_ANGLE, -start_depth);
    let bend = offset(rest, BREACH_ARRIVAL_HEADING, -BREACH_BEND);
    let rise = (t / RISE_END).clamp(0., 1.);
    // Slow as it nears the surface from below, fast through it, and
    // easing past the pose and back.
    let along = ease_out_back(rise * rise, RISE_OVERSHOOT);
    let turn = ease_out_cubic(rise);
    WhalePose {
        center: bezier(start, bend, rest, along),
        angle: lerp(BREACH_START_ANGLE, stage.whale_rest_angle, turn),
    }
}

/// The whale `t` of the way through the dive: tipping over forwards and
/// slipping nose first under the lines, faster as it goes.
pub fn dive_pose(stage: &InstallStage, t: f32) -> WhalePose {
    let rest = rest_center(stage);
    let reach = offset(rest, DIVE_HEADING, DIVE_REACH);
    let tail_clear = stage.whale_length / 2. * DIVE_END_ANGLE.to_radians().sin().abs();
    let end = pt(rest.x + DIVE_DRIFT, stage.height + OFF_PAGE + tail_clear);
    let t = t.clamp(0., 1.);
    WhalePose {
        center: bezier(rest, reach, end, t * t),
        angle: lerp(
            stage.whale_rest_angle,
            DIVE_END_ANGLE,
            smoothstep(0., 0.85, t),
        ),
    }
}

fn rest_center(stage: &InstallStage) -> Pt {
    pt(stage.whale_rest.0, stage.whale_rest.1)
}

/// How far back along its heading the whale starts, so its nose is at
/// the page's bottom edge, lost in the depths.
fn start_depth(stage: &InstallStage, angle: f32) -> f32 {
    let sin = angle.to_radians().sin();
    let nose_under = stage.height - stage.whale_rest.1;
    nose_under / sin + stage.whale_length / 2.
}

/// `from` moved `distance` along a heading `angle` degrees up from level.
fn offset(from: Pt, angle: f32, distance: f32) -> Pt {
    let (sin, cos) = angle.to_radians().sin_cos();
    pt(from.x + cos * distance, from.y - sin * distance)
}

/// When and where the nose crosses each line, rising (the breach) or
/// falling (the dive), in the order it happens.
fn crossings(
    stage: &InstallStage,
    duration: f32,
    pose: impl Fn(f32) -> WhalePose,
    rising: bool,
) -> Vec<Crossing> {
    let noses: Vec<Pt> = (0..=CROSSING_SAMPLES)
        .map(|sample| pose(sample as f32 / CROSSING_SAMPLES as f32).nose(stage.whale_length))
        .collect();
    let mut found: Vec<Crossing> = (0..stage.line_lengths.len())
        .filter_map(|depth| {
            let y = line_y(stage, depth);
            let step = noses.windows(2).position(|pair| {
                let (before, after) = (pair[0].y - y, pair[1].y - y);
                if rising {
                    before > 0. && after <= 0.
                } else {
                    before < 0. && after >= 0.
                }
            })?;
            Some(Crossing {
                x: noses[step + 1].x,
                seconds: (step + 1) as f32 / CROSSING_SAMPLES as f32 * duration,
                depth,
            })
        })
        .collect();
    found.sort_by(|a, b| a.seconds.total_cmp(&b.seconds));
    found
}

fn line_y(stage: &InstallStage, depth: usize) -> f32 {
    stage.line_top + stage.line_gap * depth as f32
}

/// Every text line at `seconds`, pushed about by `crossings` and scaled
/// by `strength` (0 is calm).
fn disturbed_lines(
    stage: &InstallStage,
    crossings: &[Crossing],
    seconds: f32,
    strength: f32,
) -> Vec<Vec<Pt>> {
    let mut lines = Vec::new();
    for (depth, length) in stage.line_lengths.iter().enumerate() {
        let crossing = crossings
            .iter()
            .find(|crossing| crossing.depth == depth)
            .copied()
            .filter(|_| strength > 0.);
        let parted = crossing
            .is_some_and(|crossing| parting(&crossing, seconds) * strength * PART_WIDTH >= MIN_GAP);
        let left = stage.line_left;
        let mut before = Vec::new();
        let mut after = Vec::new();
        let steps = (length / LINE_STEP).ceil() as usize;
        for step in 0..=steps {
            let x = (left + step as f32 * LINE_STEP).min(left + length);
            let (dx, dy) = crossing.map_or((0., 0.), |crossing| {
                displacement(&crossing, x, seconds, strength)
            });
            let moved = pt(x + dx, line_y(stage, depth) + dy);
            match crossing {
                Some(crossing) if parted && x > crossing.x => after.push(moved),
                _ => before.push(moved),
            }
        }
        lines.extend([before, after].into_iter().filter(|piece| piece.len() > 1));
    }
    lines
}

/// How far the point `x` along a line moves, sideways and down, at
/// `seconds` after the start, from the whale crossing it.
fn displacement(crossing: &Crossing, x: f32, seconds: f32, strength: f32) -> (f32, f32) {
    let damping = DEPTH_DAMPING.powi(crossing.depth as i32) * strength;
    let since = seconds - crossing.seconds;
    let distance = x - crossing.x;
    if since < 0. {
        let swell = smoothstep(-SWELL_LEAD, 0., since) * gaussian(distance, SWELL_REACH);
        return (0., -SWELL_HEIGHT * swell * damping);
    }
    let part = distance.signum()
        * PART_WIDTH
        * parting(crossing, seconds)
        * gaussian(distance, PART_REACH);
    (part * damping, ripple(distance.abs(), since) * damping)
}

/// How far open the gap where the whale crossed is, 0 to 1 of its widest.
fn parting(crossing: &Crossing, seconds: f32) -> f32 {
    let since = seconds - crossing.seconds;
    if since < 0. {
        return 0.;
    }
    (1. - (-since / PART_OPEN).exp()) * (-since / PART_CLOSE).exp()
}

/// A ripple running out from where the whale crossed, `since` seconds
/// on, at `distance` from there. The first swing lifts the line.
fn ripple(distance: f32, since: f32) -> f32 {
    let front = RIPPLE_SPEED * since;
    if distance > front + RIPPLE_WAVELENGTH / 4. {
        return 0.;
    }
    let phase = TAU * (distance - front) / RIPPLE_WAVELENGTH;
    let reach = (-distance / RIPPLE_REACH).exp();
    let fade = (-since / RIPPLE_FADE).exp();
    -RIPPLE_HEIGHT * phase.cos() * reach * fade
}

/// Letters thrown up from the surface where the whale broke it,
/// alternately to its left and right.
fn spray(stage: &InstallStage, surface: &Crossing, seconds: f32, letters: &str) -> Vec<Letter> {
    letters
        .chars()
        .enumerate()
        .filter_map(|(index, ch)| {
            let since = seconds - surface.seconds - index as f32 * SPRAY_STAGGER;
            if !(0. ..SPRAY_LIFE).contains(&since) {
                return None;
            }
            let side = if index % 2 == 0 { -1. } else { 1. };
            let (near, far) = (golden(index), golden(index + letters.len()));
            let start_x = surface.x + side * (SPRAY_EDGE + SPRAY_EDGE_JITTER * near);
            let throw = side * (SPRAY_THROW + SPRAY_THROW_JITTER * far);
            let lift = SPRAY_LIFT + SPRAY_LIFT_JITTER * near;
            let x = start_x + throw * since;
            let y = stage.line_top - stage.letter_size - lift * since
                + SPRAY_GRAVITY * since * since / 2.;
            let alpha =
                smoothstep(0., 0.05, since) * (1. - smoothstep(0.55, 1., since / SPRAY_LIFE));
            Some(Letter {
                ch,
                at: pt(x, y),
                alpha,
            })
        })
        .collect()
}

/// The caret after the first line, riding its ripples.
fn caret(
    stage: &InstallStage,
    crossings: &[Crossing],
    seconds: f32,
    strength: f32,
    glow: f32,
) -> Caret {
    let x = stage.line_left + stage.line_lengths[0] + stage.caret_gap;
    let dy = crossings
        .iter()
        .find(|crossing| crossing.depth == 0)
        .map_or(0., |surface| displacement(surface, x, seconds, strength).1);
    Caret {
        at: pt(x, stage.line_top - stage.caret_height / 2. + dy),
        glow,
        alpha: 1.,
    }
}

/// A spread of values in 0..1 that never bunch up.
fn golden(index: usize) -> f32 {
    (index as f32 * 0.618_034 + 0.31).fract()
}

fn gaussian(distance: f32, reach: f32) -> f32 {
    (-(distance / reach).powi(2)).exp()
}

fn lerp(from: f32, to: f32, t: f32) -> f32 {
    from + (to - from) * t
}

fn bezier(start: Pt, control: Pt, end: Pt, t: f32) -> Pt {
    let u = 1. - t;
    pt(
        u * u * start.x + 2. * u * t * control.x + t * t * end.x,
        u * u * start.y + 2. * u * t * control.y + t * t * end.y,
    )
}

fn smoothstep(from: f32, to: f32, x: f32) -> f32 {
    let t = ((x - from) / (to - from)).clamp(0., 1.);
    t * t * (3. - 2. * t)
}

/// Up and back down over 0..1, and 0 outside it.
fn bump(t: f32) -> f32 {
    if t <= 0. || t >= 1. {
        return 0.;
    }
    (t * std::f32::consts::PI).sin().powi(2)
}

fn ease_out_cubic(t: f32) -> f32 {
    1. - (1. - t).powi(3)
}

/// Eases out past 1 by an amount set by `overshoot`, then back to 1.
fn ease_out_back(t: f32, overshoot: f32) -> f32 {
    let u = t - 1.;
    1. + (overshoot + 1.) * u.powi(3) + overshoot * u.powi(2)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::InstallTheme;

    fn stage() -> InstallStage {
        InstallTheme::new(false).stage
    }

    fn calm(stage: &InstallStage) -> Vec<Vec<Pt>> {
        disturbed_lines(stage, &[], 0., 0.)
    }

    #[test]
    fn the_breach_ends_on_the_icon() {
        let stage = stage();
        let rest = frame(&stage, Moment::REST);
        let whale = rest.whale.unwrap();
        assert!((whale.center.x - stage.whale_rest.0).abs() < 0.01);
        assert!((whale.center.y - stage.whale_rest.1).abs() < 0.01);
        assert!((whale.angle - stage.whale_rest_angle).abs() < 0.01);
        assert_eq!(rest.lines, calm(&stage));
        assert!(rest.letters.is_empty());
        assert_eq!(rest.caret.glow, 0.);
    }

    #[test]
    fn the_whale_starts_under_the_page_and_breaks_the_surface() {
        let stage = stage();
        let start = breach_pose(&stage, 0.);
        assert!(start.nose(stage.whale_length).y >= stage.height - 0.01);
        let found = crossings(
            &stage,
            stage.breach_seconds,
            |t| breach_pose(&stage, t),
            true,
        );
        let depths: Vec<usize> = found.iter().map(|crossing| crossing.depth).collect();
        assert_eq!(depths, vec![4, 3, 2, 1, 0], "the deepest line parts first");
        assert!(
            found
                .windows(2)
                .all(|pair| pair[0].seconds <= pair[1].seconds)
        );
    }

    #[test]
    fn lines_part_and_letters_fly_as_it_breaks_through() {
        let stage = stage();
        let found = crossings(
            &stage,
            stage.breach_seconds,
            |t| breach_pose(&stage, t),
            true,
        );
        let surface = found.last().unwrap();
        let t = (surface.seconds + 0.1) / stage.breach_seconds;
        let splash = frame(&stage, Moment::Breach(t));
        assert!(
            splash.lines.len() > stage.line_lengths.len(),
            "some lines split in two"
        );
        assert!(!splash.letters.is_empty());
        assert!(
            splash
                .letters
                .iter()
                .all(|letter| letter.at.y < stage.line_top)
        );
    }

    #[test]
    fn the_caret_glows_once_as_the_whale_settles() {
        let stage = stage();
        let glows: Vec<f32> = (0..=20)
            .map(|step| frame(&stage, Moment::Breach(step as f32 / 20.)).caret.glow)
            .collect();
        assert_eq!(glows[0], 0.);
        assert!(glows.iter().any(|glow| *glow > 0.9));
        assert_eq!(*glows.last().unwrap(), 0.);
    }

    #[test]
    fn the_dive_leaves_the_page_calm_and_empty() {
        let stage = stage();
        let under = frame(&stage, Moment::Dive(1.));
        assert_eq!(under.whale, None);
        assert_eq!(under.caret.alpha, 0.);
        assert_eq!(under.lines, calm(&stage));
        let late = dive_pose(&stage, 0.999);
        let tail = offset(late.center, late.angle, -stage.whale_length / 2.);
        assert!(tail.y > stage.height, "the tail is under the page too");
    }

    #[test]
    fn writing_moves_the_caret_with_the_progress() {
        let stage = stage();
        let half = frame(&stage, Moment::Writing(0.5));
        let written = half.written_to.unwrap();
        assert!((written - (stage.line_left + stage.line_lengths[0] / 2.)).abs() < 0.01);
        assert!((half.caret.at.x - written - stage.caret_gap).abs() < 0.01);
        let done = frame(&stage, Moment::Writing(1.));
        assert_eq!(done.caret.at, frame(&stage, Moment::REST).caret.at);
    }

    #[test]
    fn a_frozen_moment_is_read_from_text() {
        assert_eq!(Moment::parse("0.25"), Some(Moment::Breach(0.25)));
        assert_eq!(Moment::parse("dive:0.5"), Some(Moment::Dive(0.5)));
        assert_eq!(Moment::parse("writing:2"), Some(Moment::Writing(1.)));
        assert_eq!(Moment::parse("swim:0.5"), None);
        assert_eq!(Moment::parse("soon"), None);
    }
}
