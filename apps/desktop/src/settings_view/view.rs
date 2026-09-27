//! The settings screen's state: which page is showing, which control has
//! focus, and what the vault's settings, theme and key rules hold now.

use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use editor_config::schema::SettingKind;
use editor_config::theme::Theme as Tokens;
use editor_config::{Config, Platform, RuleSet};
use gpui::{
    App, AppContext, Context, DismissEvent, Entity, EventEmitter, FocusHandle, Focusable,
    ListAlignment, ListOffset, ListState, Subscription, Window, px,
};
use serde_json::Value;

use super::capture::Capture;
use super::config_files;
use super::menu::OpenMenu;
use super::model::{
    ACCENT_DESCRIPTION, ACCENT_TITLE, ACCENT_TOKEN, DARK_ACCENT_TOKEN, FontSlot, PAGES, Page,
    PageSpec, RowSpec, SettingItem, ShortcutQuery, ShortcutRow, map_name_label, map_names,
    page_cards, setting_items, shortcut_rows, theme_number_items, words_match,
};
use super::snippet_editor::SnippetEditor;
use super::snippets_page::{ReplacementRow, SnippetRow, TypingLists};
use super::store::{SettingsFile, settings_path};
use crate::editor::EditorView;
use crate::text_input::{TextInput, TextInputEvent, TextInputStyle};
use crate::theme::{ACCENT_CHOICES, DARK_ACCENT_CHOICES, KeycapTheme, SettingsTheme, Theme};

/// What the settings screen tells its host about files it wrote.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SettingsEvent {
    /// Something was written: a setting's dotted key such as `files.trash`
    /// (`.editor/settings.toml`), `theme.` and a token such as
    /// `theme.font.text` (`.editor/theme.toml`), or `rules`
    /// (`.editor/rules.toml`).
    Changed(String),
}

/// What the settings screen asks its host to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SettingsRequest {
    /// Run a command, such as `vault.open`, once the screen has closed.
    RunCommand(String),
}

/// Where keyboard focus is on the screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingsFocus {
    Search,
    Sections,
    /// A row on the current page, by index into [`SettingsView::rows`].
    Control(usize),
}

/// One row on the current page.
#[derive(Clone, Debug, PartialEq)]
pub enum ControlRow {
    Setting(SettingItem),
    /// The field that adds an entry to a map setting.
    MapAdd(SettingItem),
    /// One entry of a map setting, such as a per-syntax override.
    MapEntry {
        map: SettingItem,
        item: SettingItem,
    },
    /// A theme font, chosen from the system's fonts.
    Font(FontSlot),
    /// The theme's accent colour.
    Accent,
    /// The vault's folder, with a button to open another vault.
    Vault,
    /// The app's version. It has no control, so focus skips it.
    Version,
    Shortcut(ShortcutRow),
    /// The address of the repository the vault syncs with.
    SyncRemote,
    /// The token sync signs in with.
    SyncAccount,
    /// The field that adds an entry to a list setting.
    ListAdd(SettingItem),
    /// One entry of a list setting, such as a device-only pattern.
    ListEntry {
        list: SettingItem,
        value: String,
    },
    /// Where the snippets come from, with a button to add one.
    SnippetsFile,
    Snippet(SnippetRow),
    /// The editor open on a snippet, drawn under its row.
    SnippetEditor,
    Replacement(ReplacementRow),
}

impl ControlRow {
    /// The setting this row edits.
    pub fn item(&self) -> Option<&SettingItem> {
        match self {
            ControlRow::Setting(item) | ControlRow::MapAdd(item) | ControlRow::ListAdd(item) => {
                Some(item)
            }
            ControlRow::MapEntry { item, .. } => Some(item),
            _ => None,
        }
    }

    /// Whether the row is edited through a text field.
    pub fn uses_field(&self) -> bool {
        match self {
            ControlRow::Setting(item) => item.kind == SettingKind::Text,
            ControlRow::MapAdd(item) => map_names(&item.key).is_none(),
            ControlRow::SyncRemote | ControlRow::ListAdd(_) | ControlRow::SnippetEditor => true,
            _ => false,
        }
    }

