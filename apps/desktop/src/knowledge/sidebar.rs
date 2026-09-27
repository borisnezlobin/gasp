//! The right sidebar: the active note's backlinks (with unlinked
//! mentions), its outgoing links, its outline, and every tag in the vault.
//!
//! It follows the workspace's active note and the vault index, and only
//! works out what the view on screen needs. Edits to the note update the
//! outline and outgoing links a moment after typing stops, parsed off the
//! main thread; the sidebar doesn't redraw on keystrokes.

use std::collections::HashSet;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::time::Duration;

use gpui::{
    App, AppContext, Context, Entity, EntityId, EventEmitter, ListAlignment, ListState,
    SharedString, Subscription, Task, px,
};

use super::index::LinkIndex;
use super::mentions::{Excerpt, Mention, MentionSearch, link_excerpt, note_title};
use super::parse::{Link, parse_note};
use crate::editor::{EditorEvent, EditorView};
use crate::icons::IconName;
use crate::outline::{Heading, headings};
use crate::vault_index::VaultIndex;
use crate::workspace::Workspace;

/// How long typing has to pause before the outline and links catch up.
const EDIT_SETTLE: Duration = Duration::from_millis(250);

/// The most unlinked mentions listed.
const MENTION_LIMIT: usize = 200;

/// The sidebar's views, in the order its header shows them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SidebarView {
    Backlinks,
    Outgoing,
    Outline,
    Tags,
}

impl SidebarView {
    pub const ALL: [SidebarView; 4] = [
        SidebarView::Backlinks,
        SidebarView::Outgoing,
        SidebarView::Outline,
        SidebarView::Tags,
    ];

    /// The name `device.toml` remembers it by.
    pub fn key(self) -> &'static str {
        match self {
            SidebarView::Backlinks => "backlinks",
            SidebarView::Outgoing => "outgoing-links",
            SidebarView::Outline => "outline",
            SidebarView::Tags => "tags",
        }
    }

    pub fn from_key(key: &str) -> Option<SidebarView> {
        SidebarView::ALL.into_iter().find(|view| view.key() == key)
    }

    /// The command that shows it.
    pub fn command(self) -> &'static str {
        match self {
            SidebarView::Backlinks => "sidebar.backlinks",
            SidebarView::Outgoing => "sidebar.outgoing-links",
            SidebarView::Outline => "sidebar.outline",
            SidebarView::Tags => "sidebar.tags",
        }
    }

    pub fn icon(self) -> IconName {
        match self {
            SidebarView::Backlinks => IconName::ArrowSquareIn,
            SidebarView::Outgoing => IconName::ArrowSquareOut,
            SidebarView::Outline => IconName::ListDashes,
            SidebarView::Tags => IconName::Hash,
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            SidebarView::Backlinks => "Backlinks",
            SidebarView::Outgoing => "Outgoing links",
            SidebarView::Outline => "Outline",
            SidebarView::Tags => "Tags",
        }
    }
}

/// What the sidebar asks the workspace to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SidebarEvent {
    /// Open the note, with the cursor at the byte offset if given.
    Open {
        path: PathBuf,
        offset: Option<usize>,
    },
    /// Follow a link from the active note, creating its note if missing.
    Follow(String),
    /// Move the active note's cursor to this byte offset.
    Jump(usize),
    /// Search the vault for this tag, `#` included.
    SearchTag(String),
    /// Make a plain mention in `source` a link.
    LinkMention {
        source: PathBuf,
        range: Range<usize>,
        expected: String,
        link: String,
    },
    /// Show this view, as a header button asks.
    Show(SidebarView),
    Hide,
}

