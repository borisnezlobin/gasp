//! The left panel slot. It hosts any view (the file tree goes here), can be
//! resized and hidden, and follows `sidebar.files.reveal` and `.mode`.
//!
//! Hover reveal runs through the config crate's rules engine: the pointer
//! touching the window's left edge and leaving the panel are rule events,
//! and the default rules turn them into `sidebar.files.show` and, 300 ms
//! later, `sidebar.files.hide`.
//!
//! Whether the pointer is in the panel is worked out from where the panel
//! was last drawn, not from GPUI's hover, which ends whenever anything is
//! drawn over the panel (a menu's backdrop, a popover) or a drag is under
//! way. The panel also counts as in use while it has the keyboard, has a
//! menu open or is being resized, and a hide that comes due while it's in
//! use is dropped.

use std::time::{Duration, Instant};

use gasp_config::rules::Clock;
use gasp_config::settings::{SettingsIndex, SidebarMode, SidebarReveal};
use gasp_config::{Event, EventKind, MatchContext, Platform, RuleEngine, RuleSet, Settings};
use gpui::{AnyView, BackgroundExecutor, FocusHandle, Pixels, Point, Task};

use crate::ui::DrawnArea;

/// The rule target for the strip along the window's left edge.
pub const LEFT_EDGE_TARGET: &str = "window.left-edge";
/// The rule target for the panel itself.
pub const PANEL_TARGET: &str = "sidebar.files";

/// Time as GPUI's executor sees it, so tests can move it forward.
pub struct ExecutorClock {
    executor: BackgroundExecutor,
    start: Instant,
}

impl ExecutorClock {
    pub fn new(executor: BackgroundExecutor) -> Self {
        let start = executor.now();
        ExecutorClock { executor, start }
    }
}

impl Clock for ExecutorClock {
    fn now(&self) -> Duration {
        self.executor.now().saturating_duration_since(self.start)
    }
}

/// The left panel's state.
pub struct LeftPanel {
    view: Option<AnyView>,
    focus: Option<FocusHandle>,
    pub width: Pixels,
    reveal: SidebarReveal,
    mode: SidebarMode,
    /// Shown by the toggle, or always.
    pinned: bool,
    /// Shown for now by hovering or by keyboard focus.
    revealed: bool,
    rules: RuleEngine,
    settings: SettingsIndex,
    pub(crate) tick: Option<Task<()>>,
    area: DrawnArea,
    /// Whether the panel was in use when the rules last heard of it.
    in_use: bool,
    /// Where the pointer was last seen, in the window or leaving it.
    pointer: Option<Point<Pixels>>,
}

/// What keeps a panel shown on hover from hiding.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PanelHolds {
    /// The pointer is over the panel or the outer half of its edge.
    pub pointer: bool,
    /// The panel, or something in it, has the keyboard.
    pub focus: bool,
    /// A menu or prompt the panel opened is showing.
    pub menu: bool,
    /// The panel is being resized, or something picked up or carried
    /// over it is being dragged.
    pub drag: bool,
}

impl PanelHolds {
    pub fn any(self) -> bool {
        self.pointer || self.focus || self.menu || self.drag
    }
}

impl LeftPanel {
    pub fn new(settings: &Settings, rules: &RuleSet, width: Pixels) -> Self {
        let files = &settings.sidebar.files;
        LeftPanel {
            view: None,
            focus: None,
            width,
            reveal: files.reveal,
            mode: files.mode,
            pinned: files.reveal == SidebarReveal::Always,
            revealed: false,
            rules: RuleEngine::new(rules, Platform::current()),
            settings: SettingsIndex::new(settings),
            tick: None,
            area: DrawnArea::default(),
            in_use: false,
            pointer: None,
        }
    }

