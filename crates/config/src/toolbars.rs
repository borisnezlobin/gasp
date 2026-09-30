//! Toolbars: rows of buttons for commands, and the status bar's widgets,
//! each with a place in the window, a behaviour that says when it shows,
//! and a style. They're described in `toolbars.toml`, whose built-in file
//! holds the status bar, the floating selection bar, and the iPhone's
//! keyboard bar and bottom bar.

use std::collections::BTreeMap;
use std::fmt;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use toml_edit::DocumentMut;

use crate::diagnostics::Diagnostic;
use crate::loader::Built;
use crate::rules::parse_duration;

pub const DEFAULT_TOOLBARS: &str = include_str!("../defaults/toolbars.toml");

/// Where a toolbar sits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Place {
    /// The row along the window's bottom edge.
    StatusBar,
    /// Above the notes, under the tab bars.
    EditorTop,
    /// Below the notes, above the status bar.
    EditorBottom,
    WindowLeft,
    WindowRight,
    /// Floating just above selected text.
    Selection,
    /// Floating at the end of the line the cursor is on.
    CursorLine,
    /// Above the iPhone's software keyboard.
    Keyboard,
    /// The iPhone's bar at the bottom of the screen, around the title of
    /// the note showing, which its spacer stands for.
    BrowserBar,
}

impl Place {
    pub const ALL: [Place; 9] = [
        Place::StatusBar,
        Place::EditorTop,
        Place::EditorBottom,
        Place::WindowLeft,
        Place::WindowRight,
        Place::Selection,
        Place::CursorLine,
        Place::Keyboard,
        Place::BrowserBar,
    ];

    /// Whether the bar floats over the note rather than taking a strip of
    /// the window.
    pub fn is_floating(self) -> bool {
        matches!(self, Place::Selection | Place::CursorLine)
    }

    /// Whether the bar runs down a side of the window.
    pub fn is_vertical(self) -> bool {
        matches!(self, Place::WindowLeft | Place::WindowRight)
    }

    /// Whether the desktop app draws bars here; the keyboard bar and the
    /// browser bar are the iPhone's.
    pub fn on_desktop(self) -> bool {
        !matches!(self, Place::Keyboard | Place::BrowserBar)
    }

    /// Whether a bar here can float over the note as a pill instead of
    /// taking a strip of its own.
    pub fn can_overlay(self) -> bool {
        matches!(
            self,
            Place::EditorTop | Place::EditorBottom | Place::WindowLeft | Place::WindowRight
        )
    }
}

/// What a docked bar sits on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Surface {
    /// A strip of its own beside the note, which the note makes room for.
    #[default]
    Strip,
    /// A pill floating over the note's edge, taking no room.
    Overlay,
}

impl Surface {
    pub const ALL: [Surface; 2] = [Surface::Strip, Surface::Overlay];
}

/// When a toolbar shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Behaviour {
    Always,
    /// When the pointer comes near the window edge it sits on.
    OnHover,
    /// Hidden while typing, and back once typing pauses.
    HideWhileTyping,
    /// Only while text is selected.
    WithSelection,
    /// Only while the cursor is in one of the toolbar's contexts.
    InContext,
}

impl Behaviour {
    pub const ALL: [Behaviour; 5] = [
        Behaviour::Always,
        Behaviour::OnHover,
        Behaviour::HideWhileTyping,
        Behaviour::WithSelection,
        Behaviour::InContext,
    ];
}

/// The kind of text the cursor is in, for [`Behaviour::InContext`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ToolbarContext {
    Text,
    Math,
    Code,
    Table,
}

impl ToolbarContext {
    pub const ALL: [ToolbarContext; 4] = [
        ToolbarContext::Text,
        ToolbarContext::Math,
        ToolbarContext::Code,
        ToolbarContext::Table,
    ];
}

/// How a command's button reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ButtonStyle {
    Icons,
    IconsAndLabels,
    Labels,
}