/// One row of the list the sidebar shows.
#[derive(Clone, Debug, PartialEq)]
pub enum Row {
    /// A line that says what the list below is, with its count.
    Summary(SharedString),
    /// Nothing to list, and why.
    Message(SharedString),
    /// A note that links here, heading its excerpts.
    Source {
        path: PathBuf,
        title: SharedString,
        folder: SharedString,
    },
    /// Where a link or mention is, with the line around it.
    Context {
        path: PathBuf,
        offset: usize,
        excerpt: Excerpt,
        /// For a plain mention: the words and the link that replaces them.
        mention: Option<(Range<usize>, String, String)>,
    },
    /// The button that shows or hides the unlinked mentions.
    UnlinkedToggle { open: bool, count: Option<usize> },
    /// A link out of the note.
    Outgoing {
        label: SharedString,
        target: String,
        detail: Option<SharedString>,
        exists: bool,
        is_note: bool,
    },
    Heading {
        title: SharedString,
        offset: usize,
        depth: usize,
        current: bool,
    },
    Tag {
        /// The full tag, such as `physics/waves`.
        name: String,
        /// What shows: the last part of a nested tag.
        label: SharedString,
        depth: usize,
        notes: usize,
        children: bool,
        collapsed: bool,
    },
}

/// The note the workspace shows, which the sidebar is about.
struct ActiveNote {
    path: PathBuf,
    editor: Entity<EditorView>,
    cursor: usize,
}

/// What the unlinked mentions search has found.
enum Unlinked {
    Idle,
    Searching,
    Found(Vec<Mention>),
}

/// The note's own text, parsed: headings and links.
#[derive(Default)]
struct LiveNote {
    headings: Vec<Heading>,
    links: Vec<Link>,
}

pub struct KnowledgeSidebar {
    vault: PathBuf,
    index: Entity<VaultIndex>,
    view: SidebarView,
    active: Option<ActiveNote>,
    live: LiveNote,
    rows: Vec<Row>,
    pub(super) list: ListState,
    show_unlinked: bool,
    unlinked: Unlinked,
    collapsed_tags: HashSet<String>,
    editor_events: Option<Subscription>,
    parse_task: Option<Task<()>>,
    mention_task: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<SidebarEvent> for KnowledgeSidebar {}

impl KnowledgeSidebar {
    pub fn new(
        vault: &Path,
        index: Entity<VaultIndex>,
        view: SidebarView,
        cx: &mut Context<Self>,
    ) -> Self {
        let observe_index = cx.observe(&index, |sidebar, _, cx| sidebar.index_changed(cx));
        KnowledgeSidebar {
            vault: vault.to_path_buf(),
            index,
            view,
            active: None,
            live: LiveNote::default(),
            rows: Vec::new(),
            list: ListState::new(0, ListAlignment::Top, px(200.)),
            show_unlinked: false,
            unlinked: Unlinked::Idle,
            collapsed_tags: HashSet::new(),
            editor_events: None,
            parse_task: None,
            mention_task: None,
            _subscriptions: vec![observe_index],
        }
    }

    /// Follows the workspace's active note from now on.
    pub fn follow_workspace(&mut self, workspace: &Entity<Workspace>, cx: &mut Context<Self>) {
        let observe = cx.observe(workspace, |sidebar, workspace, cx| {
            sidebar.sync_active(&workspace, cx)
        });
        self._subscriptions.push(observe);
        // The workspace may be mid-update, as while it's being built.
        let workspace = workspace.downgrade();
        cx.spawn(async move |sidebar, cx| {
            let Some(workspace) = workspace.upgrade() else {
                return;
            };
            sidebar
                .update(cx, |sidebar, cx| sidebar.sync_active(&workspace, cx))
                .ok();
        })
        .detach();
    }

    pub fn view(&self) -> SidebarView {
        self.view
    }

    pub fn rows(&self) -> &[Row] {
        &self.rows
    }

    pub fn index(&self) -> &Entity<VaultIndex> {
        &self.index
    }

    pub fn active_path(&self) -> Option<&Path> {
        self.active.as_ref().map(|active| active.path.as_path())
    }

    pub fn shows_unlinked(&self) -> bool {
        self.show_unlinked
    }

