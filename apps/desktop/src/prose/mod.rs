//! Prose in the editor: sentence-length tints behind the text and the
//! grammar checker's wavy underlines.
//!
//! Both work paragraph by paragraph and only on screen. Each frame lists
//! the paragraphs it shows and looks each one up by a hash of its text,
//! so a paragraph is segmented once and checked once until it changes,
//! and typing re-segments just the paragraph being typed in. Checking
//! runs on the grammar worker a moment after typing pauses; until then a
//! changed paragraph keeps the underlines it had, moved along with the
//! edits, less any an edit touched.

pub mod card;
pub mod checker;
pub mod commands;

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::ops::Range;
use std::rc::Rc;
use std::time::{Duration, Instant};

use editor_config::settings::ProseSettings;
use editor_core::syntax::Edit;
use editor_prose::{
    Flag, FlagKind, Length, Purpose, Thresholds, Unit, projection::Piece, sentence_lengths, units,
};
use gpui::{Bounds, Context, Hsla, Pixels, Point, Task, point};

use self::checker::Job;
use crate::editor::EditorView;
use crate::frame::FrameLayout;
use crate::theme::Theme;

/// How long typing pauses before changed paragraphs are checked.
pub const CHECK_DELAY: Duration = Duration::from_millis(500);

/// Paragraphs kept in each cache before it starts over.
const CACHE_LIMIT: usize = 4096;

/// A paragraph's sentences, counted from its start.
type Tints = Rc<[(Range<usize>, Length)]>;

/// A paragraph's flags, counted from its start, and the checker
/// generation they came from.
struct Checked {
    flags: Rc<[Flag]>,
    generation: u64,
}

/// The editor's prose state.
#[derive(Default)]
pub struct ProseState {
    /// Sentence-length thresholds, while highlighting is on.
    rhythm: Option<Thresholds>,
    /// Whether the grammar checker underlines problems.
    grammar: bool,
    tints: HashMap<u64, Tints>,
    flags: HashMap<u64, Checked>,
    /// The text on screen in the last frame.
    visible: Range<usize>,
    /// Whether a paragraph on screen has flags missing or stale.
    waiting: bool,
    job: Option<Task<()>>,
    /// Whether the text changed since the check timer last fired.
    edited: bool,
    /// The flags drawn in the last frame, at note offsets.
    shown: Vec<Flag>,
    /// The sentences tinted in the last frame.
    tinted: Vec<(Range<usize>, Length)>,
}

impl ProseState {
    pub fn from_settings(settings: &ProseSettings) -> ProseState {
        let mut state = ProseState::default();
        state.apply(settings);
        state
    }

    /// Takes new settings; answers whether anything drawn changes.
    fn apply(&mut self, settings: &ProseSettings) -> bool {
        let length = &settings.sentence_length;
        let rhythm = length.enabled.then_some(Thresholds {
            short_below: length.short_below as usize,
            long_above: length.long_above as usize,
        });
        let changed = rhythm != self.rhythm || settings.grammar.enabled != self.grammar;
        self.rhythm = rhythm;
        self.grammar = settings.grammar.enabled;
        if !self.grammar {
            self.shown.clear();
            self.waiting = false;
        }
        changed
    }
}

/// What prose adds to a frame, in window coordinates.
#[derive(Clone, Debug, Default)]
pub struct ProseFrame {
    /// Sentence tints, painted behind the text.
    pub tints: Vec<(Bounds<Pixels>, Hsla)>,
    /// Where wavy underlines start, how wide they are, and their colour.
    pub underlines: Vec<(Point<Pixels>, Pixels, Hsla)>,
}

impl EditorView {
    /// Follows the prose settings.
    pub fn apply_prose_settings(&mut self, settings: &ProseSettings, cx: &mut Context<Self>) {
        if self.prose.apply(settings) {
            cx.notify();
        }
    }

    /// Whether sentences are tinted by length.
    pub fn is_highlighting_sentences(&self) -> bool {
        self.prose.rhythm.is_some()
    }

    /// Notes an edit, so checking waits for typing to pause, and moves
    /// the flags on screen along with it until their paragraph is checked
    /// again.
    pub(crate) fn prose_edited(&mut self, edit: &Edit) {
        self.prose.edited = true;
        carry_through(&mut self.prose.shown, edit);
    }

