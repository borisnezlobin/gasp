//! Matching events against rules, with delayed rules that the opposite event cancels.

use std::collections::HashMap;
use std::time::Duration;

use super::{Clock, EventKind, Rule, RuleSet};
use crate::commands::Args;
use crate::keys::KeyChord;
use crate::platform::{InputContext, Platform};
use crate::settings::SettingsIndex;

/// Something that happened in the UI.
#[derive(Clone, Debug, PartialEq)]
pub struct Event {
    pub kind: EventKind,
    /// The UI target, such as `sidebar.files` or `window.left-edge`.
    pub at: Option<String>,
    /// The pressed chord for key events, already resolved (no `Mod`).
    pub chord: Option<KeyChord>,
}

impl Event {
    pub fn new(kind: EventKind) -> Event {
        Event {
            kind,
            at: None,
            chord: None,
        }
    }

    pub fn key(chord: KeyChord) -> Event {
        Event {
            chord: Some(chord),
            ..Event::new(EventKind::Key)
        }
    }

    pub fn at(kind: EventKind, target: &str) -> Event {
        Event {
            at: Some(target.to_string()),
            ..Event::new(kind)
        }
    }
}

/// The state rules can depend on when an event arrives.
#[derive(Clone, Copy, Debug, Default)]
pub struct MatchContext<'a> {
    /// The kind of text the cursor is in, if the editor has focus.
    pub input: Option<InputContext>,
    /// Current settings, for rules with `if` conditions.
    pub settings: Option<&'a SettingsIndex>,
}

/// A command a rule wants run.
#[derive(Clone, Debug, PartialEq)]
pub struct Dispatch {
    pub command: String,
    pub args: Args,
    pub rule_id: Option<String>,
}