    /// Whether keyboard focus can land on the row.
    pub fn is_focusable(&self) -> bool {
        *self != ControlRow::Version
    }

    /// Whether the row is on the Snippets page's lists.
    pub fn is_typing_row(&self) -> bool {
        matches!(
            self,
            ControlRow::SnippetsFile
                | ControlRow::Snippet(_)
                | ControlRow::SnippetEditor
                | ControlRow::Replacement(_)
        )
    }

    /// The title of a row on the Snippets page: what's typed.
    fn typing_title(&self) -> String {
        match self {
            ControlRow::Snippet(row) => row.trigger.clone(),
            ControlRow::Replacement(row) => row.from.clone(),
            ControlRow::SnippetEditor => "Edit snippet".to_string(),
            _ => "Snippets".to_string(),
        }
    }

    /// The row's title, as the screen shows it.
    pub fn title(&self) -> String {
        match self {
            ControlRow::Setting(item) | ControlRow::MapAdd(item) => item.title.clone(),
            ControlRow::MapEntry { item, .. } => item.title.clone(),
            ControlRow::Font(slot) => slot.title().to_string(),
            ControlRow::Accent => ACCENT_TITLE.to_string(),
            ControlRow::Vault => "Vault".to_string(),
            ControlRow::Version => format!("Version {}", env!("CARGO_PKG_VERSION")),
            ControlRow::Shortcut(shortcut) => shortcut.title.clone(),
            ControlRow::SyncRemote => "Notes repository".to_string(),
            ControlRow::SyncAccount => "GitHub token".to_string(),
            ControlRow::ListAdd(item) => item.title.clone(),
            ControlRow::ListEntry { value, .. } => value.clone(),
            ControlRow::SnippetsFile
            | ControlRow::Snippet(_)
            | ControlRow::SnippetEditor
            | ControlRow::Replacement(_) => self.typing_title(),
        }
    }
}

/// A group of rows drawn on one card.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Card {
    /// Shown above the card, for groups of shortcuts.
    pub title: Option<String>,
    /// The card's rows, as indices into [`SettingsView::rows`].
    pub rows: Range<usize>,
}

/// The rows of a page and the cards they sit on.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PaneLayout {
    pub rows: Vec<ControlRow>,
    pub cards: Vec<Card>,
}

impl PaneLayout {
    pub(super) fn push_card(&mut self, title: Option<String>, rows: Vec<ControlRow>) {
        if rows.is_empty() {
            return;
        }
        let start = self.rows.len();
        self.rows.extend(rows);
        self.cards.push(Card {
            title,
            rows: start..self.rows.len(),
        });
    }
}

/// How far past the visible rows the page's list lays out, in pixels,
/// so rows reached by keyboard are measured before they're scrolled to.
const LIST_OVERDRAW: f32 = 400.;

/// What the page's list was last given: which page, for which search,
/// and which items.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct ListShows {
    pub page: Option<Page>,
    pub query: String,
    pub items: Rc<[super::render::PaneItem]>,
}

/// The pages that match the search, each with its rows, built once per
/// change to the search, the settings file or the rules rather than on
/// every call: a frame asks for them once per row.
#[derive(Default)]
pub(super) struct Layouts {
    pages: Vec<(Page, Rc<PaneLayout>)>,
}