    /// Shows `view`, working out its rows.
    pub fn set_view(&mut self, view: SidebarView, cx: &mut Context<Self>) {
        if self.view == view {
            return;
        }
        self.view = view;
        self.reparse_now(cx);
        self.rebuild(true, cx);
    }

    /// Shows or hides the unlinked mentions, searching for them if needed.
    pub fn toggle_unlinked(&mut self, cx: &mut Context<Self>) {
        self.show_unlinked = !self.show_unlinked;
        if self.show_unlinked {
            self.search_mentions(cx);
        }
        self.rebuild(false, cx);
    }

    /// Collapses or expands a tag's nested tags.
    pub fn toggle_tag(&mut self, name: &str, cx: &mut Context<Self>) {
        if !self.collapsed_tags.remove(name) {
            self.collapsed_tags.insert(name.to_string());
        }
        self.rebuild(false, cx);
    }

    // ---- Following the workspace ----

    fn sync_active(&mut self, workspace: &Entity<Workspace>, cx: &mut Context<Self>) {
        let (path, editor) = {
            let workspace = workspace.read(cx);
            (workspace.active_path(cx), workspace.active_editor(cx))
        };
        let same_note = match (&self.active, &path, &editor) {
            (Some(active), Some(path), Some(editor)) => {
                active.path == *path && active.editor.entity_id() == editor.entity_id()
            }
            (None, None, _) | (None, _, None) => true,
            _ => false,
        };
        if same_note {
            self.cursor_moved(cx);
            return;
        }
        let active = path.zip(editor).map(|(path, editor)| {
            let cursor = editor.read(cx).cursor();
            ActiveNote {
                path,
                editor,
                cursor,
            }
        });
        self.set_active(active, cx);
    }

    fn set_active(&mut self, active: Option<ActiveNote>, cx: &mut Context<Self>) {
        self.editor_events = active.as_ref().map(|active| {
            cx.subscribe(&active.editor, |sidebar, _, event: &EditorEvent, cx| {
                if *event == EditorEvent::Edited {
                    sidebar.schedule_reparse(cx);
                }
            })
        });
        self.active = active;
        self.live = LiveNote::default();
        self.unlinked = Unlinked::Idle;
        if self.show_unlinked {
            self.search_mentions(cx);
        }
        self.reparse_now(cx);
        self.rebuild(true, cx);
    }

    /// Marks the heading the cursor is under, when the outline shows.
    fn cursor_moved(&mut self, cx: &mut Context<Self>) {
        let Some(active) = &mut self.active else {
            return;
        };
        let cursor = active.editor.read(cx).cursor();
        if cursor == active.cursor {
            return;
        }
        let before = current_heading(&self.live.headings, active.cursor);
        active.cursor = cursor;
        if self.view == SidebarView::Outline
            && current_heading(&self.live.headings, cursor) != before
        {
            self.rebuild(false, cx);
        }
    }

    fn index_changed(&mut self, cx: &mut Context<Self>) {
        if self.show_unlinked && self.view == SidebarView::Backlinks {
            self.search_mentions(cx);
        }
        if self.view != SidebarView::Outline {
            self.rebuild(false, cx);
        }
    }

    // ---- The note's own text ----

    /// Whether the view on screen reads the note's text itself.
    fn needs_live_text(&self) -> bool {
        matches!(self.view, SidebarView::Outline | SidebarView::Outgoing)
    }

    fn schedule_reparse(&mut self, cx: &mut Context<Self>) {
        if !self.needs_live_text() {
            return;
        }
        let Some(editor) = self.active.as_ref().map(|active| active.editor.clone()) else {
            return;
        };
        self.parse_task = Some(cx.spawn(async move |sidebar, cx| {
            cx.background_executor().timer(EDIT_SETTLE).await;
            let Ok(text) = editor.read_with(cx, |editor, _| editor.text()) else {
                return;
            };
            let live = cx.background_spawn(async move { parse_live(&text) }).await;
            sidebar
                .update(cx, |sidebar, cx| {
                    sidebar.live = live;
                    sidebar.rebuild(false, cx);
                })
                .ok();
        }));
    }