impl ButtonStyle {
    pub const ALL: [ButtonStyle; 3] = [
        ButtonStyle::Icons,
        ButtonStyle::IconsAndLabels,
        ButtonStyle::Labels,
    ];

    pub fn shows_icon(self) -> bool {
        self != ButtonStyle::Labels
    }

    pub fn shows_label(self) -> bool {
        self != ButtonStyle::Icons
    }
}

/// How much room a toolbar's buttons take.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Density {
    Compact,
    Comfortable,
}

impl Density {
    pub const ALL: [Density; 2] = [Density::Compact, Density::Comfortable];
}

/// A value of one of the choice fields, as the file spells it.
pub fn choice_name<T: Serialize>(value: T) -> String {
    match toml::Value::try_from(value) {
        Ok(toml::Value::String(name)) => name,
        _ => String::new(),
    }
}

/// Something the status bar shows that isn't a command.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Widget {
    WordCount,
    CharacterCount,
    ReadingTime,
    EditTime,
    CursorPosition,
    Sync,
}

/// (widget, name in the file, title, Phosphor icon)
const WIDGETS: [(Widget, &str, &str, &str); 6] = [
    (Widget::WordCount, "word-count", "Word count", "text-aa"),
    (
        Widget::CharacterCount,
        "character-count",
        "Character count",
        "text-t",
    ),
    (
        Widget::ReadingTime,
        "reading-time",
        "Reading time",
        "book-open",
    ),
    (Widget::EditTime, "edit-time", "Time spent editing", "clock"),
    (
        Widget::CursorPosition,
        "cursor-position",
        "Cursor position",
        "cursor-text",
    ),
    (Widget::Sync, "sync", "Sync status", "cloud-check"),
];

impl Widget {
    pub const ALL: [Widget; 6] = [
        Widget::WordCount,
        Widget::CharacterCount,
        Widget::ReadingTime,
        Widget::EditTime,
        Widget::CursorPosition,
        Widget::Sync,
    ];

    fn entry(self) -> (Widget, &'static str, &'static str, &'static str) {
        WIDGETS[self as usize]
    }

    pub fn name(self) -> &'static str {
        self.entry().1
    }

    pub fn title(self) -> &'static str {
        self.entry().2
    }

    pub fn icon(self) -> &'static str {
        self.entry().3
    }

    pub fn from_name(name: &str) -> Option<Widget> {
        WIDGETS
            .iter()
            .find(|(_, known, ..)| *known == name)
            .map(|(widget, ..)| *widget)
    }
}

pub const SEPARATOR: &str = "separator";
pub const SPACER: &str = "spacer";
pub const MENU_PREFIX: &str = "menu:";

/// One thing on a toolbar.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ToolbarItem {
    Command(String),
    Widget(Widget),
    /// A thin line between groups.
    Separator,
    /// Space that grows to push what follows to the far end.
    Spacer,
    /// A button that opens a dropdown of a `[menu.<id>]`'s commands.
    Menu(String),
}

impl ToolbarItem {
    /// Reads an item as a toolbar's `items` list spells it.
    pub fn parse(text: &str) -> ToolbarItem {
        let text = text.trim();
        if let Some(menu) = text.strip_prefix(MENU_PREFIX) {
            return ToolbarItem::Menu(menu.to_owned());
        }
        match text {
            SEPARATOR => ToolbarItem::Separator,
            SPACER => ToolbarItem::Spacer,
            _ => Widget::from_name(text).map_or_else(
                || ToolbarItem::Command(text.to_owned()),
                ToolbarItem::Widget,
            ),
        }
    }
}

impl fmt::Display for ToolbarItem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ToolbarItem::Command(id) => f.write_str(id),
            ToolbarItem::Widget(widget) => f.write_str(widget.name()),
            ToolbarItem::Separator => f.write_str(SEPARATOR),
            ToolbarItem::Spacer => f.write_str(SPACER),
            ToolbarItem::Menu(id) => write!(f, "{MENU_PREFIX}{id}"),
        }
    }
}