/// The settings screen for one vault, sized as a modal over the window.
pub struct SettingsView {
    pub(super) focus_handle: FocusHandle,
    pub(super) vault_root: PathBuf,
    pub(super) style: SettingsTheme,
    /// How shortcuts are drawn, shared with the rest of the app.
    pub(super) keycaps: KeycapTheme,
    pub(super) items: Vec<SettingItem>,
    pub(super) rules: RuleSet,
    pub(super) shortcuts: Vec<ShortcutRow>,
    pub(super) file: SettingsFile,
    pub(super) tokens: Tokens,
    /// Whether the screen draws, and edits the accent of, the dark theme.
    pub(super) dark: bool,
    pub(super) font_names: Vec<String>,
    /// The family each theme font draws with: the one it names, or a
    /// fallback when the system doesn't have it.
    pub(super) shown_fonts: [String; 3],
    pub(super) search: Entity<TextInput>,
    pub(super) query: String,
    pub(super) current: usize,
    pub(super) focus: SettingsFocus,
    pub(super) fields: HashMap<String, Entity<TextInput>>,
    pub(super) hex_field: Entity<TextInput>,
    pub(super) number_edit: Option<(String, String)>,
    pub(super) error: Option<(String, String)>,
    pub(super) menu: Option<OpenMenu>,
    pub(super) capture: Option<Capture>,
    /// The page's scrolling list, which builds only the rows in view.
    pub(super) list: ListState,
    /// The page, search and item count the list was last given.
    pub(super) list_shows: RefCell<Option<ListShows>>,
    pub(super) layouts: RefCell<Option<Rc<Layouts>>>,
    /// The vault's sync, for the Sync page.
    pub(super) sync: Option<Entity<crate::sync::SyncService>>,
    pub(super) remote_cache: Option<String>,
    pub(super) signed_in_cache: bool,
    /// The snippets and replacements the Snippets page lists.
    pub(super) typing_lists: TypingLists,
    pub(super) snippet_editor: Option<SnippetEditor>,
    /// The math snippet rows show as what they give, rendered as the
    /// rows come into view and kept for when they come back.
    pub(super) math: crate::preview::math::MathStore,
    /// A sample note in the vault's current look, shown above the
    /// Appearance page's controls: the screen covers the notes, so a
    /// change shows here as it's made.
    pub(super) preview: Option<Entity<EditorView>>,
    pub(super) _subscriptions: Vec<Subscription>,
}

/// The note the Appearance page's preview shows: a heading, emphasis, a
/// link, code and math, so each font and colour setting has something to
/// change.
pub const PREVIEW_NOTE: &str = "## Wave packets\n\nA *wave packet* is a sum of waves whose **phases** agree in one place, so it moves like a particle. See [[Fourier series]], `np.fft` and $\\omega = ck$.";

impl EventEmitter<SettingsEvent> for SettingsView {}
impl EventEmitter<SettingsRequest> for SettingsView {}
impl EventEmitter<DismissEvent> for SettingsView {}

impl Focusable for SettingsView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

/// The key a map setting's "add" field is stored under.
pub(super) fn add_field_key(map_key: &str) -> String {
    format!("{map_key}+")
}

/// The key a theme token's changes are reported and errors kept under.
pub(super) fn theme_key(token: &str) -> String {
    format!("theme.{token}")
}

impl SettingsView {
    /// A settings screen for the vault at `vault_root`, reading its
    /// settings, theme and key rules from `.editor/`.
    pub fn new(
        vault_root: impl Into<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let vault_root = vault_root.into();
        let rules = config_files::load_rules(&vault_root);
        Self::with_rules(vault_root, &rules, window, cx)
    }

    /// A settings screen that lists the shortcuts in `rules`.
    pub fn with_rules(
        vault_root: impl Into<PathBuf>,
        rules: &RuleSet,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let vault_root = vault_root.into();
        let search = cx.new(|cx| {
            TextInput::new(window, cx)
                .with_placeholder("Search settings")
                .with_style(TextInputStyle::Query)
        });
        let hex_field = cx.new(|cx| {
            TextInput::new(window, cx)
                .with_placeholder("#000000")
                .with_style(TextInputStyle::Query)
        });
        let tokens = config_files::load_tokens(&vault_root);
        let font_names = crate::ui::installed_fonts(cx).to_vec();
        let mut view = SettingsView {
            focus_handle: cx.focus_handle(),
            file: SettingsFile::load(&settings_path(&vault_root)).unwrap_or_default(),
            vault_root,
            style: SettingsTheme::default(),
            keycaps: crate::ui::ui_theme(cx).keycap,
            items: setting_items()
                .into_iter()
                .chain(theme_number_items(config_files::default_number))
                .collect(),
            shortcuts: shortcut_rows(rules, Platform::current()),
            rules: rules.clone(),
            tokens,
            dark: crate::ui::is_dark(cx),
            font_names,
            shown_fonts: Default::default(),
            search,
            hex_field,
            query: String::new(),
            current: 0,
            focus: SettingsFocus::Sections,
            fields: HashMap::new(),
            number_edit: None,
            error: None,
            menu: None,
            capture: None,
            list: ListState::new(0, ListAlignment::Top, px(LIST_OVERDRAW)),
            list_shows: RefCell::default(),
            layouts: RefCell::default(),
            sync: None,
            remote_cache: None,
            signed_in_cache: false,
            typing_lists: TypingLists::default(),
            snippet_editor: None,
            math: Default::default(),
            preview: None,
            _subscriptions: Vec::new(),
        };
        view.typing_lists = TypingLists::load(&view.vault_root);
        view.restyle();
        let mut subscriptions = view.watch_inputs(window, cx);
        subscriptions.extend(view.build_fields(window, cx));
        view._subscriptions = subscriptions;
        view.sync_fields(cx);
        view
    }

