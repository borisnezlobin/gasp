//! Tools on the vault's config in `.gasp/`: settings, theme tokens,
//! commands and their keys, rules, snippets and replacements.
//!
//! Every write goes through the same writers the settings screen uses,
//! so comments and layout in the files survive, and every write is
//! checked first: a value the app wouldn't load is refused with the
//! reason, and nothing is written.

use std::path::Path;

use gasp_config::commands::BUILTIN_COMMANDS;
use gasp_config::config_files::{
    REPLACEMENTS_FILE, RULES_FILE, SNIPPETS_FILE, default_number, default_token, known_commands,
    load_rules, load_tokens, replacements_path, rules_path, snippets_path, write_theme_number,
    write_theme_token,
};
use gasp_config::loader::{build_rules, build_settings};
use gasp_config::schema::{SettingDescriptor, SettingKind, setting_descriptors};
use gasp_config::settings::SettingsIndex;
use gasp_config::store::{SETTINGS_FILE, SettingsFile, save, settings_path, write_setting};
use gasp_config::toolbar_files::{TOOLBARS_FILE, item_names, load_toolbars, toolbars_path};
use gasp_config::toolbars::{
    Behaviour, ButtonStyle, Density, MENU_PREFIX, Place, SEPARATOR, SPACER, Surface, Toolbar,
    ToolbarContext, Widget, build_toolbars, choice_name,
};
use gasp_config::{CONFIG_DIR, Diagnostic, Platform, config_dir};
use gasp_snippets::{
    DEFAULT_REPLACEMENTS, DEFAULT_SNIPPETS, Replacements, SnippetEngine, SnippetFile,
};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Map, Value, json};

use crate::context::Context;
use crate::tool::{NoArguments, Output, ToolError, ToolResult, ToolSpec};

pub fn tools() -> Vec<ToolSpec> {
    let mut tools = vec![
        ToolSpec::reads(
            "get_settings",
            concat!(
                "Read settings: each one's dotted key, kind, value in effect, default, whether \
                 the vault's ",
                config_dir!(),
                "/settings.toml sets it, and what it does. Give `key` for one setting or a \
                 group, such as `files` or `editor.smart-quotes`."
            ),
            get_settings,
        ),
        ToolSpec::writes(
            "set_setting",
            concat!(
                "Change one setting by dotted key, such as `files.trash` or `mcp.enabled`; null \
                 puts it back to its default. The value is checked and written to ",
                config_dir!(),
                "/settings.toml the way the settings screen writes it, keeping comments. \
                 The app picks the change up at once."
            ),
            set_setting,
        ),
        ToolSpec::reads(
            "get_theme",
            "Read the theme's tokens (colours, fonts, sizes, spacing) as resolved, for light \
             or dark mode. Give `prefix`, such as `color` or `font`, for some of them.",
            get_theme,
        ),
        ToolSpec::writes(
            "set_theme_token",
            concat!(
                "Change one theme token in ",
                config_dir!(),
                "/theme.toml, such as `color.accent` = \"#2f5fd0\" or \
                 `font.line-height.body` = 1.6; `dark.` before a colour's name sets it for \
                 dark mode. A value may reference another token as \"{color.black}\". null \
                 puts the built-in value back."
            ),
            set_theme_token,
        ),
        ToolSpec::reads(
            "list_commands",
            "List the app's commands: id (for run_command and rules), title, category, \
             whether the palette shows it, and its keys on this computer.",
            list_commands,
        ),
    ];
    tools.extend(file_tools());
    tools
}