/// One `[toolbar.<id>]` table as written. Every field is optional so a
/// vault's file can change just one of a built-in toolbar's.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct ToolbarSpec {
    pub title: Option<String>,
    pub enabled: Option<bool>,
    pub place: Option<Place>,
    pub behaviour: Option<Behaviour>,
    pub contexts: Option<Vec<ToolbarContext>>,
    pub style: Option<ButtonStyle>,
    pub density: Option<Density>,
    pub surface: Option<Surface>,
    pub items: Option<Vec<String>>,
}

/// One `[menu.<id>]` table as written.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct MenuSpec {
    pub title: Option<String>,
    pub icon: Option<String>,
    pub items: Option<Vec<String>>,
}

/// The `[timing]` table as written, durations such as `300ms`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct TimingSpec {
    pub hover_delay: Option<String>,
    pub hide_delay: Option<String>,
    pub typing_pause: Option<String>,
}

/// The whole `toolbars.toml`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ToolbarsFile {
    pub timing: TimingSpec,
    pub toolbar: BTreeMap<String, ToolbarSpec>,
    pub menu: BTreeMap<String, MenuSpec>,
}

impl ToolbarSpec {
    fn layer(&mut self, overlay: &ToolbarSpec) {
        let ToolbarSpec {
            title,
            enabled,
            place,
            behaviour,
            contexts,
            style,
            density,
            surface,
            items,
        } = overlay.clone();
        self.title = title.or(self.title.take());
        self.enabled = enabled.or(self.enabled);
        self.place = place.or(self.place);
        self.behaviour = behaviour.or(self.behaviour);
        self.contexts = contexts.or(self.contexts.take());
        self.style = style.or(self.style);
        self.density = density.or(self.density);
        self.surface = surface.or(self.surface);
        self.items = items.or(self.items.take());
    }
}

impl MenuSpec {
    fn layer(&mut self, overlay: &MenuSpec) {
        let MenuSpec { title, icon, items } = overlay.clone();
        self.title = title.or(self.title.take());
        self.icon = icon.or(self.icon.take());
        self.items = items.or(self.items.take());
    }
}

impl TimingSpec {
    fn layer(&mut self, overlay: &TimingSpec) {
        let TimingSpec {
            hover_delay,
            hide_delay,
            typing_pause,
        } = overlay.clone();
        self.hover_delay = hover_delay.or(self.hover_delay.take());
        self.hide_delay = hide_delay.or(self.hide_delay.take());
        self.typing_pause = typing_pause.or(self.typing_pause.take());
    }
}

/// A toolbar as the app shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Toolbar {
    /// Its name in the file, such as `status`.
    pub id: String,
    pub title: String,
    pub enabled: bool,
    pub place: Place,
    pub behaviour: Behaviour,
    pub contexts: Vec<ToolbarContext>,
    pub style: ButtonStyle,
    pub density: Density,
    /// Only for places that [`Place::can_overlay`]; the rest ignore it.
    pub surface: Surface,
    pub items: Vec<ToolbarItem>,
    /// Whether the built-in file has it, so it's turned off rather than
    /// removed.
    pub built_in: bool,
}

/// A dropdown of commands that toolbars can hold.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolbarMenu {
    pub id: String,
    pub title: String,
    /// A Phosphor icon name.
    pub icon: String,
    pub items: Vec<String>,
}

/// How long hover reveal and typing pauses take.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ToolbarTiming {
    pub hover_delay: Duration,
    pub hide_delay: Duration,
    pub typing_pause: Duration,
}

/// Every toolbar and menu, in the order the settings screen lists them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Toolbars {
    pub timing: ToolbarTiming,
    pub toolbars: Vec<Toolbar>,
    pub menus: Vec<ToolbarMenu>,
}