    fn watch_inputs(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Vec<Subscription> {
        let search = self.search.clone();
        let hex = self.hex_field.clone();
        vec![
            cx.subscribe_in(&search, window, Self::on_search_event),
            cx.on_focus(&search.focus_handle(cx), window, |view, _, cx| {
                view.focus = SettingsFocus::Search;
                cx.notify();
            }),
            cx.subscribe_in(&hex, window, Self::on_hex_event),
            cx.on_focus(&hex.focus_handle(cx), window, |view, _, cx| {
                view.focus_row_where(|row| *row == ControlRow::Accent, cx);
            }),
        ]
    }

    /// Text fields for text settings and for adding map entries.
    fn build_fields(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Vec<Subscription> {
        let keys: Vec<(String, bool)> = self
            .items
            .iter()
            .filter_map(|item| match &item.kind {
                SettingKind::Text => Some((item.key.clone(), false)),
                SettingKind::Map(_) | SettingKind::List(_) => {
                    Some((add_field_key(&item.key), true))
                }
                _ => None,
            })
            .chain([(super::sync_page::REMOTE_FIELD.to_string(), false)])
            .collect();
        let mut subscriptions = Vec::new();
        for (key, adds) in keys {
            let placeholder = super::sync_page::placeholder(&key, adds);
            let field = cx.new(|cx| {
                TextInput::new(window, cx)
                    .with_placeholder(placeholder)
                    .with_style(TextInputStyle::Query)
            });
            let event_key = key.clone();
            subscriptions.push(cx.subscribe_in(
                &field,
                window,
                move |view, _, event, window, cx| {
                    view.on_field_event(&event_key, event, window, cx);
                },
            ));
            let focus_key = key.clone();
            subscriptions.push(
                cx.on_focus(&field.focus_handle(cx), window, move |view, _, cx| {
                    view.focus_field_row(&focus_key, cx);
                }),
            );
            self.fields.insert(key, field);
        }
        subscriptions
    }

    /// Rebuilds the screen's own look from the theme tokens, with fonts the
    /// system doesn't have replaced by ones it does.
    pub(super) fn restyle(&mut self) {
        let tokens = self.tokens.for_mode(self.dark);
        let mut theme = Theme::from_tokens(tokens, 12);
        theme.resolve_fonts(&self.font_names);
        let mut style = SettingsTheme::from_tokens(tokens);
        self.shown_fonts = [
            theme.body_font_family.to_string(),
            theme.ui_font_family.to_string(),
            theme.code_font_family.to_string(),
        ];
        style.font_family = theme.ui_font_family;
        style.code_font_family = theme.code_font_family;
        self.style = style;
    }

    /// Follows the app into light or dark mode.
    pub(super) fn follow_theme(&mut self, cx: &mut Context<Self>) {
        let dark = crate::ui::is_dark(cx);
        if dark != self.dark {
            self.dark = dark;
            self.keycaps = crate::ui::ui_theme(cx).keycap;
            self.restyle();
            self.sync_fields(cx);
        }
    }

    /// The token the accent row edits: each mode has its own accent.
    pub(super) fn accent_token(&self) -> &'static str {
        if self.dark {
            DARK_ACCENT_TOKEN
        } else {
            ACCENT_TOKEN
        }
    }

    /// The swatches the accent row offers in the current mode.
    pub(super) fn accent_choices(&self) -> &'static [&'static str] {
        if self.dark {
            &DARK_ACCENT_CHOICES
        } else {
            &ACCENT_CHOICES
        }
    }