/// The tools that read and write a whole config file.
fn file_tools() -> Vec<ToolSpec> {
    vec![
        ToolSpec::reads(
            "get_rules",
            concat!(
                "Read ",
                config_dir!(),
                "/rules.toml, the vault's rules (keys, pointer and typing events) layered \
                 over the built-in ones, and the key rules in effect."
            ),
            get_rules,
        ),
        ToolSpec::writes(
            "set_rules",
            concat!(
                "Replace ",
                config_dir!(),
                "/rules.toml with `text`. It's checked first; a file that doesn't load is \
                 refused with the reason. Unknown commands come back as warnings."
            ),
            set_rules,
        ),
        ToolSpec::reads(
            "get_toolbars",
            concat!(
                "Read ",
                config_dir!(),
                "/toolbars.toml and the toolbars in effect: each one's id, title, place, \
                 behaviour, style and items, the menus they can open, and every item a \
                 toolbar can hold besides commands (widgets, separator, spacer, menu:<id>)."
            ),
            get_toolbars,
        ),
        ToolSpec::writes(
            "set_toolbars",
            concat!(
                "Replace ",
                config_dir!(),
                "/toolbars.toml with `text`. A [toolbar.<id>] there changes only the fields \
                 it names on a built-in toolbar (status, selection, keyboard) and adds any new \
                 id; `enabled = false` turns one off. It's checked first; a file that doesn't \
                 load is refused with the reason, and unknown commands come back as warnings. \
                 The app picks the change up at once."
            ),
            set_toolbars,
        ),
        ToolSpec::reads(
            "get_snippets",
            concat!(
                "Read the math snippets file (",
                config_dir!(),
                "/snippets.txt, else the built-in one), one snippet per line as \
                 `trigger → expansion  options`."
            ),
            get_snippets,
        ),
        ToolSpec::writes(
            "set_snippets",
            concat!(
                "Replace ",
                config_dir!(),
                "/snippets.txt with `text`. Every snippet is parsed and compiled first; any \
                 error is refused with its line."
            ),
            set_snippets,
        ),
        ToolSpec::reads(
            "get_replacements",
            concat!(
                "Read the typing replacements table (",
                config_dir!(),
                "/replacements.toml, else the built-in one), such as -> becoming →."
            ),
            get_replacements,
        ),
        ToolSpec::writes(
            "set_replacements",
            concat!(
                "Replace ",
                config_dir!(),
                "/replacements.toml with `text`, checked first."
            ),
            set_replacements,
        ),
    ]
}

// ---- Settings ----

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct GetSettings {
    /// A dotted key or group, such as `files` or `files.trash`; every setting if left out.
    #[serde(default)]
    key: Option<String>,
}

fn get_settings(context: &Context, args: GetSettings) -> ToolResult {
    let wanted = args.key.as_deref().map(str::trim).unwrap_or_default();
    let descriptors: Vec<SettingDescriptor> = setting_descriptors()
        .into_iter()
        .filter(|descriptor| in_group(&descriptor.key, wanted))
        .collect();
    if descriptors.is_empty() {
        return Err(ToolError::new(format!(
            "there's no setting or group {wanted:?}; call get_settings without a key to list them"
        )));
    }
    let index = SettingsIndex::new(&context.settings());
    let file = SettingsFile::load(&settings_path(context.root())).unwrap_or_default();
    let settings: Vec<Value> = descriptors
        .iter()
        .map(|descriptor| setting_json(descriptor, &index, &file))
        .collect();
    let mut result = json!({ "settings": settings });
    if let Some(problem) = settings_problem(context.root()) {
        result["file_error"] = Value::from(problem);
    }
    Ok(Output::Json(result))
}

/// Whether `key` is `group` or inside it; an empty group holds everything.
fn in_group(key: &str, group: &str) -> bool {
    group.is_empty()
        || key == group
        || key
            .strip_prefix(group)
            .is_some_and(|rest| rest.starts_with('.'))
}

fn setting_json(
    descriptor: &SettingDescriptor,
    index: &SettingsIndex,
    file: &SettingsFile,
) -> Value {
    let key = descriptor.key.as_str();
    let (value, in_file) = match descriptor.kind {
        SettingKind::Map(_) => {
            let entries: Map<String, Value> = file.entries(key).into_iter().collect();
            (Value::Object(entries.clone()), !entries.is_empty())
        }
        _ => {
            let value = index
                .get(key)
                .and_then(|value| serde_json::to_value(value).ok());
            (value.unwrap_or(Value::Null), file.get(key).is_some())
        }
    };
    json!({
        "key": key,
        "kind": kind_name(&descriptor.kind),
        "value": value,
        "default": descriptor.default,
        "set_in_file": in_file,
        "description": descriptor.description,
    })
}