    /// Parses the note's text now, for a note or view that just appeared.
    fn reparse_now(&mut self, cx: &mut Context<Self>) {
        self.parse_task = None;
        self.live = match (&self.active, self.needs_live_text()) {
            (Some(active), true) => parse_live(&active.editor.read(cx).text()),
            _ => LiveNote::default(),
        };
    }

    // ---- Unlinked mentions ----

    fn search_mentions(&mut self, cx: &mut Context<Self>) {
        let Some(target) = self.active_relative() else {
            self.unlinked = Unlinked::Found(Vec::new());
            return;
        };
        let index = self.index.read(cx);
        if !index.is_ready() {
            self.unlinked = Unlinked::Searching;
            return;
        }
        let search = MentionSearch::new(index.links(), &target);
        if matches!(self.unlinked, Unlinked::Idle) {
            self.unlinked = Unlinked::Searching;
        }
        self.mention_task = Some(cx.spawn(async move |sidebar, cx| {
            let found = cx
                .background_spawn(async move { search.run(MENTION_LIMIT) })
                .await;
            sidebar
                .update(cx, |sidebar, cx| {
                    sidebar.unlinked = Unlinked::Found(found);
                    sidebar.rebuild(false, cx);
                })
                .ok();
        }));
    }

    fn active_relative(&self) -> Option<String> {
        let active = self.active.as_ref()?;
        super::build::relative(&self.vault, &active.path)
    }

    // ---- Rows ----

    /// Works out the rows again. `from_top` scrolls back up, for a new
    /// note or view; otherwise the list keeps its place.
    fn rebuild(&mut self, from_top: bool, cx: &mut Context<Self>) {
        let rows = self.build_rows(cx);
        if rows == self.rows && !from_top {
            return;
        }
        if from_top {
            self.list.reset(rows.len());
        } else {
            self.list.splice(0..self.rows.len(), rows.len());
        }
        self.rows = rows;
        cx.notify();
    }

    fn build_rows(&self, cx: &App) -> Vec<Row> {
        let index = self.index.read(cx);
        let relative = self.active_relative();
        let needs_note = self.view != SidebarView::Tags;
        if needs_note && relative.is_none() {
            return vec![Row::Message(no_note_message(self.view).into())];
        }
        let needs_index = matches!(self.view, SidebarView::Backlinks | SidebarView::Tags);
        if needs_index && !index.is_ready() {
            return vec![Row::Message("Reading the vault’s links…".into())];
        }
        let relative = relative.unwrap_or_default();
        match self.view {
            SidebarView::Backlinks => self.backlink_rows(index.links(), &relative),
            SidebarView::Outgoing => outgoing_rows(index.links(), &relative, &self.live.links),
            SidebarView::Outline => self.outline_rows(),
            SidebarView::Tags => tag_rows(index.links(), &self.collapsed_tags),
        }
    }

    fn backlink_rows(&self, index: &LinkIndex, target: &str) -> Vec<Row> {
        let mut rows = backlink_rows(&self.vault, index, target);
        let (count, mentions) = match &self.unlinked {
            Unlinked::Found(found) => (Some(found.len()), found.as_slice()),
            _ => (None, &[][..]),
        };
        rows.push(Row::UnlinkedToggle {
            open: self.show_unlinked,
            // None found says so below; a 0 beside it would repeat it.
            count: count.filter(|count| self.show_unlinked && *count > 0),
        });
        if self.show_unlinked {
            rows.extend(mention_rows(&self.vault, index, target, mentions, count));
        }
        rows
    }