    /// Takes new settings and rules, as when the user changes them. The
    /// panel stays shown or hidden unless its reveal mode changed.
    pub fn apply_settings(&mut self, settings: &Settings, rules: &RuleSet) {
        let files = &settings.sidebar.files;
        if files.reveal != self.reveal {
            self.pinned = files.reveal == SidebarReveal::Always;
            self.revealed = false;
        }
        self.reveal = files.reveal;
        self.mode = files.mode;
        self.rules = RuleEngine::new(rules, Platform::current());
        self.settings = SettingsIndex::new(settings);
    }

    pub fn set_view(&mut self, view: AnyView, focus: Option<FocusHandle>) {
        self.view = Some(view);
        self.focus = focus;
    }

    pub fn view(&self) -> Option<&AnyView> {
        self.view.as_ref()
    }

    pub fn focus_handle(&self) -> Option<&FocusHandle> {
        self.focus.as_ref()
    }

    pub fn reveal(&self) -> SidebarReveal {
        self.reveal
    }

    pub fn mode(&self) -> SidebarMode {
        self.mode
    }

    pub fn is_visible(&self) -> bool {
        self.pinned || self.revealed
    }

    /// Whether the panel is only showing for now: revealed by hover or
    /// the keyboard, or open over the note it covers.
    pub fn is_passing(&self) -> bool {
        let covering = self.overlays() && self.reveal != SidebarReveal::Always;
        self.revealed || (covering && self.pinned)
    }

    /// Whether the panel covers the notes rather than moving them aside.
    pub fn overlays(&self) -> bool {
        self.mode == SidebarMode::Overlay
    }

    /// Whether the left-edge strip should listen for the pointer. With
    /// nothing in the panel, there's nothing to reveal.
    pub fn wants_edge(&self) -> bool {
        self.reveal == SidebarReveal::Hover && !self.is_visible() && self.view.is_some()
    }

    pub fn toggle(&mut self) {
        let visible = self.is_visible();
        self.pinned = !visible;
        self.revealed = false;
    }

    /// Shows the panel for now: pinned unless it reveals on hover.
    pub fn show(&mut self) {
        // The pointer (or the keyboard) that showed it is using it, so
        // leaving before the panel is first drawn still counts as leaving.
        self.in_use = true;
        if self.reveal == SidebarReveal::Hover {
            self.revealed = true;
        } else {
            self.pinned = true;
        }
    }

    pub fn hide(&mut self) {
        // A hidden panel isn't anywhere to be in.
        self.in_use = false;
        self.area.set(None);
        if self.reveal == SidebarReveal::Hover {
            self.revealed = false;
        } else {
            self.pinned = false;
        }
    }

    pub fn set_pinned(&mut self, pinned: bool) {
        self.pinned = pinned;
    }

    pub fn is_pinned(&self) -> bool {
        self.pinned
    }

    /// Where the panel was last drawn.
    pub fn area(&self) -> &DrawnArea {
        &self.area
    }

    /// Notes where the pointer is now, in the window or leaving it.
    pub fn set_pointer(&mut self, pointer: Point<Pixels>) {
        self.pointer = Some(pointer);
    }

    /// Where the pointer was last seen.
    pub fn pointer(&self) -> Option<Point<Pixels>> {
        self.pointer
    }

    /// Whether the rules last heard the panel was in use.
    pub fn in_use(&self) -> bool {
        self.in_use
    }

    /// The panel came into use or went out of it. Returns the rule event
    /// for the change, if there was one.
    pub fn set_in_use(&mut self, in_use: bool) -> Option<EventKind> {
        if in_use == self.in_use {
            return None;
        }
        self.in_use = in_use;
        Some(if in_use {
            EventKind::PointerEnter
        } else {
            EventKind::PointerLeave
        })
    }

    /// Runs a pointer event through the rules, returning the commands to run.
    pub fn pointer_event(
        &mut self,
        kind: EventKind,
        target: &str,
        clock: &dyn Clock,
    ) -> Vec<String> {
        let context = MatchContext {
            input: None,
            settings: Some(&self.settings),
        };
        self.rules
            .handle(&Event::at(kind, target), &context, clock)
            .into_iter()
            .map(|dispatch| dispatch.command)
            .collect()
    }