    /// The family a theme font draws with now.
    pub fn shown_font(&self, slot: FontSlot) -> &str {
        let index = FontSlot::ALL.iter().position(|s| *s == slot).unwrap_or(0);
        &self.shown_fonts[index]
    }

    /// Why a theme font draws with another family, when it does.
    pub(super) fn font_note(&self, slot: FontSlot) -> Option<String> {
        let wanted = self.token(slot.token())?;
        let shown = self.shown_font(slot);
        (!self.font_names.is_empty() && wanted != shown)
            .then(|| format!("{wanted} isn’t installed, so {shown} shows instead."))
    }

    // ---- Public API ----

    pub fn vault_root(&self) -> &Path {
        &self.vault_root
    }

    /// Re-reads the settings, theme and rules files, such as after they
    /// changed on disk.
    pub fn reload(&mut self, cx: &mut Context<Self>) {
        if let Ok(file) = SettingsFile::load(&settings_path(&self.vault_root)) {
            self.file = file;
            self.invalidate_layouts();
        }
        self.tokens = config_files::load_tokens(&self.vault_root);
        self.typing_lists = TypingLists::load(&self.vault_root);
        self.invalidate_layouts();
        self.restyle();
        self.set_rules(&config_files::load_rules(&self.vault_root), cx);
        self.sync_fields(cx);
        cx.notify();
    }

    /// Replaces the shortcuts list, such as after `rules.toml` changed.
    pub fn set_rules(&mut self, rules: &RuleSet, cx: &mut Context<Self>) {
        self.rules = rules.clone();
        self.shortcuts = shortcut_rows(rules, Platform::current());
        self.invalidate_layouts();
        cx.notify();
    }

    /// Replaces the font names the font menus offer. The screen reads them
    /// from the system when it opens; tests set their own.
    pub fn set_font_names(&mut self, names: Vec<String>, cx: &mut Context<Self>) {
        self.font_names = names;
        self.restyle();
        cx.notify();
    }