    fn outline_rows(&self) -> Vec<Row> {
        let headings = &self.live.headings;
        if headings.is_empty() {
            return vec![Row::Message("This note has no headings yet.".into())];
        }
        let cursor = self.active.as_ref().map_or(0, |active| active.cursor);
        let current = current_heading(headings, cursor);
        headings
            .iter()
            .zip(nesting(headings))
            .enumerate()
            .map(|(at, (heading, depth))| Row::Heading {
                title: heading.title.clone().into(),
                offset: heading.offset,
                depth,
                current: current == Some(at),
            })
            .collect()
    }
}

fn parse_live(text: &str) -> LiveNote {
    LiveNote {
        headings: headings(text),
        links: parse_note(text).links,
    }
}

/// How deep each heading sits under the ones before it, so an `###`
/// straight after an `#` indents one step, not two.
fn nesting(headings: &[Heading]) -> Vec<usize> {
    let mut open: Vec<u8> = Vec::new();
    headings
        .iter()
        .map(|heading| {
            while open.last().is_some_and(|level| *level >= heading.level) {
                open.pop();
            }
            open.push(heading.level);
            open.len() - 1
        })
        .collect()
}

/// The heading whose section holds the byte offset `cursor`.
fn current_heading(headings: &[Heading], cursor: usize) -> Option<usize> {
    headings
        .iter()
        .rposition(|heading| heading.offset <= cursor)
}

fn no_note_message(view: SidebarView) -> &'static str {
    match view {
        SidebarView::Backlinks => "Open a note to see the notes that link to it.",
        SidebarView::Outgoing => "Open a note to see where its links go.",
        SidebarView::Outline => "Open a note to see its headings.",
        SidebarView::Tags => "",
    }
}

/// "1 note links here", "3 notes link here".
fn count_phrase(count: usize, one: &str, many: &str) -> String {
    if count == 1 {
        format!("1 {one}")
    } else {
        format!("{count} {many}")
    }
}

fn source_row(vault: &Path, source: &str) -> Row {
    let folder = super::index_folder(source);
    Row::Source {
        path: vault.join(source),
        title: note_title(source).to_string().into(),
        folder: folder.into(),
    }
}

/// The notes linking to `target`, each with the lines its links are on.
pub fn backlink_rows(vault: &Path, index: &LinkIndex, target: &str) -> Vec<Row> {
    let mut backlinks = index.backlinks(target);
    backlinks.retain(|backlink| backlink.source != target);
    backlinks.sort_by_key(|backlink| note_title(backlink.source).to_lowercase());
    if backlinks.is_empty() {
        return vec![Row::Summary("No notes link here yet.".into())];
    }
    let mut rows = vec![Row::Summary(
        count_phrase(backlinks.len(), "note links here", "notes link here").into(),
    )];
    for backlink in backlinks {
        let Some(entry) = index.note(backlink.source) else {
            continue;
        };
        rows.push(source_row(vault, backlink.source));
        rows.extend(backlink.links.iter().map(|link| Row::Context {
            path: vault.join(backlink.source),
            offset: link.range.start,
            excerpt: link_excerpt(&entry.text, link),
            mention: None,
        }));
    }
    rows
}

/// The unlinked mentions found, grouped by note, each with a Link button.
fn mention_rows(
    vault: &Path,
    index: &LinkIndex,
    target: &str,
    mentions: &[Mention],
    count: Option<usize>,
) -> Vec<Row> {
    let Some(count) = count else {
        return vec![Row::Message("Looking for mentions…".into())];
    };
    if count == 0 {
        return vec![Row::Message(
            format!(
                "No other note mentions “{}” without linking it.",
                note_title(target)
            )
            .into(),
        )];
    }
    let linkpath = super::mentions::linkpath_for(index, target);
    let mut rows = Vec::new();
    let mut last: Option<&str> = None;
    for mention in mentions {
        if last != Some(mention.source.as_str()) {
            rows.push(source_row(vault, &mention.source));
            last = Some(&mention.source);
        }
        let words = &mention.excerpt.text[mention.excerpt.highlight.clone()];
        rows.push(Row::Context {
            path: vault.join(&mention.source),
            offset: mention.range.start,
            excerpt: mention.excerpt.clone(),
            mention: Some((
                mention.range.clone(),
                words.to_string(),
                super::mentions::wikilink_for(&linkpath, words),
            )),
        });
    }
    rows
}