/// A setting's kind in words, with the choices when it has them.
fn kind_name(kind: &SettingKind) -> Value {
    match kind {
        SettingKind::Bool => "bool".into(),
        SettingKind::Integer => "integer".into(),
        SettingKind::Number => "number".into(),
        SettingKind::Text => "text".into(),
        SettingKind::Choice(choices) => json!({ "one_of": choices }),
        SettingKind::Map(values) => json!({ "map_of": kind_name(values) }),
        SettingKind::List(items) => json!({ "list_of": kind_name(items) }),
    }
}

/// Why the vault's settings file doesn't load, if it doesn't.
fn settings_problem(root: &Path) -> Option<String> {
    let text = std::fs::read_to_string(settings_path(root)).ok()?;
    let errors = build_settings(SETTINGS_FILE, Some(&text)).err()?;
    Some(describe(&errors))
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SetSetting {
    /// The dotted key, such as `files.trash`; for a map setting, the map's key, a dot and
    /// the entry's name, such as `markdown.symbols.overrides.link-url`.
    key: String,
    /// The new value, of the setting's kind; null puts the default back.
    value: Value,
}

fn set_setting(context: &Context, args: SetSetting) -> ToolResult {
    let key = args.key.trim();
    let default = setting_default(key)?;
    let value = Some(&args.value).filter(|value| !value.is_null());
    write_setting(context.root(), key, value, &default)
        .map_err(|reason| ToolError::new(format!("{key} wasn't changed: {reason}")))?;
    let now = SettingsIndex::new(&context.settings())
        .get(key)
        .and_then(|value| serde_json::to_value(value).ok());
    Ok(Output::Json(
        json!({ "key": key, "value": now.unwrap_or(args.value) }),
    ))
}

/// The default of the setting `key` names: a leaf, or an entry in a map
/// setting, whose default is to be absent.
fn setting_default(key: &str) -> Result<Value, ToolError> {
    let descriptors = setting_descriptors();
    if let Some(found) = descriptors.iter().find(|descriptor| descriptor.key == key) {
        if matches!(found.kind, SettingKind::Map(_)) {
            return Err(ToolError::new(format!(
                "{key} is a map; set one entry, as {key}.<name>"
            )));
        }
        return Ok(found.default.clone());
    }
    let in_map = descriptors.iter().any(|descriptor| {
        matches!(descriptor.kind, SettingKind::Map(_)) && in_group(key, &descriptor.key)
    });
    match in_map {
        true => Ok(Value::Null),
        false => Err(ToolError::new(format!(
            "there's no setting {key:?}; get_settings lists them"
        ))),
    }
}

// ---- Theme ----

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct GetTheme {
    /// Only tokens under this, such as `color` or `font.scale`.
    #[serde(default)]
    prefix: Option<String>,
    /// The dark mode tokens instead of the light ones.
    #[serde(default)]
    dark: bool,
}

fn get_theme(context: &Context, args: GetTheme) -> ToolResult {
    let tokens = load_tokens(context.root());
    let theme = tokens.for_mode(args.dark);
    let prefix = args.prefix.as_deref().map(str::trim).unwrap_or_default();
    let values: Map<String, Value> = theme
        .names()
        .filter(|name| in_group(name, prefix))
        .filter_map(|name| Some((name.to_string(), token_json(theme.get(name)?))))
        .collect();
    if values.is_empty() {
        return Err(ToolError::new(format!(
            "there are no tokens under {prefix:?}"
        )));
    }
    Ok(Output::Json(json!({
        "mode": if args.dark { "dark" } else { "light" },
        "tokens": values,
    })))
}

fn token_json(value: &gasp_config::theme::TokenValue) -> Value {
    use gasp_config::theme::TokenValue;
    match value {
        TokenValue::Text(text) => Value::from(text.as_str()),
        TokenValue::Integer(number) => Value::from(*number),
        TokenValue::Float(number) => Value::from(*number),
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SetThemeToken {
    /// The token, such as `color.accent`, `dark.color.accent` or `font.text`.
    token: String,
    /// A string (a colour, font or `{reference}`), a number, or null for the built-in value.
    value: Value,
}

fn set_theme_token(context: &Context, args: SetThemeToken) -> ToolResult {
    let token = args.token.trim();
    let base = token.strip_prefix("dark.").unwrap_or(token);
    if default_token(base).is_none() && default_number(base).is_none() {
        return Err(ToolError::new(format!(
            "there's no theme token {token:?}; get_theme lists them"
        )));
    }
    let written = match &args.value {
        Value::Null => write_theme_token(context.root(), token, None),
        Value::String(text) => write_theme_token(context.root(), token, Some(text)),
        Value::Number(number) => write_theme_number(context.root(), token, number.as_f64()),
        _ => return Err(ToolError::new("`value` must be a string, a number or null")),
    };
    let tokens =
        written.map_err(|reason| ToolError::new(format!("{token} wasn't changed: {reason}")))?;
    let dark = token.starts_with("dark.");
    let now = tokens
        .for_mode(dark)
        .get(base)
        .map_or(Value::Null, token_json);
    Ok(Output::Json(json!({ "token": token, "value": now })))
}

// ---- Commands and rules ----

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ListCommands {
    /// Only commands whose id, title or category contain this, ignoring case.
    #[serde(default)]
    query: Option<String>,
}

fn list_commands(context: &Context, args: ListCommands) -> ToolResult {
    let rules = load_rules(context.root());
    let platform = Platform::current();
    let query = args.query.unwrap_or_default().to_lowercase();
    let commands: Vec<Value> = BUILTIN_COMMANDS
        .iter()
        .filter(|spec| {
            [spec.id, spec.title, spec.category]
                .iter()
                .any(|field| field.to_lowercase().contains(&query))
        })
        .map(|spec| {
            let keys: Vec<String> = rules
                .keys_for(spec.id, platform)
                .into_iter()
                .map(|chord| chord.display_for(platform))
                .collect();
            json!({
                "id": spec.id,
                "title": spec.title,
                "category": spec.category,
                "in_palette": spec.palette,
                "keys": keys,
            })
        })
        .collect();
    Ok(Output::Json(json!({ "commands": commands })))
}

fn get_rules(context: &Context, _: NoArguments) -> ToolResult {
    let text = read_config(&rules_path(context.root()))?;
    let platform = Platform::current();
    let rules = load_rules(context.root());
    let keys: Vec<Value> = rules
        .rules()
        .iter()
        .filter(|rule| rule.is_key() && rule.applies_on(platform))
        .filter_map(|rule| {
            let chord = rule.chord_for(platform)?;
            Some(json!({
                "id": rule.id,
                "keys": chord.display_for(platform),
                "command": rule.command,
            }))
        })
        .collect();
    Ok(Output::Json(json!({
        "path": vault_relative(RULES_FILE),
        "text": text.unwrap_or_default(),
        "key_rules_in_effect": keys,
    })))
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct FileText {
    /// The file's whole new text.
    text: String,
}

fn set_rules(context: &Context, args: FileText) -> ToolResult {
    let (_, warnings) = build_rules(RULES_FILE, Some(&args.text), &known_commands())
        .map_err(|errors| refused("rules.toml", &errors))?;
    write_config(&rules_path(context.root()), &args.text)?;
    let mut said = format!("Saved {}.", vault_relative(RULES_FILE));
    if !warnings.is_empty() {
        said.push_str(&format!(" Warnings: {}", describe(&warnings)));
    }
    Ok(Output::Text(said))
}

// ---- Toolbars ----

fn toolbar_json(toolbar: &Toolbar) -> Value {
    json!({
        "id": toolbar.id,
        "title": toolbar.title,
        "enabled": toolbar.enabled,
        "built_in": toolbar.built_in,
        "place": choice_name(toolbar.place),
        "behaviour": choice_name(toolbar.behaviour),
        "contexts": toolbar.contexts.iter().map(|c| choice_name(*c)).collect::<Vec<_>>(),
        "style": choice_name(toolbar.style),
        "density": choice_name(toolbar.density),
        "surface": choice_name(toolbar.surface),
        "items": item_names(&toolbar.items),
    })
}

fn get_toolbars(context: &Context, _: NoArguments) -> ToolResult {
    let text = read_config(&toolbars_path(context.root()))?;
    let toolbars = load_toolbars(context.root());
    let menus: Vec<Value> = toolbars
        .menus
        .iter()
        .map(|menu| json!({"id": menu.id, "title": menu.title, "icon": menu.icon, "items": menu.items}))
        .collect();
    let widgets: Vec<Value> = Widget::ALL
        .iter()
        .map(|widget| json!({"item": widget.name(), "title": widget.title()}))
        .collect();
    Ok(Output::Json(json!({
        "path": vault_relative(TOOLBARS_FILE),
        "text": text.unwrap_or_default(),
        "toolbars": toolbars.toolbars.iter().map(toolbar_json).collect::<Vec<_>>(),
        "menus": menus,
        "widgets": widgets,
        "other_items": [SEPARATOR, SPACER, format!("{MENU_PREFIX}<id>")],
        "places": Place::ALL.map(choice_name),
        "behaviours": Behaviour::ALL.map(choice_name),
        "styles": ButtonStyle::ALL.map(choice_name),
        "densities": Density::ALL.map(choice_name),
        "surfaces": Surface::ALL.map(choice_name),
        "contexts": ToolbarContext::ALL.map(choice_name),
    })))
}

fn set_toolbars(context: &Context, args: FileText) -> ToolResult {
    let (_, warnings) = build_toolbars(TOOLBARS_FILE, Some(&args.text), &known_commands())
        .map_err(|errors| refused(TOOLBARS_FILE, &errors))?;
    write_config(&toolbars_path(context.root()), &args.text)?;
    let mut said = format!("Saved {}.", vault_relative(TOOLBARS_FILE));
    if !warnings.is_empty() {
        said.push_str(&format!(" Warnings: {}", describe(&warnings)));
    }
    Ok(Output::Text(said))
}

// ---- Snippets and replacements ----

fn get_snippets(context: &Context, _: NoArguments) -> ToolResult {
    config_text(
        &snippets_path(context.root()),
        &vault_relative(SNIPPETS_FILE),
        DEFAULT_SNIPPETS,
    )
}

fn set_snippets(context: &Context, args: FileText) -> ToolResult {
    let file = SnippetFile::parse(&args.text).map_err(|errors| {
        let lines: Vec<String> = errors.iter().map(ToString::to_string).collect();
        ToolError::new(format!("snippets.txt wasn't saved: {}", lines.join("; ")))
    })?;
    SnippetEngine::from_file(&file)
        .map_err(|error| ToolError::new(format!("snippets.txt wasn't saved: {error}")))?;
    write_config(&snippets_path(context.root()), &args.text)?;
    Ok(Output::Text(format!(
        "Saved {} with {} snippets.",
        vault_relative(SNIPPETS_FILE),
        file.snippets().count()
    )))
}

fn get_replacements(context: &Context, _: NoArguments) -> ToolResult {
    let path = replacements_path(context.root());
    config_text(
        &path,
        &vault_relative(REPLACEMENTS_FILE),
        DEFAULT_REPLACEMENTS,
    )
}

fn set_replacements(context: &Context, args: FileText) -> ToolResult {
    Replacements::from_toml(&args.text)
        .map_err(|error| ToolError::new(format!("replacements.toml wasn't saved: {error}")))?;
    write_config(&replacements_path(context.root()), &args.text)?;
    Ok(Output::Text(format!(
        "Saved {}.",
        vault_relative(REPLACEMENTS_FILE)
    )))
}

/// A config file's path as the vault sees it, such as `.gasp/rules.toml`.
fn vault_relative(file: &str) -> String {
    format!("{CONFIG_DIR}/{file}")
}

/// A config file's text, or the built-in one when the vault has none.
fn config_text(path: &Path, label: &str, builtin: &str) -> ToolResult {
    let text = read_config(path)?;
    Ok(Output::Json(json!({
        "path": label,
        "from_vault": text.is_some(),
        "text": text.as_deref().unwrap_or(builtin),
    })))
}

fn read_config(path: &Path) -> Result<Option<String>, ToolError> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(ToolError::io(&path.to_string_lossy(), &error)),
    }
}

fn write_config(path: &Path, text: &str) -> Result<(), ToolError> {
    save(path, text).map_err(|error| ToolError::io(&path.to_string_lossy(), &error))
}

fn refused(file: &str, errors: &[Diagnostic]) -> ToolError {
    ToolError::new(format!("{file} wasn't saved: {}", describe(errors)))
}

fn describe(diagnostics: &[Diagnostic]) -> String {
    let lines: Vec<String> = diagnostics.iter().map(ToString::to_string).collect();
    lines.join("; ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::testing::{call, call_err, text, vault};

    fn read(dir: &tempfile::TempDir, file: &str) -> String {
        std::fs::read_to_string(dir.path().join(CONFIG_DIR).join(file)).unwrap_or_default()
    }

    #[test]
    fn settings_read_with_their_defaults() {
        let settings = vault_relative(SETTINGS_FILE);
        let (_dir, context) = vault(&[(settings.as_str(), "[files]\ntrash = \"vault\"\n")]);
        let files = call(&context, "get_settings", json!({"key": "files"}));
        let trash = files["settings"]
            .as_array()
            .unwrap()
            .iter()
            .find(|setting| setting["key"] == "files.trash")
            .unwrap();
        assert_eq!(trash["value"], "vault");
        assert_eq!(trash["default"], "system");
        assert_eq!(trash["set_in_file"], true);
        assert!(trash["kind"]["one_of"].as_array().unwrap().len() >= 3);
        let all = call(&context, "get_settings", json!({}));
        assert!(all["settings"].as_array().unwrap().len() > 20);
        assert!(call_err(&context, "get_settings", json!({"key": "fil"})).contains("no setting"));
    }

    #[test]
    fn set_setting_keeps_comments_and_checks_values() {
        let (dir, context) = vault(&[(
            vault_relative(SETTINGS_FILE).as_str(),
            "# Mine.\n[files]\ntrash = \"vault\"   # keep it here\n",
        )]);
        let set = call(
            &context,
            "set_setting",
            json!({"key": "files.trash", "value": "delete"}),
        );
        assert_eq!(set["value"], "delete");
        assert_eq!(
            read(&dir, "settings.toml"),
            "# Mine.\n[files]\ntrash = \"delete\"   # keep it here\n"
        );
        let error = call_err(
            &context,
            "set_setting",
            json!({"key": "files.trash", "value": "shred"}),
        );
        assert!(error.contains("wasn't changed"), "{error}");
        let error = call_err(
            &context,
            "set_setting",
            json!({"key": "nope.x", "value": 1}),
        );
        assert!(error.contains("no setting"));
        call(
            &context,
            "set_setting",
            json!({"key": "files.trash", "value": null}),
        );
        // The table keeps the comment above it, so it stays.
        assert_eq!(read(&dir, "settings.toml"), "# Mine.\n[files]\n");
        call(
            &context,
            "set_setting",
            json!({"key": "markdown.symbols.overrides.link-url", "value": "always-hidden"}),
        );
        assert!(read(&dir, "settings.toml").contains("link-url = \"always-hidden\""));
    }

    #[test]
    fn a_broken_settings_file_is_reported() {
        let (_dir, context) = vault(&[(vault_relative(SETTINGS_FILE).as_str(), "[files\n")]);
        let read = call(&context, "get_settings", json!({"key": "files.trash"}));
        assert!(read["file_error"].is_string());
        assert_eq!(read["settings"][0]["value"], "system");
    }

    #[test]
    fn theme_tokens_read_and_write() {
        let (dir, context) = vault(&[]);
        let colours = call(&context, "get_theme", json!({"prefix": "color"}));
        assert!(colours["tokens"]["color.accent"].is_string());
        let set = call(
            &context,
            "set_theme_token",
            json!({"token": "color.accent", "value": "#2f5fd0"}),
        );
        assert_eq!(set["value"], "#2f5fd0");
        assert!(read(&dir, "theme.toml").contains("accent = \"#2f5fd0\""));
        call(
            &context,
            "set_theme_token",
            json!({"token": "font.line-height.body", "value": 1.9}),
        );
        let fonts = call(&context, "get_theme", json!({"prefix": "font.line-height"}));
        assert_eq!(fonts["tokens"]["font.line-height.body"], 1.9);
        let error = call_err(
            &context,
            "set_theme_token",
            json!({"token": "color.accent", "value": "{color.nope}"}),
        );
        assert!(error.contains("wasn't changed"), "{error}");
        let error = call_err(
            &context,
            "set_theme_token",
            json!({"token": "color.nope", "value": "#fff"}),
        );
        assert!(error.contains("no theme token"));
    }

    #[test]
    fn commands_list_with_their_keys() {
        let (_dir, context) = vault(&[]);
        let bold = call(&context, "list_commands", json!({"query": "format.bold"}));
        assert_eq!(bold["commands"][0]["id"], "format.bold");
        assert_eq!(bold["commands"][0]["keys"].as_array().unwrap().len(), 1);
        let all = call(&context, "list_commands", json!({}));
        assert_eq!(
            all["commands"].as_array().unwrap().len(),
            BUILTIN_COMMANDS.len()
        );
    }

    #[test]
    fn rules_are_checked_before_saving() {
        let (dir, context) = vault(&[]);
        let rules = call(&context, "get_rules", json!({}));
        assert_eq!(rules["text"], "");
        assert!(!rules["key_rules_in_effect"].as_array().unwrap().is_empty());
        let good = "[[rule]]\nid = \"mine\"\non = \"key\"\nkeys = \"F8\"\ndo = \"tab.new\"\n";
        text(&context, "set_rules", json!({"text": good}));
        assert_eq!(read(&dir, "rules.toml"), good);
        let unknown = good.replace("tab.new", "no.such");
        let said = text(&context, "set_rules", json!({"text": unknown}));
        assert!(said.contains("no command called `no.such`"), "{said}");
        let error = call_err(&context, "set_rules", json!({"text": "[[rule]\n"}));
        assert!(error.contains("wasn't saved"));
        assert_eq!(read(&dir, "rules.toml"), unknown);
    }

    #[test]
    fn toolbars_read_and_are_checked_before_saving() {
        let (dir, context) = vault(&[]);
        let toolbars = call(&context, "get_toolbars", json!({}));
        assert_eq!(toolbars["text"], "");
        assert_eq!(toolbars["toolbars"][0]["id"], "status");
        assert_eq!(toolbars["toolbars"][1]["behaviour"], "with-selection");
        assert!(
            toolbars["widgets"]
                .as_array()
                .unwrap()
                .iter()
                .any(|w| w["item"] == "word-count")
        );
        let good = "[toolbar.status]\nitems = [\"export.html\", \"spacer\", \"word-count\"]\n";
        text(&context, "set_toolbars", json!({"text": good}));
        assert_eq!(read(&dir, "toolbars.toml"), good);
        let status = &call(&context, "get_toolbars", json!({}))["toolbars"][0];
        assert_eq!(status["items"][0], "export.html");
        let unknown = good.replace("export.html", "no.such");
        let said = text(&context, "set_toolbars", json!({"text": unknown}));
        assert!(said.contains("no command called `no.such`"), "{said}");
        let error = call_err(
            &context,
            "set_toolbars",
            json!({"text": "[toolbar.status]\nplace = \"roof\"\n"}),
        );
        assert!(error.contains("wasn't saved"), "{error}");
        assert_eq!(read(&dir, "toolbars.toml"), unknown);
    }

    #[test]
    fn snippets_and_replacements_are_validated() {
        let (dir, context) = vault(&[]);
        let builtin = call(&context, "get_snippets", json!({}));
        assert_eq!(builtin["from_vault"], false);
        assert_eq!(builtin["text"], DEFAULT_SNIPPETS);
        let said = text(
            &context,
            "set_snippets",
            json!({"text": "mk → $●$  text, instant\n"}),
        );
        assert!(said.contains("1 snippets"), "{said}");
        assert_eq!(
            call(&context, "get_snippets", json!({}))["from_vault"],
            true
        );
        let error = call_err(&context, "set_snippets", json!({"text": "no arrow here\n"}));
        assert!(error.contains("wasn't saved"), "{error}");
        assert_eq!(read(&dir, "snippets.txt"), "mk → $●$  text, instant\n");
        let replacements = call(&context, "get_replacements", json!({}));
        assert_eq!(replacements["text"], DEFAULT_REPLACEMENTS);
        let error = call_err(&context, "set_replacements", json!({"text": "[[x"}));
        assert!(error.contains("wasn't saved"));
        text(
            &context,
            "set_replacements",
            json!({"text": DEFAULT_REPLACEMENTS}),
        );
        assert_eq!(read(&dir, "replacements.toml"), DEFAULT_REPLACEMENTS);
    }
}