/// What a toolbar's behaviour depends on at one moment.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ToolbarConditions {
    pub has_selection: bool,
    /// The kind of text the cursor is in, when a note has focus.
    pub context: Option<ToolbarContext>,
    /// Whether typing hasn't paused long enough yet.
    pub typing: bool,
    /// Whether the pointer is at the toolbar or the edge that reveals it.
    pub hovered: bool,
    /// Whether the keyboard is in the toolbar, which keeps any toolbar up.
    pub focused: bool,
}

impl Toolbar {
    /// Whether the toolbar shows now.
    pub fn is_shown(&self, conditions: &ToolbarConditions) -> bool {
        if !self.enabled {
            return false;
        }
        if conditions.focused {
            return true;
        }
        match self.behaviour {
            Behaviour::Always => true,
            Behaviour::OnHover => conditions.hovered,
            Behaviour::HideWhileTyping => !conditions.typing,
            Behaviour::WithSelection => conditions.has_selection,
            Behaviour::InContext => conditions
                .context
                .is_some_and(|context| self.contexts.contains(&context)),
        }
    }

    /// Whether the bar floats over the note as a pill.
    pub fn floats_over_note(&self) -> bool {
        self.surface == Surface::Overlay && self.place.can_overlay()
    }

    /// The command ids on the toolbar, in order.
    pub fn commands(&self) -> impl Iterator<Item = &str> {
        self.items.iter().filter_map(|item| match item {
            ToolbarItem::Command(id) => Some(id.as_str()),
            _ => None,
        })
    }
}

impl Toolbars {
    /// The built-in toolbars.
    pub fn defaults() -> Toolbars {
        build_toolbars("toolbars.toml", None, &[])
            .map(|(toolbars, _)| toolbars)
            .expect("the built-in toolbars load")
    }

    pub fn get(&self, id: &str) -> Option<&Toolbar> {
        self.toolbars.iter().find(|toolbar| toolbar.id == id)
    }

    pub fn menu(&self, id: &str) -> Option<&ToolbarMenu> {
        self.menus.iter().find(|menu| menu.id == id)
    }

    /// The turned-on toolbars at `place`, in order.
    pub fn at(&self, place: Place) -> impl Iterator<Item = &Toolbar> {
        self.toolbars
            .iter()
            .filter(move |toolbar| toolbar.enabled && toolbar.place == place)
    }
}

impl Default for Toolbars {
    fn default() -> Self {
        Toolbars::defaults()
    }
}

/// Whether `id` can name a toolbar or menu: lowercase letters, digits and
/// dashes, so it's a bare TOML key.
pub fn is_valid_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-')
}

/// The built-in toolbars with `user`'s layered on top. Warns about items
/// naming commands that aren't in `known_commands` (when it isn't empty)
/// and menus that don't exist.
pub fn build_toolbars(file: &str, user: Option<&str>, known_commands: &[&str]) -> Built<Toolbars> {
    let defaults = parse_file("defaults/toolbars.toml", DEFAULT_TOOLBARS)?;
    let default_order = table_order(DEFAULT_TOOLBARS);
    let text = user.unwrap_or_default();
    let overlay = match user {
        Some(text) => parse_file(file, text)?,
        None => ToolbarsFile::default(),
    };
    check_ids(file, text, &overlay)?;
    let timing = resolve_timing(file, text, &defaults.timing, &overlay.timing)?;
    let mut order = default_order.clone();
    order.extend(
        table_order(text)
            .into_iter()
            .filter(|id| !default_order.contains(id)),
    );
    let toolbars = order
        .iter()
        .filter_map(|id| resolve_toolbar(id, &defaults, &overlay))
        .collect();
    let toolbars = Toolbars {
        timing,
        toolbars,
        menus: resolve_menus(&defaults, &overlay),
    };
    let warnings = item_warnings(file, text, &overlay, &toolbars, known_commands);
    Ok((toolbars, warnings))
}

fn parse_file(file: &str, text: &str) -> Result<ToolbarsFile, Vec<Diagnostic>> {
    toml::from_str(text).map_err(|error| vec![Diagnostic::from_toml(file, text, &error)])
}