    /// The sentences tinted in the last frame, with their lengths.
    pub fn tinted_sentences(&self) -> &[(Range<usize>, Length)] {
        &self.prose.tinted
    }

    /// The flags drawn in the last frame.
    pub fn shown_flags(&self) -> &[Flag] {
        &self.prose.shown
    }

    /// The flag drawn under `offset`, if any.
    pub fn flag_at(&self, offset: usize) -> Option<&Flag> {
        self.prose
            .shown
            .iter()
            .find(|flag| flag.range.start <= offset && offset < flag.range.end)
    }

    /// The flag under a window position, as for a right-click.
    pub fn flag_at_point(&self, position: Point<Pixels>) -> Option<Flag> {
        let offset = self.frame.as_ref()?.offset_at(position)?;
        self.flag_at(offset).cloned()
    }

    /// Replaces a flagged range with one of its suggestions, if the text
    /// there is still what was flagged.
    pub fn accept_flag(&mut self, flag: &Flag, replacement: &str, cx: &mut Context<Self>) {
        self.close_preview(cx);
        if self.flag_at(flag.range.start) != Some(flag) {
            return;
        }
        self.replace(flag.range.clone(), replacement, cx);
    }

    /// Stops flagging this phrase anywhere.
    pub fn ignore_flag(&mut self, flag: &Flag, cx: &mut Context<Self>) {
        self.close_preview(cx);
        let phrase = self.source.text()[flag.range.clone()].to_owned();
        checker::ignore(&phrase, cx);
        cx.notify();
    }

    /// Tints and underlines for the lines in `frame`.
    pub(crate) fn prose_frame(
        &mut self,
        frame: &FrameLayout,
        cx: &mut Context<Self>,
    ) -> ProseFrame {
        let (Some(first), Some(last)) = (frame.lines.first(), frame.lines.last()) else {
            return ProseFrame::default();
        };
        if self.read_only {
            return ProseFrame::default();
        }
        let started = Instant::now();
        let visible = first.visual.start..last.visual.end();
        self.prose.tinted = self.sentence_tints(visible.clone());
        let theme = &self.theme;
        let tints = self
            .prose
            .tinted
            .iter()
            .flat_map(|(range, length)| {
                let color = theme.sentence[*length as usize];
                band_rects(frame, range, theme)
                    .into_iter()
                    .map(move |rect| (rect, color))
            })
            .collect();
        self.prose.shown = self.visible_flags(visible, cx);
        let underlines = self
            .prose
            .shown
            .iter()
            .flat_map(|flag| underline_spans(frame, flag, &self.theme))
            .collect();
        self.timings.prose.push(started.elapsed());
        ProseFrame { tints, underlines }
    }

    /// Each sentence on screen with its length.
    fn sentence_tints(&mut self, visible: Range<usize>) -> Vec<(Range<usize>, Length)> {
        let Some(thresholds) = self.prose.rhythm else {
            return Vec::new();
        };
        let text = self.source.text();
        let cache = &mut self.prose.tints;
        if cache.len() > CACHE_LIMIT {
            cache.clear();
        }
        let mut found = Vec::new();
        for unit in units(self.source.tree(), visible, Purpose::Rhythm) {
            let start = unit.range.start;
            let key = key_of(&text[unit.range.clone()], Purpose::Rhythm, thresholds);
            let tints = cache.entry(key).or_insert_with(|| {
                sentence_lengths(text, &unit, thresholds)
                    .into_iter()
                    .map(|(range, length)| (range.start - start..range.end - start, length))
                    .collect()
            });
            found.extend(
                tints
                    .iter()
                    .map(|(range, length)| (range.start + start..range.end + start, *length)),
            );
        }
        found
    }