/// The note's links, one row per place they go, in the order written.
pub fn outgoing_rows(index: &LinkIndex, source: &str, links: &[Link]) -> Vec<Row> {
    let mut seen = HashSet::new();
    let mut rows = Vec::new();
    let mut missing = 0;
    for link in links {
        let resolved = index.resolve(source, link);
        let key = resolved
            .clone()
            .unwrap_or_else(|| link.target.to_lowercase());
        if !seen.insert((key, link.subpath.clone())) {
            continue;
        }
        let is_note = resolved
            .as_deref()
            .map_or(!link.target.contains('.'), super::index::is_note_path);
        missing += usize::from(resolved.is_none() && is_note);
        rows.push(Row::Outgoing {
            label: resolved
                .as_deref()
                .map_or(link.target.as_str(), note_title)
                .to_string()
                .into(),
            target: follow_target(link),
            detail: link.subpath.clone().map(SharedString::from),
            exists: resolved.is_some(),
            is_note,
        });
    }
    if rows.is_empty() {
        return vec![Row::Message("This note doesn’t link anywhere yet.".into())];
    }
    // Notes first, then notes still to write, then images and other files.
    rows.sort_by_key(|row| match row {
        Row::Outgoing {
            exists, is_note, ..
        } => match (exists, is_note) {
            (true, true) => 0,
            (false, _) => 1,
            (true, false) => 2,
        },
        _ => 3,
    });
    let mut summary = count_phrase(rows.len(), "link goes out", "links go out");
    match missing {
        0 => {}
        1 => summary.push_str(", 1 to a note that doesn’t exist yet"),
        _ => summary.push_str(&format!(", {missing} to notes that don’t exist yet")),
    }
    rows.insert(0, Row::Summary(summary.into()));
    rows
}

/// What following a link opens: its target and heading, as `link.follow`
/// takes them.
fn follow_target(link: &Link) -> String {
    let target = link.target.strip_suffix(".md").unwrap_or(&link.target);
    match &link.subpath {
        Some(subpath) => format!("{target}#{subpath}"),
        None => target.to_string(),
    }
}

/// Every tag as a tree, with nested tags under their parents.
pub fn tag_rows(index: &LinkIndex, collapsed: &HashSet<String>) -> Vec<Row> {
    let tags = index.tags();
    if tags.is_empty() {
        return vec![Row::Message(
            "No notes have tags yet. Write #tag in a note to add one.".into(),
        )];
    }
    let mut rows = vec![Row::Summary(
        count_phrase(
            tags.iter().filter(|tag| !tag.name.contains('/')).count(),
            "tag",
            "tags",
        )
        .into(),
    )];
    let mut hidden_under: Option<String> = None;
    for (at, tag) in tags.iter().enumerate() {
        let lower = tag.name.to_lowercase();
        if let Some(parent) = &hidden_under {
            if lower.starts_with(parent.as_str()) {
                continue;
            }
            hidden_under = None;
        }
        let children = tags
            .get(at + 1)
            .is_some_and(|next| next.name.to_lowercase().starts_with(&format!("{lower}/")));
        let is_collapsed = children && collapsed.contains(&tag.name);
        if is_collapsed {
            hidden_under = Some(format!("{lower}/"));
        }
        rows.push(Row::Tag {
            label: tag
                .name
                .rsplit('/')
                .next()
                .unwrap_or(&tag.name)
                .to_string()
                .into(),
            depth: tag.name.matches('/').count(),
            notes: tag.notes,
            children,
            collapsed: is_collapsed,
            name: tag.name.clone(),
        });
    }
    rows
}