    /// Focuses the search box, ready to type.
    pub fn focus_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.set_focus(SettingsFocus::Search, window, cx);
    }

    /// Focuses row `index` of the current page, as the arrows would.
    pub fn focus_control(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.set_focus(SettingsFocus::Control(index), window, cx);
    }

    /// Searches for `text`, as typing it into the search box would.
    pub fn search(&mut self, text: &str, cx: &mut Context<Self>) {
        self.set_query(text, cx);
    }

    /// Shows a page by id, such as `files` or
    /// [`super::model::SHORTCUTS_SECTION`]. Clears any search.
    pub fn show_section(&mut self, id: &str, cx: &mut Context<Self>) {
        self.set_query("", cx);
        let index = self
            .visible_sections()
            .iter()
            .position(|page| PageSpec::get(*page).id == id);
        if let Some(index) = index {
            self.select_section(index, cx);
        }
    }

    /// Scrolls row `index` of the current page into view. The page draws
    /// only the rows in view, so a row has bounds once it's been revealed.
    pub fn reveal_row(&mut self, index: usize, cx: &mut Context<Self>) {
        self.list.scroll_to_reveal_item(self.child_index(index));
        cx.notify();
    }

    /// Focuses the section list.
    pub fn focus_sections(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.set_focus(SettingsFocus::Sections, window, cx);
    }

    pub fn focus_state(&self) -> SettingsFocus {
        self.focus
    }

    /// The page showing on the right.
    /// Styles the Appearance page's preview by `config`, making it the
    /// first time.
    pub fn set_preview_config(&mut self, config: &Config, cx: &mut Context<Self>) {
        match &self.preview {
            Some(preview) => preview.update(cx, |preview, cx| {
                preview.apply_config(config, cx);
                cx.notify();
            }),
            None => {
                let radius = self.style.card_radius;
                let preview = cx.new(|cx| {
                    let mut preview = EditorView::with_config(PREVIEW_NOTE, Vec::new(), config, cx);
                    preview.read_only = true;
                    preview.corner_radius = radius;
                    preview
                });
                self.preview = Some(preview);
            }
        }
        cx.notify();
    }

    /// The preview, while the page it belongs on is showing.
    pub fn shown_preview(&self) -> Option<&Entity<EditorView>> {
        self.preview
            .as_ref()
            .filter(|_| self.current_section() == Some(Page::Appearance))
    }

    pub fn current_section(&self) -> Option<Page> {
        self.layouts()
            .pages
            .get(self.current)
            .map(|(page, _)| *page)
    }

    /// The value a setting has now: the file's, or else the default.
    pub fn value(&self, key: &str) -> Option<Value> {
        self.file
            .get(key)
            .or_else(|| self.item_for(key).map(|item| item.default.clone()))
    }

    /// The value a theme token has now, such as `font.text`.
    pub fn token(&self, name: &str) -> Option<String> {
        self.tokens.text(name).map(str::to_string)
    }

    /// The error from the last write, if it failed: (key, message).
    pub fn last_error(&self) -> Option<(&str, &str)> {
        self.error
            .as_ref()
            .map(|(key, message)| (key.as_str(), message.as_str()))
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    /// The screen's own look.
    pub fn style(&self) -> &SettingsTheme {
        &self.style
    }

    // ---- Pages and rows ----

    pub(super) fn item_for(&self, key: &str) -> Option<&SettingItem> {
        self.items.iter().find(|item| item.key == key)
    }

    /// Pages with at least one row matching the search, in list order.
    pub fn visible_sections(&self) -> Vec<Page> {
        self.layouts().pages.iter().map(|(page, _)| *page).collect()
    }

    /// Every page that matches the search, with its rows.
    fn layouts(&self) -> Rc<Layouts> {
        if let Some(layouts) = self.layouts.borrow().as_ref() {
            return layouts.clone();
        }
        let pages = PAGES
            .iter()
            .map(|spec| (spec.page, Rc::new(self.layout_for(spec.page))))
            .filter(|(_, layout)| !layout.rows.is_empty())
            .collect();
        let layouts = Rc::new(Layouts { pages });
        *self.layouts.borrow_mut() = Some(layouts.clone());
        layouts
    }

    /// Drops the pages built for the old search, file or rules.
    pub(super) fn invalidate_layouts(&self) {
        self.layouts.borrow_mut().take();
    }

    /// Replaces the search, in the box and in the rows it picks.
    pub(super) fn set_query(&mut self, text: &str, cx: &mut Context<Self>) {
        if self.search.read(cx).text() != text {
            self.search.update(cx, |field, cx| field.set_text(text, cx));
        }
        self.query = text.to_string();
        self.invalidate_layouts();
        self.select_section(0, cx);
    }

    pub fn section_title(&self, page: Page) -> String {
        PageSpec::get(page).title.to_string()
    }

    /// The rows the right pane shows for the current page and search.
    pub fn rows(&self) -> Vec<ControlRow> {
        self.layout().rows.clone()
    }

    /// The current page's rows and cards.
    pub fn layout(&self) -> Rc<PaneLayout> {
        self.layouts()
            .pages
            .get(self.current)
            .map(|(_, layout)| layout.clone())
            .unwrap_or_default()
    }

    /// The search a page's rows are filtered by: none when the search
    /// names the page itself, such as "shortcuts", so the whole page shows.
    fn query_for(&self, page: Page) -> &str {
        let query = self.query.trim();
        if !query.is_empty() && words_match(PageSpec::get(page).title, query) {
            ""
        } else {
            query
        }
    }

    fn layout_for(&self, page: Page) -> PaneLayout {
        let mut layout = PaneLayout::default();
        let query = self.query_for(page);
        if page == Page::Shortcuts {
            self.shortcut_cards(query, &mut layout);
            return layout;
        }
        for card in page_cards(page, &self.items) {
            let rows = card
                .iter()
                .flat_map(|spec| self.rows_for_spec(spec, query))
                .collect();
            layout.push_card(None, rows);
        }
        if page == Page::Snippets {
            self.snippet_cards(query, &mut layout);
        }
        layout
    }

    fn shortcut_cards(&self, query: &str, layout: &mut PaneLayout) {
        let query = ShortcutQuery::new(query);
        let mut categories: Vec<&str> = Vec::new();
        for row in &self.shortcuts {
            if !categories.contains(&row.category.as_str()) {
                categories.push(&row.category);
            }
        }
        for category in categories {
            let rows = self
                .shortcuts
                .iter()
                .filter(|row| row.category == category && row.matches_query(&query))
                .cloned()
                .map(ControlRow::Shortcut)
                .collect();
            layout.push_card(Some(category.to_string()), rows);
        }
    }

    fn rows_for_spec(&self, spec: &RowSpec, query: &str) -> Vec<ControlRow> {
        let row = match spec {
            RowSpec::Setting(key) => {
                return self
                    .item_for(key)
                    .filter(|item| item.matches(query))
                    .map(|item| self.rows_for_item(item))
                    .unwrap_or_default();
            }
            RowSpec::Font(slot) => ControlRow::Font(*slot),
            RowSpec::Accent => ControlRow::Accent,
            RowSpec::Vault => ControlRow::Vault,
            RowSpec::Version => ControlRow::Version,
            RowSpec::SyncRemote => ControlRow::SyncRemote,
            RowSpec::SyncAccount if !self.remote_takes_token() => return Vec::new(),
            RowSpec::SyncAccount => ControlRow::SyncAccount,
        };
        let haystack = format!("{} {}", row.title(), self.row_description(&row));
        if words_match(&haystack, query) {
            vec![row]
        } else {
            Vec::new()
        }
    }

    fn rows_for_item(&self, item: &SettingItem) -> Vec<ControlRow> {
        if let SettingKind::List(_) = &item.kind {
            return self.list_rows(item);
        }
        let SettingKind::Map(inner) = &item.kind else {
            return vec![ControlRow::Setting(item.clone())];
        };
        let entries =
            self.file
                .entries(&item.key)
                .into_iter()
                .map(|(name, _)| ControlRow::MapEntry {
                    map: item.clone(),
                    item: SettingItem {
                        key: format!("{}.{name}", item.key),
                        title: map_name_label(&item.key, &name),
                        description: String::new(),
                        kind: (**inner).clone(),
                        default: Value::Null,
                    },
                });
        std::iter::once(ControlRow::MapAdd(item.clone()))
            .chain(entries)
            .collect()
    }

    /// The muted line under a row's title.
    pub(super) fn row_description(&self, row: &ControlRow) -> String {
        match row {
            ControlRow::Setting(item) | ControlRow::MapAdd(item) | ControlRow::ListAdd(item) => {
                item.description.clone()
            }
            ControlRow::MapEntry { .. } | ControlRow::Version | ControlRow::ListEntry { .. } => {
                String::new()
            }
            ControlRow::SyncRemote => self.remote_description(),
            ControlRow::SyncAccount => self.account_description(),
            ControlRow::Font(slot) => slot.description().to_string(),
            ControlRow::Accent => ACCENT_DESCRIPTION.to_string(),
            ControlRow::Vault => self.vault_root.display().to_string(),
            ControlRow::Shortcut(_) => String::new(),
            ControlRow::SnippetsFile => self.snippets_file_description(),
            ControlRow::Snippet(_) | ControlRow::SnippetEditor | ControlRow::Replacement(_) => {
                String::new()
            }
        }
    }

    // ---- Focus ----

    /// Moves focus and gives keyboard input to the right element.
    pub(super) fn set_focus(
        &mut self,
        focus: SettingsFocus,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.commit_number(cx);
        self.menu = None;
        if focus != self.focus {
            self.error = None;
        }
        self.focus = focus;
        match focus {
            SettingsFocus::Search => window.focus(&self.search.focus_handle(cx)),
            SettingsFocus::Control(index) => {
                self.list.scroll_to_reveal_item(self.child_index(index));
                match self
                    .layout()
                    .rows
                    .get(index)
                    .and_then(|row| self.field_for(row))
                {
                    Some(field) => window.focus(&field.focus_handle(cx)),
                    None => window.focus(&self.focus_handle),
                }
            }
            SettingsFocus::Sections => window.focus(&self.focus_handle),
        }
        cx.notify();
    }

    /// The list item that holds row `index`: the page title comes
    /// first, then each card's title (if any) and its rows.
    pub(super) fn child_index(&self, index: usize) -> usize {
        let layout = self.layout();
        let titles = layout
            .cards
            .iter()
            .filter(|card| card.rows.start <= index && card.title.is_some())
            .count();
        1 + titles + index
    }

    pub(super) fn field_for(&self, row: &ControlRow) -> Option<Entity<TextInput>> {
        let key = match row {
            ControlRow::Setting(item) if item.kind == SettingKind::Text => item.key.clone(),
            ControlRow::MapAdd(item) if map_names(&item.key).is_none() => add_field_key(&item.key),
            ControlRow::ListAdd(item) => add_field_key(&item.key),
            ControlRow::SyncRemote if self.sync_remote().is_some() => {
                super::sync_page::REMOTE_FIELD.to_string()
            }
            ControlRow::SnippetEditor => {
                return self.snippet_editor.as_ref().map(|editor| {
                    editor
                        .field(super::snippet_editor::EditorField::Trigger)
                        .clone()
                });
            }
            _ => return None,
        };
        self.fields.get(&key).cloned()
    }

    /// A text field got focus from a click: point the cursor at its row.
    fn focus_field_row(&mut self, key: &str, cx: &mut Context<Self>) {
        let remote = key == super::sync_page::REMOTE_FIELD;
        self.focus_row_where(
            |row| {
                let adds = matches!(row, ControlRow::MapAdd(_) | ControlRow::ListAdd(_));
                (remote && *row == ControlRow::SyncRemote)
                    || row.item().is_some_and(|item| {
                        item.key == key || (add_field_key(&item.key) == key && adds)
                    })
            },
            cx,
        );
    }

    pub(super) fn focus_row_where(
        &mut self,
        wanted: impl Fn(&ControlRow) -> bool,
        cx: &mut Context<Self>,
    ) {
        if let Some(index) = self.layout().rows.iter().position(wanted) {
            self.focus = SettingsFocus::Control(index);
            cx.notify();
        }
    }

    pub(super) fn select_section(&mut self, index: usize, cx: &mut Context<Self>) {
        let count = self.visible_sections().len();
        if count > 0 {
            self.current = index.min(count - 1);
            self.menu = None;
            self.list.scroll_to(ListOffset::default());
            cx.notify();
        }
    }

    fn on_search_event(
        &mut self,
        _: &Entity<TextInput>,
        event: &TextInputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            TextInputEvent::Changed => {
                let text = self.search.read(cx).text().to_string();
                self.set_query(&text, cx);
            }
            TextInputEvent::Submitted => self.focus_first_control(window, cx),
            TextInputEvent::Blurred => {}
            TextInputEvent::Cancelled if !self.query.is_empty() => self.set_query("", cx),
            TextInputEvent::Cancelled => cx.emit(DismissEvent),
        }
    }

    /// Focuses the first row that takes focus, if there is one.
    pub(super) fn focus_first_control(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(index) = self.layout().rows.iter().position(ControlRow::is_focusable) {
            self.set_focus(SettingsFocus::Control(index), window, cx);
        }
    }

    /// Closes the screen, running `command` once it has.
    pub(super) fn request_command(&mut self, command: &str, cx: &mut Context<Self>) {
        cx.emit(SettingsRequest::RunCommand(command.to_string()));
        cx.emit(DismissEvent);
    }
}