    /// The flags on screen, from the cache. Paragraphs with none cached,
    /// or stale ones, are queued for the checker.
    fn visible_flags(&mut self, visible: Range<usize>, cx: &mut Context<Self>) -> Vec<Flag> {
        self.prose.waiting = false;
        self.prose.visible = visible.clone();
        if !self.prose.grammar {
            return Vec::new();
        }
        let generation = checker::generation(cx);
        let ignored = checker::ignored(cx);
        let text = self.source.text();
        let typing_at = self.typing_at();
        let carried = std::mem::take(&mut self.prose.shown);
        if self.prose.flags.len() > CACHE_LIMIT {
            self.prose.flags.clear();
        }
        let mut shown = Vec::new();
        for unit in units(self.source.tree(), visible, Purpose::Grammar) {
            let slice = &text[unit.range.clone()];
            let key = key_of(slice, Purpose::Grammar, Thresholds::default());
            let start = unit.range.start;
            let Some(checked) = self.prose.flags.get(&key) else {
                // Not checked since it changed: what it showed, moved.
                self.prose.waiting = true;
                let is_ignored = |flag: &&Flag| {
                    text.get(flag.range.clone())
                        .is_none_or(|phrase| ignored.contains(&phrase.to_lowercase()))
                };
                shown.extend(
                    carried
                        .iter()
                        .filter(|flag| within(&flag.range, &unit.range) && !is_ignored(flag))
                        .cloned(),
                );
                continue;
            };
            self.prose.waiting |= checked.generation != generation;
            let is_ignored = |flag: &&Flag| {
                slice
                    .get(flag.range.clone())
                    .is_none_or(|phrase| ignored.contains(&phrase.to_lowercase()))
            };
            shown.extend(
                checked
                    .flags
                    .iter()
                    .filter(|flag| !is_ignored(flag))
                    .map(|flag| flag.shifted(start as isize))
                    .filter(|flag| Some(flag.range.end) != typing_at),
            );
        }
        if self.prose.waiting && self.prose.job.is_none() {
            self.schedule_check(cx);
        }
        shown
    }

    /// The paragraphs on screen whose flags are missing or stale, each
    /// on its own for the checker.
    fn grammar_jobs(&self, generation: u64) -> Vec<Job> {
        let text = self.source.text();
        let visible =
            self.prose.visible.start.min(text.len())..self.prose.visible.end.min(text.len());
        units(self.source.tree(), visible, Purpose::Grammar)
            .into_iter()
            .filter_map(|unit| {
                let slice = &text[unit.range.clone()];
                let key = key_of(slice, Purpose::Grammar, Thresholds::default());
                let current = self
                    .prose
                    .flags
                    .get(&key)
                    .is_some_and(|checked| checked.generation == generation);
                (!current).then(|| job_for(key, slice, &unit))
            })
            .collect()
    }

    /// Where the cursor sits while nothing is selected: the end of a word
    /// being typed isn't flagged until the cursor moves on.
    fn typing_at(&self) -> Option<usize> {
        let range = self.selected_range();
        range.is_empty().then_some(range.start)
    }

    /// Checks the waiting paragraphs once typing has paused for a whole
    /// [`CHECK_DELAY`].
    fn schedule_check(&mut self, cx: &mut Context<Self>) {
        self.prose.edited = false;
        self.prose.job = Some(cx.spawn(async move |view, cx| {
            loop {
                cx.background_executor().timer(CHECK_DELAY).await;
                let typing = view.update(cx, |view, _| std::mem::take(&mut view.prose.edited));
                match typing {
                    Ok(true) => continue,
                    Ok(false) => break,
                    Err(_) => return,
                }
            }
            let Ok((jobs, generation)) = view.update(cx, |view, cx| {
                let generation = checker::generation(cx);
                (view.grammar_jobs(generation), generation)
            }) else {
                return;
            };
            let Ok(reply) = cx.update(|cx| checker::check(jobs, cx)) else {
                return;
            };
            let checked = reply.await.unwrap_or_default();
            view.update(cx, |view, cx| {
                view.prose.job = None;
                for (key, flags) in checked {
                    let flags = flags.into();
                    view.prose.flags.insert(key, Checked { flags, generation });
                }
                cx.notify();
            })
            .ok();
        }));
    }
}

/// Moves `flags`, at note offsets, through an edit: ones after it shift
/// and ones it touches go, since what they flagged has changed.
fn carry_through(flags: &mut Vec<Flag>, edit: &Edit) {
    let delta = edit.new_len as isize - edit.old.len() as isize;
    flags.retain(|flag| flag.range.end < edit.old.start || flag.range.start > edit.old.end);
    for flag in flags.iter_mut() {
        if flag.range.start > edit.old.end {
            *flag = flag.shifted(delta);
        }
    }
}