/// Rows the tests look for: what kind each is, with its main text.
pub fn describe(rows: &[Row]) -> Vec<String> {
    rows.iter()
        .map(|row| match row {
            Row::Summary(text) => format!("summary: {text}"),
            Row::Message(text) => format!("message: {text}"),
            Row::Source { title, .. } => format!("note: {title}"),
            Row::Context {
                excerpt, mention, ..
            } => {
                let kind = if mention.is_some() { "mention" } else { "link" };
                format!("{kind}: {}", excerpt.text)
            }
            Row::UnlinkedToggle { open, count } => {
                format!("unlinked: {open} {count:?}")
            }
            Row::Outgoing { label, exists, .. } => format!("out: {label} {exists}"),
            Row::Heading {
                title,
                depth,
                current,
                ..
            } => format!("heading: {depth} {title} {current}"),
            Row::Tag {
                name,
                notes,
                collapsed,
                ..
            } => format!("tag: {name} {notes} {collapsed}"),
        })
        .collect()
}

/// For tests: the entity id of the editor the sidebar follows.
pub fn followed_editor(sidebar: &KnowledgeSidebar) -> Option<EntityId> {
    sidebar
        .active
        .as_ref()
        .map(|active| active.editor.entity_id())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index(notes: &[(&str, &str)]) -> LinkIndex {
        let mut index = LinkIndex::new();
        for (path, text) in notes {
            index.set_note(path, *text);
        }
        index
    }

    #[test]
    fn backlinks_group_links_under_their_notes() {
        let index = index(&[
            ("Target.md", ""),
            ("b/Second.md", "Also [[Target|the target]] here."),
            (
                "First.md",
                "- See [[Target]].\n\nAnd [[Target#Part]] again.",
            ),
        ]);
        let rows = backlink_rows(Path::new("/v"), &index, "Target.md");
        assert_eq!(
            describe(&rows),
            [
                "summary: 2 notes link here",
                "note: First",
                "link: See Target.",
                "link: And Target again.",
                "note: Second",
                "link: Also the target here.",
            ]
        );
    }

    #[test]
    fn no_backlinks_says_so() {
        let index = index(&[("Target.md", "[[Target]]")]);
        let rows = backlink_rows(Path::new("/v"), &index, "Target.md");
        assert_eq!(describe(&rows), ["summary: No notes link here yet."]);
    }

    #[test]
    fn outgoing_links_count_missing_notes_once() {
        let index = index(&[("A.md", ""), ("B.md", "")]);
        let links = parse_note("[[B]] [[Missing]] [[B]] [[B#Part]] ![[pic.png]]").links;
        let rows = outgoing_rows(&index, "A.md", &links);
        assert_eq!(
            describe(&rows),
            [
                "summary: 4 links go out, 1 to a note that doesn’t exist yet",
                "out: B true",
                "out: B true",
                "out: Missing false",
                "out: pic.png false",
            ]
        );
    }

    #[test]
    fn headings_nest_by_what_comes_before() {
        let found = headings("# A\n### B\n## C\n#### D\n# E\n");
        assert_eq!(nesting(&found), [0, 1, 1, 2, 0]);
    }

    #[test]
    fn tags_nest_and_collapse() {
        let index = index(&[
            ("A.md", "#physics/waves #physics/optics #maths"),
            ("B.md", "#physics"),
        ]);
        let all = tag_rows(&index, &HashSet::new());
        assert_eq!(
            describe(&all),
            [
                "summary: 2 tags",
                "tag: maths 1 false",
                "tag: physics 2 false",
                "tag: physics/optics 1 false",
                "tag: physics/waves 1 false",
            ]
        );
        let collapsed = tag_rows(&index, &HashSet::from(["physics".to_string()]));
        assert_eq!(collapsed.len(), 3);
        assert_eq!(describe(&collapsed)[2], "tag: physics 2 true");
    }
}