/// The ids of the `[toolbar.<id>]` tables in the order the file has them.
fn table_order(text: &str) -> Vec<String> {
    let Ok(doc) = text.parse::<DocumentMut>() else {
        return Vec::new();
    };
    doc.get("toolbar")
        .and_then(|item| item.as_table_like())
        .map(|table| table.iter().map(|(id, _)| id.to_owned()).collect())
        .unwrap_or_default()
}

fn span_of(text: &str, needle: &str) -> Option<std::ops::Range<usize>> {
    text.find(needle).map(|start| start..start + needle.len())
}

fn check_ids(file: &str, text: &str, overlay: &ToolbarsFile) -> Result<(), Vec<Diagnostic>> {
    let bad = overlay
        .toolbar
        .keys()
        .chain(overlay.menu.keys())
        .find(|id| !is_valid_id(id));
    match bad {
        Some(id) => Err(vec![Diagnostic::error(
            file,
            text,
            span_of(text, id),
            format!("`{id}` can only use lowercase letters, digits and dashes"),
        )]),
        None => Ok(()),
    }
}

fn duration(
    file: &str,
    text: &str,
    field: (&str, Option<&String>),
    fallback: &str,
) -> Result<Duration, Vec<Diagnostic>> {
    let (name, value) = field;
    let value = value.map_or(fallback, String::as_str);
    parse_duration(value).ok_or_else(|| {
        vec![Diagnostic::error(
            file,
            text,
            span_of(text, value),
            format!("`{name}` needs a time such as \"300ms\" or \"1s\""),
        )]
    })
}

fn resolve_timing(
    file: &str,
    text: &str,
    defaults: &TimingSpec,
    overlay: &TimingSpec,
) -> Result<ToolbarTiming, Vec<Diagnostic>> {
    let mut timing = defaults.clone();
    timing.layer(overlay);
    Ok(ToolbarTiming {
        hover_delay: duration(
            file,
            text,
            ("hover-delay", timing.hover_delay.as_ref()),
            "150ms",
        )?,
        hide_delay: duration(
            file,
            text,
            ("hide-delay", timing.hide_delay.as_ref()),
            "400ms",
        )?,
        typing_pause: duration(
            file,
            text,
            ("typing-pause", timing.typing_pause.as_ref()),
            "1s",
        )?,
    })
}

fn resolve_toolbar(id: &str, defaults: &ToolbarsFile, overlay: &ToolbarsFile) -> Option<Toolbar> {
    let built_in = defaults.toolbar.get(id);
    let mut spec = built_in.cloned().unwrap_or_default();
    if let Some(user) = overlay.toolbar.get(id) {
        spec.layer(user);
    } else if built_in.is_none() {
        return None;
    }
    Some(Toolbar {
        id: id.to_owned(),
        title: spec.title.unwrap_or_else(|| id.to_owned()),
        enabled: spec.enabled.unwrap_or(true),
        place: spec.place.unwrap_or(Place::EditorTop),
        behaviour: spec.behaviour.unwrap_or(Behaviour::Always),
        contexts: spec.contexts.unwrap_or_default(),
        style: spec.style.unwrap_or(ButtonStyle::Icons),
        density: spec.density.unwrap_or(Density::Compact),
        surface: spec.surface.unwrap_or_default(),
        items: spec
            .items
            .unwrap_or_default()
            .iter()
            .map(|item| ToolbarItem::parse(item))
            .collect(),
        built_in: built_in.is_some(),
    })
}

fn resolve_menus(defaults: &ToolbarsFile, overlay: &ToolbarsFile) -> Vec<ToolbarMenu> {
    let mut specs = defaults.menu.clone();
    for (id, spec) in &overlay.menu {
        specs.entry(id.clone()).or_default().layer(spec);
    }
    specs
        .into_iter()
        .map(|(id, spec)| ToolbarMenu {
            title: spec.title.unwrap_or_else(|| id.clone()),
            icon: spec.icon.unwrap_or_else(|| "dots-three".to_owned()),
            items: spec.items.unwrap_or_default(),
            id,
        })
        .collect()
}