/// Whether `inner` lies within `outer`.
fn within(inner: &Range<usize>, outer: &Range<usize>) -> bool {
    outer.start <= inner.start && inner.end <= outer.end
}

/// A cache key for a paragraph read for `purpose`.
fn key_of(slice: &str, purpose: Purpose, thresholds: Thresholds) -> u64 {
    let mut hasher = DefaultHasher::new();
    slice.hash(&mut hasher);
    purpose.hash(&mut hasher);
    thresholds.hash(&mut hasher);
    hasher.finish()
}

/// A paragraph for the checker, on its own: the text and pieces counted
/// from its start.
fn job_for(key: u64, slice: &str, unit: &Unit) -> Job {
    let start = unit.range.start;
    let pieces = unit
        .pieces
        .iter()
        .map(|piece| {
            Piece::new(
                piece.range.start - start..piece.range.end - start,
                piece.kind,
            )
        })
        .collect();
    Job {
        key,
        text: slice.to_owned(),
        unit: Unit {
            range: 0..slice.len(),
            pieces,
        },
    }
}

/// Rectangles covering `range`, one per row, a little shorter than the
/// row so a tint reads as a highlighter stroke on each line.
fn band_rects(frame: &FrameLayout, range: &Range<usize>, theme: &Theme) -> Vec<Bounds<Pixels>> {
    let first = frame
        .lines
        .partition_point(|placed| placed.visual.end() < range.start);
    let mut rects = Vec::new();
    for placed in frame.lines[first..]
        .iter()
        .take_while(|placed| placed.visual.start <= range.end)
    {
        let visual = &placed.visual;
        let from = range.start.max(visual.start) - visual.start;
        let to = range.end.min(visual.end()) - visual.start;
        for (_, row) in visual.caret_rows() {
            if to <= row.range.start || from >= row.range.end {
                continue;
            }
            let left = row.x_for(from.max(row.range.start));
            let right = row.x_for(to.min(row.range.end));
            if right <= left {
                continue;
            }
            let top = placed.top + row.top + row.caret_top + theme.sentence_tint_inset;
            let bottom =
                placed.top + row.top + row.caret_top + row.caret_height - theme.sentence_tint_inset;
            rects.push(Bounds::from_corners(
                point(frame.text_left + left, top),
                point(frame.text_left + right, bottom),
            ));
        }
    }
    rects
}

/// Where a flag's wavy underline goes on each row it covers.
fn underline_spans(
    frame: &FrameLayout,
    flag: &Flag,
    theme: &Theme,
) -> Vec<(Point<Pixels>, Pixels, Hsla)> {
    let color = match flag.kind {
        FlagKind::Spelling => theme.flag_spelling,
        FlagKind::Mechanical => theme.flag_mechanical,
    };
    band_rects(frame, &flag.range, theme)
        .into_iter()
        .map(|rect| {
            let origin = point(
                rect.left(),
                rect.bottom() + theme.sentence_tint_inset - theme.flag_underline_rise,
            );
            (origin, rect.size.width, color)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use editor_prose::FlagKind;

    use super::*;

    fn flag(range: Range<usize>) -> Flag {
        Flag {
            range,
            kind: FlagKind::Spelling,
            rule: String::new(),
            message: String::new(),
            replacements: Vec::new(),
        }
    }

    fn ranges(flags: &[Flag]) -> Vec<Range<usize>> {
        flags.iter().map(|flag| flag.range.clone()).collect()
    }

    #[test]
    fn flags_move_with_edits_before_them_and_go_when_touched() {
        let mut flags = vec![flag(0..4), flag(10..14), flag(20..24)];
        // Two bytes typed between the first and second flags.
        carry_through(
            &mut flags,
            &Edit {
                old: 6..6,
                new_len: 2,
            },
        );
        assert_eq!(ranges(&flags), [0..4, 12..16, 22..26]);
        // A letter typed at the end of the second flag's word.
        carry_through(
            &mut flags,
            &Edit {
                old: 16..16,
                new_len: 1,
            },
        );
        assert_eq!(ranges(&flags), [0..4, 23..27]);
        // Text deleted from inside the last one.
        carry_through(
            &mut flags,
            &Edit {
                old: 24..25,
                new_len: 0,
            },
        );
        assert_eq!(ranges(&flags), vec![0..4]);
    }
}