impl Dispatch {
    fn from_rule(rule: &Rule) -> Dispatch {
        Dispatch {
            command: rule.command.clone(),
            args: rule.args.clone(),
            rule_id: rule.id.clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Trigger {
    Key(KeyChord),
    Target(EventKind, Option<String>),
}

#[derive(Clone, Debug)]
struct Pending {
    rule: usize,
    due: Duration,
    kind: EventKind,
    at: Option<String>,
}

/// Which pending rules an event cancels: an event of the first kind cancels
/// pending rules of the second kind at the same target.
const CANCELS: &[(EventKind, EventKind)] = &[
    (EventKind::PointerEnter, EventKind::PointerLeave),
    (EventKind::PointerLeave, EventKind::PointerEnter),
    (EventKind::Focus, EventKind::Blur),
    (EventKind::Blur, EventKind::Focus),
    (EventKind::Typing, EventKind::Idle),
];

/// Matches events to rules for one platform.
#[derive(Clone, Debug)]
pub struct RuleEngine {
    rules: Vec<Rule>,
    index: HashMap<Trigger, Vec<usize>>,
    pending: Vec<Pending>,
}

impl RuleEngine {
    /// Keeps the rules that apply on `platform` and indexes them by trigger.
    pub fn new(rules: &RuleSet, platform: Platform) -> RuleEngine {
        let rules: Vec<Rule> = rules
            .rules()
            .iter()
            .filter(|rule| rule.applies_on(platform))
            .cloned()
            .collect();
        let mut index: HashMap<Trigger, Vec<usize>> = HashMap::new();
        for (position, rule) in rules.iter().enumerate() {
            index
                .entry(trigger_of(rule, platform))
                .or_default()
                .push(position);
        }
        RuleEngine {
            rules,
            index,
            pending: Vec::new(),
        }
    }

    /// Handles one event: fires anything already due, cancels pending rules the
    /// event undoes, then runs or schedules the rules it matches.
    pub fn handle(
        &mut self,
        event: &Event,
        context: &MatchContext<'_>,
        clock: &dyn Clock,
    ) -> Vec<Dispatch> {
        let now = clock.now();
        let mut dispatches = self.fire_due(now);
        self.cancel_undone_by(event);
        for position in self.matching(event, context) {
            dispatches.extend(self.run_or_schedule(position, event, now));
        }
        dispatches
    }

    /// Fires pending rules whose delay has passed.
    pub fn tick(&mut self, clock: &dyn Clock) -> Vec<Dispatch> {
        self.fire_due(clock.now())
    }

    /// When the host should next call [`RuleEngine::tick`], if anything is pending.
    pub fn next_deadline(&self) -> Option<Duration> {
        self.pending.iter().map(|pending| pending.due).min()
    }

    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    fn matching(&self, event: &Event, context: &MatchContext<'_>) -> Vec<usize> {
        let candidates = self.candidates(event);
        let admitted = candidates
            .into_iter()
            .filter(|position| admits(&self.rules[*position], context));
        if event.kind != EventKind::Key {
            return admitted.collect();
        }
        admitted
            .max_by_key(|position| (specificity(&self.rules[*position]), *position))
            .into_iter()
            .collect()
    }

    fn candidates(&self, event: &Event) -> Vec<usize> {
        let triggers = match event.chord {
            Some(chord) if event.kind == EventKind::Key => vec![Trigger::Key(chord)],
            _ => vec![
                Trigger::Target(event.kind, event.at.clone()),
                Trigger::Target(event.kind, None),
            ],
        };
        let mut positions: Vec<usize> = triggers
            .iter()
            .filter_map(|trigger| self.index.get(trigger))
            .flatten()
            .copied()
            .collect();
        positions.sort_unstable();
        positions.dedup();
        positions
    }

    fn run_or_schedule(
        &mut self,
        position: usize,
        event: &Event,
        now: Duration,
    ) -> Option<Dispatch> {
        let rule = &self.rules[position];
        if rule.after.is_zero() {
            return Some(Dispatch::from_rule(rule));
        }
        if !self.pending.iter().any(|pending| pending.rule == position) {
            self.pending.push(Pending {
                rule: position,
                due: now + rule.after,
                kind: event.kind,
                at: event.at.clone(),
            });
        }
        None
    }

    fn cancel_undone_by(&mut self, event: &Event) {
        let undone: Vec<EventKind> = CANCELS
            .iter()
            .filter(|(by, _)| *by == event.kind)
            .map(|(_, undone)| *undone)
            .collect();
        self.pending
            .retain(|pending| !(undone.contains(&pending.kind) && pending.at == event.at));
    }

    fn fire_due(&mut self, now: Duration) -> Vec<Dispatch> {
        let (mut due, waiting): (Vec<Pending>, Vec<Pending>) = self
            .pending
            .drain(..)
            .partition(|pending| pending.due <= now);
        self.pending = waiting;
        due.sort_by_key(|pending| (pending.due, pending.rule));
        due.iter()
            .map(|pending| Dispatch::from_rule(&self.rules[pending.rule]))
            .collect()
    }
}

fn trigger_of(rule: &Rule, platform: Platform) -> Trigger {
    match rule.chord_for(platform) {
        Some(chord) => Trigger::Key(chord),
        None => Trigger::Target(rule.on, rule.at.clone()),
    }
}

fn admits(rule: &Rule, context: &MatchContext<'_>) -> bool {
    let context_ok = rule.when.is_none_or(|when| context.input == Some(when));
    context_ok && conditions_hold(rule, context.settings)
}

fn conditions_hold(rule: &Rule, settings: Option<&SettingsIndex>) -> bool {
    if rule.conditions.is_empty() {
        return true;
    }
    let Some(settings) = settings else {
        return false;
    };
    rule.conditions
        .iter()
        .all(|(key, expected)| settings.get(key) == Some(expected))
}

fn specificity(rule: &Rule) -> (bool, bool) {
    (rule.when.is_some(), !rule.conditions.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::ManualClock;

    const HOVER_RULES: &str = r#"
[[rule]]
id = "show"
on = "pointer.enter"
at = "window.left-edge"
do = "sidebar.files.show"
platform = "desktop"

[[rule]]
id = "hide"
on = "pointer.leave"
at = "sidebar.files"
after = "300ms"
do = "sidebar.files.hide"
"#;

    fn engine(text: &str, platform: Platform) -> RuleEngine {
        RuleEngine::new(&RuleSet::from_toml("rules.toml", text).unwrap(), platform)
    }

    fn commands(dispatches: &[Dispatch]) -> Vec<&str> {
        dispatches.iter().map(|d| d.command.as_str()).collect()
    }

    #[test]
    fn pointer_enter_runs_immediately() {
        let mut engine = engine(HOVER_RULES, Platform::Macos);
        let clock = ManualClock::new();
        let event = Event::at(EventKind::PointerEnter, "window.left-edge");
        let fired = engine.handle(&event, &MatchContext::default(), &clock);
        assert_eq!(commands(&fired), ["sidebar.files.show"]);
    }

    #[test]
    fn platform_filter_drops_rules() {
        let mut engine = engine(HOVER_RULES, Platform::Ios);
        let event = Event::at(EventKind::PointerEnter, "window.left-edge");
        let fired = engine.handle(&event, &MatchContext::default(), &ManualClock::new());
        assert!(fired.is_empty());
    }

    #[test]
    fn delayed_rule_fires_after_its_delay() {
        let mut engine = engine(HOVER_RULES, Platform::Linux);
        let clock = ManualClock::new();
        let leave = Event::at(EventKind::PointerLeave, "sidebar.files");
        assert!(
            engine
                .handle(&leave, &MatchContext::default(), &clock)
                .is_empty()
        );
        assert_eq!(engine.next_deadline(), Some(Duration::from_millis(300)));
        clock.advance_ms(299);
        assert!(engine.tick(&clock).is_empty());
        clock.advance_ms(1);
        assert_eq!(commands(&engine.tick(&clock)), ["sidebar.files.hide"]);
        assert_eq!(engine.pending_count(), 0);
    }

    #[test]
    fn re_entering_cancels_a_pending_hide() {
        let mut engine = engine(HOVER_RULES, Platform::Linux);
        let clock = ManualClock::new();
        let context = MatchContext::default();
        engine.handle(
            &Event::at(EventKind::PointerLeave, "sidebar.files"),
            &context,
            &clock,
        );
        clock.advance_ms(100);
        engine.handle(
            &Event::at(EventKind::PointerEnter, "sidebar.files"),
            &context,
            &clock,
        );
        clock.advance_ms(500);
        assert!(engine.tick(&clock).is_empty());
        assert_eq!(engine.next_deadline(), None);
    }

    #[test]
    fn entering_a_different_target_does_not_cancel() {
        let mut engine = engine(HOVER_RULES, Platform::Linux);
        let clock = ManualClock::new();
        let context = MatchContext::default();
        engine.handle(
            &Event::at(EventKind::PointerLeave, "sidebar.files"),
            &context,
            &clock,
        );
        engine.handle(
            &Event::at(EventKind::PointerEnter, "editor"),
            &context,
            &clock,
        );
        clock.advance_ms(300);
        assert_eq!(commands(&engine.tick(&clock)), ["sidebar.files.hide"]);
    }

    #[test]
    fn late_events_fire_overdue_rules_first() {
        let mut engine = engine(HOVER_RULES, Platform::Linux);
        let clock = ManualClock::new();
        let context = MatchContext::default();
        engine.handle(
            &Event::at(EventKind::PointerLeave, "sidebar.files"),
            &context,
            &clock,
        );
        clock.advance_ms(400);
        let fired = engine.handle(
            &Event::at(EventKind::PointerEnter, "sidebar.files"),
            &context,
            &clock,
        );
        assert_eq!(commands(&fired), ["sidebar.files.hide"]);
    }

    #[test]
    fn typing_cancels_pending_idle_rules() {
        let text = "[[rule]]\non = \"idle\"\nafter = \"2s\"\ndo = \"sync.now\"\n";
        let mut engine = engine(text, Platform::Linux);
        let clock = ManualClock::new();
        let context = MatchContext::default();
        engine.handle(&Event::new(EventKind::Idle), &context, &clock);
        clock.advance_ms(1_000);
        engine.handle(&Event::new(EventKind::Typing), &context, &clock);
        clock.advance_ms(5_000);
        assert!(engine.tick(&clock).is_empty());
    }

    #[test]
    fn key_rules_resolve_mod_per_platform() {
        let text = "[[rule]]\non = \"key\"\nkeys = \"Mod+B\"\ndo = \"format.bold\"\n";
        let context = MatchContext::default();
        let clock = ManualClock::new();
        let mut mac = engine(text, Platform::Macos);
        let cmd_b = Event::key(KeyChord::parse("Cmd+B").unwrap());
        let ctrl_b = Event::key(KeyChord::parse("Ctrl+B").unwrap());
        assert_eq!(
            commands(&mac.handle(&cmd_b, &context, &clock)),
            ["format.bold"]
        );
        assert!(mac.handle(&ctrl_b, &context, &clock).is_empty());
        let mut linux = engine(text, Platform::Linux);
        assert_eq!(
            commands(&linux.handle(&ctrl_b, &context, &clock)),
            ["format.bold"]
        );
    }

    #[test]
    fn context_specific_key_rule_beats_generic_one() {
        let text = "[[rule]]\non = \"key\"\nkeys = \"Mod+E\"\ndo = \"format.code\"\n\
                    [[rule]]\non = \"key\"\nkeys = \"Mod+E\"\nwhen = \"math\"\ndo = \"math.edit\"\n\
                    [[rule]]\non = \"key\"\nkeys = \"Mod+E\"\ndo = \"format.code\"\n";
        let mut engine = engine(text, Platform::Linux);
        let clock = ManualClock::new();
        let chord = Event::key(KeyChord::parse("Ctrl+E").unwrap());
        let in_math = MatchContext {
            input: Some(InputContext::Math),
            settings: None,
        };
        assert_eq!(
            commands(&engine.handle(&chord, &in_math, &clock)),
            ["math.edit"]
        );
        let in_text = MatchContext {
            input: Some(InputContext::Text),
            settings: None,
        };
        assert_eq!(
            commands(&engine.handle(&chord, &in_text, &clock)),
            ["format.code"]
        );
    }

    #[test]
    fn setting_conditions_gate_rules() {
        let text = "[[rule]]\non = \"pointer.enter\"\nat = \"window.left-edge\"\n\
                    if = { \"sidebar.files.reveal\" = \"hover\" }\ndo = \"sidebar.files.show\"\n";
        let mut engine = engine(text, Platform::Linux);
        let clock = ManualClock::new();
        let event = Event::at(EventKind::PointerEnter, "window.left-edge");
        let mut settings = crate::settings::Settings::default();
        let hover = SettingsIndex::new(&settings);
        let context = MatchContext {
            input: None,
            settings: Some(&hover),
        };
        assert_eq!(engine.handle(&event, &context, &clock).len(), 1);
        settings.sidebar.files.reveal = crate::settings::SidebarReveal::Toggle;
        let toggle = SettingsIndex::new(&settings);
        let context = MatchContext {
            input: None,
            settings: Some(&toggle),
        };
        assert!(engine.handle(&event, &context, &clock).is_empty());
    }

    #[test]
    fn rules_without_a_target_match_any_target() {
        let text = "[[rule]]\non = \"file.open\"\ndo = \"outline.refresh\"\n";
        let mut engine = engine(text, Platform::Linux);
        let event = Event::at(EventKind::FileOpen, "notes/a.md");
        let fired = engine.handle(&event, &MatchContext::default(), &ManualClock::new());
        assert_eq!(commands(&fired), ["outline.refresh"]);
    }
}