/// Warnings for the items a vault's file names that are no command or
/// menu. With no known commands given, only menus are checked.
fn item_warnings(
    file: &str,
    text: &str,
    overlay: &ToolbarsFile,
    toolbars: &Toolbars,
    known_commands: &[&str],
) -> Vec<Diagnostic> {
    let toolbar_items = overlay
        .toolbar
        .values()
        .filter_map(|spec| spec.items.as_ref());
    let menu_items = overlay.menu.values().filter_map(|spec| spec.items.as_ref());
    toolbar_items
        .chain(menu_items)
        .flatten()
        .filter_map(|item| unknown_item(item, toolbars, known_commands))
        .map(|(item, message)| {
            let quoted = format!("\"{item}\"");
            Diagnostic::warning(file, text, span_of(text, &quoted), message)
        })
        .collect()
}

/// An item that names nothing, with why: (the item, the message).
fn unknown_item(
    item: &str,
    toolbars: &Toolbars,
    known_commands: &[&str],
) -> Option<(String, String)> {
    let message = match ToolbarItem::parse(item) {
        ToolbarItem::Command(id)
            if !known_commands.is_empty() && !known_commands.contains(&id.as_str()) =>
        {
            format!("no command called `{id}` is registered")
        }
        ToolbarItem::Menu(id) if toolbars.menu(&id).is_none() => {
            format!("there's no [menu.{id}] to open")
        }
        _ => return None,
    };
    Some((item.to_owned(), message))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_built_in_toolbars_are_the_status_bar_selection_and_the_phones_two() {
        let toolbars = Toolbars::defaults();
        let ids: Vec<&str> = toolbars.toolbars.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, ["status", "selection", "keyboard", "browser-bar"]);
        let status = toolbars.get("status").unwrap();
        assert_eq!(status.place, Place::StatusBar);
        assert_eq!(status.items[0], ToolbarItem::Spacer);
        assert_eq!(status.items[1], ToolbarItem::Widget(Widget::WordCount));
        let selection = toolbars.get("selection").unwrap();
        assert_eq!(selection.behaviour, Behaviour::WithSelection);
        assert!(
            !selection.enabled,
            "the selection bar waits to be turned on"
        );
        assert_eq!(toolbars.at(Place::Selection).count(), 0);
        assert!(
            toolbars
                .toolbars
                .iter()
                .all(|t| t.surface == Surface::Strip)
        );
        assert_eq!(
            selection.commands().collect::<Vec<_>>(),
            [
                "format.bold",
                "format.italic",
                "format.highlight",
                "format.link",
                "format.code"
            ]
        );
        assert_eq!(toolbars.timing.typing_pause, Duration::from_secs(1));
    }

    #[test]
    fn a_vault_changes_one_field_and_adds_toolbars_in_its_order() {
        let text = "\
[toolbar.zeta]
place = \"editor-bottom\"
items = [\"export.html\", \"separator\", \"menu:insert\"]

[toolbar.selection]
enabled = true

[toolbar.alpha]
place = \"window-left\"
";
        let (toolbars, warnings) = build_toolbars("toolbars.toml", Some(text), &[]).unwrap();
        assert!(warnings.is_empty(), "{warnings:?}");
        let ids: Vec<&str> = toolbars.toolbars.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "status",
                "selection",
                "keyboard",
                "browser-bar",
                "zeta",
                "alpha"
            ]
        );
        let selection = toolbars.get("selection").unwrap();
        assert!(selection.enabled);
        assert_eq!(selection.items.len(), 5, "the other fields stay built in");
        assert_eq!(toolbars.at(Place::Selection).count(), 1);
        let zeta = toolbars.get("zeta").unwrap();
        assert!(!zeta.built_in);
        assert_eq!(zeta.title, "zeta");
        assert_eq!(zeta.items[1], ToolbarItem::Separator);
        assert_eq!(zeta.items[2], ToolbarItem::Menu("insert".into()));
    }

    #[test]
    fn unknown_commands_and_menus_warn_and_bad_values_fail() {
        let text = "[toolbar.mine]\nitems = [\"no.such\", \"menu:nope\", \"format.bold\"]\n";
        let (_, warnings) = build_toolbars("toolbars.toml", Some(text), &["format.bold"]).unwrap();
        assert_eq!(warnings.len(), 2, "{warnings:?}");
        assert!(warnings.iter().all(|w| w.line == 2 && !w.is_error()));
        let bad_place = "[toolbar.mine]\nplace = \"ceiling\"\n";
        let errors = build_toolbars("toolbars.toml", Some(bad_place), &[]).unwrap_err();
        assert_eq!(errors[0].line, 2);
        let bad_time = "[timing]\ntyping-pause = \"soon\"\n";
        let errors = build_toolbars("toolbars.toml", Some(bad_time), &[]).unwrap_err();
        assert!(errors[0].message.contains("typing-pause"));
        let bad_id = "[toolbar.My_Bar]\n";
        assert!(build_toolbars("toolbars.toml", Some(bad_id), &[]).is_err());
    }

    #[test]
    fn behaviours_decide_when_a_toolbar_shows() {
        let mut toolbar = Toolbars::defaults().get("selection").unwrap().clone();
        toolbar.enabled = true;
        let idle = ToolbarConditions::default();
        assert!(!toolbar.is_shown(&idle));
        let selecting = ToolbarConditions {
            has_selection: true,
            ..idle
        };
        assert!(toolbar.is_shown(&selecting));
        toolbar.behaviour = Behaviour::InContext;
        toolbar.contexts = vec![ToolbarContext::Math];
        let in_math = ToolbarConditions {
            context: Some(ToolbarContext::Math),
            ..idle
        };
        assert!(toolbar.is_shown(&in_math));
        toolbar.behaviour = Behaviour::HideWhileTyping;
        let typing = ToolbarConditions {
            typing: true,
            ..idle
        };
        assert!(!toolbar.is_shown(&typing));
        let focused = ToolbarConditions {
            focused: true,
            ..typing
        };
        assert!(toolbar.is_shown(&focused), "the keyboard keeps it up");
        toolbar.enabled = false;
        assert!(!toolbar.is_shown(&focused));
    }

    #[test]
    fn a_bar_floats_over_the_note_only_where_it_can() {
        let text = "\
[toolbar.pill]
place   = \"editor-bottom\"
surface = \"overlay\"

[toolbar.side]
place   = \"window-left\"
surface = \"overlay\"

[toolbar.status]
surface = \"overlay\"
";
        let (toolbars, _) = build_toolbars("toolbars.toml", Some(text), &[]).unwrap();
        let pill = toolbars.get("pill").unwrap();
        assert_eq!(pill.surface, Surface::Overlay);
        assert!(pill.floats_over_note());
        assert!(toolbars.get("side").unwrap().floats_over_note());
        let status = toolbars.get("status").unwrap();
        assert_eq!(status.surface, Surface::Overlay, "the file's value is kept");
        assert!(!status.floats_over_note(), "the status bar stays a strip");
        let bad = "[toolbar.pill]\nsurface = \"ceiling\"\n";
        let errors = build_toolbars("toolbars.toml", Some(bad), &[]).unwrap_err();
        assert_eq!(errors[0].line, 2);
        assert_eq!(choice_name(Surface::Overlay), "overlay");
    }

    #[test]
    fn items_read_and_write_the_same() {
        for text in [
            "format.bold",
            "separator",
            "spacer",
            "word-count",
            "sync",
            "menu:insert",
        ] {
            assert_eq!(ToolbarItem::parse(text).to_string(), text);
        }
        assert_eq!(choice_name(Place::CursorLine), "cursor-line");
        assert_eq!(choice_name(ButtonStyle::IconsAndLabels), "icons-and-labels");
    }
}