    /// Commands from delayed rules that are now due.
    pub fn tick(&mut self, clock: &dyn Clock) -> Vec<String> {
        self.rules
            .tick(clock)
            .into_iter()
            .map(|dispatch| dispatch.command)
            .collect()
    }

    /// How long until a delayed rule is due, if one is waiting.
    pub fn next_deadline(&self, clock: &dyn Clock) -> Option<Duration> {
        self.rules
            .next_deadline()
            .map(|due| due.saturating_sub(clock.now()))
    }
}

#[cfg(test)]
mod tests {
    use gasp_config::ManualClock;

    use super::*;

    fn panel(reveal: SidebarReveal) -> LeftPanel {
        let mut settings = Settings::default();
        settings.sidebar.files.reveal = reveal;
        LeftPanel::new(&settings, &RuleSet::defaults(), gpui::px(200.))
    }

    #[test]
    fn hover_rules_show_then_hide_after_a_delay() {
        let mut panel = panel(SidebarReveal::Hover);
        let clock = ManualClock::new();
        let shown = panel.pointer_event(EventKind::PointerEnter, LEFT_EDGE_TARGET, &clock);
        assert_eq!(shown, vec!["sidebar.files.show"]);
        assert!(
            panel
                .pointer_event(EventKind::PointerLeave, PANEL_TARGET, &clock)
                .is_empty()
        );
        clock.advance_ms(299);
        assert!(panel.tick(&clock).is_empty());
        clock.advance_ms(1);
        assert_eq!(panel.tick(&clock), vec!["sidebar.files.hide"]);
    }

    #[test]
    fn coming_back_cancels_the_hide() {
        let mut panel = panel(SidebarReveal::Hover);
        let clock = ManualClock::new();
        panel.pointer_event(EventKind::PointerLeave, PANEL_TARGET, &clock);
        panel.pointer_event(EventKind::PointerEnter, PANEL_TARGET, &clock);
        clock.advance_ms(400);
        assert!(panel.tick(&clock).is_empty());
    }

    #[test]
    fn only_a_change_in_use_is_an_event() {
        let mut panel = panel(SidebarReveal::Hover);
        assert_eq!(panel.set_in_use(false), None);
        assert_eq!(panel.set_in_use(true), Some(EventKind::PointerEnter));
        assert_eq!(panel.set_in_use(true), None);
        assert_eq!(panel.set_in_use(false), Some(EventKind::PointerLeave));
        panel.set_in_use(true);
        panel.hide();
        assert!(!panel.in_use(), "a hidden panel isn't in use");
    }

    #[test]
    fn hiding_forgets_where_the_panel_was() {
        let mut panel = panel(SidebarReveal::Hover);
        let inside = gpui::point(gpui::px(10.), gpui::px(10.));
        let size = gpui::size(gpui::px(200.), gpui::px(600.));
        let origin = gpui::point(gpui::px(0.), gpui::px(0.));
        panel.area().set(Some(gpui::Bounds::new(origin, size)));
        assert!(panel.area().contains(inside));
        panel.hide();
        assert!(!panel.area().contains(inside), "a hidden panel has no area");
    }

    #[test]
    fn hover_rules_are_off_for_the_toggle_setting() {
        let mut panel = panel(SidebarReveal::Toggle);
        let clock = ManualClock::new();
        assert!(
            panel
                .pointer_event(EventKind::PointerEnter, LEFT_EDGE_TARGET, &clock)
                .is_empty()
        );
        assert!(!panel.is_visible());
        panel.toggle();
        assert!(panel.is_visible());
        panel.toggle();
        assert!(!panel.is_visible());
    }

    #[test]
    fn always_starts_shown() {
        assert!(panel(SidebarReveal::Always).is_visible());
    }
}
