//! A flat description of every setting, generated from the settings types.
//!
//! A settings screen walks [`setting_descriptors`] to build one control per entry.

use schemars::r#gen::SchemaSettings;
use schemars::schema::{InstanceType, RootSchema, Schema, SchemaObject, SingleOrVec};
use serde_json::Value;

use crate::merge::join_key;
use crate::settings::Settings;

/// The JSON Schema for `settings.toml`, with nested types inlined.
pub fn settings_json_schema() -> RootSchema {
    SchemaSettings::draft07()
        .with(|settings| settings.inline_subschemas = true)
        .into_generator()
        .into_root_schema_for::<Settings>()
}

/// What kind of control a setting needs.
#[derive(Clone, Debug, PartialEq)]
pub enum SettingKind {
    Bool,
    Integer,
    Number,
    Text,
    /// One of a fixed list of strings.
    Choice(Vec<String>),
    /// A table from names to values of one kind.
    Map(Box<SettingKind>),
    /// A list of values of one kind, such as file patterns.
    List(Box<SettingKind>),
}

/// One setting as a settings screen sees it.
#[derive(Clone, Debug, PartialEq)]
pub struct SettingDescriptor {
    /// Dotted key, such as `sidebar.files.reveal`.
    pub key: String,
    pub kind: SettingKind,
    pub default: Value,
    pub description: Option<String>,
}

/// Every leaf setting with its kind, default and description.
pub fn setting_descriptors() -> Vec<SettingDescriptor> {
    let root = settings_json_schema();
    let mut descriptors = Vec::new();
    collect(&root.schema, "", None, &mut descriptors);
    descriptors
}

fn collect(
    schema: &SchemaObject,
    key: &str,
    default: Option<&Value>,
    out: &mut Vec<SettingDescriptor>,
) {
    let Some(object) = schema.object.as_ref().filter(|o| !o.properties.is_empty()) else {
        out.extend(leaf(schema, key, default));
        return;
    };
    for (name, property) in &object.properties {
        let Schema::Object(property) = property else {
            continue;
        };
        let child_default = property.metadata.as_ref().and_then(|m| m.default.as_ref());
        collect(property, &join_key(key, name), child_default, out);
    }
}

fn leaf(schema: &SchemaObject, key: &str, default: Option<&Value>) -> Option<SettingDescriptor> {
    Some(SettingDescriptor {
        key: key.to_string(),
        kind: kind_of(schema)?,
        default: default?.clone(),
        description: schema.metadata.as_ref().and_then(|m| m.description.clone()),
    })
}

fn kind_of(schema: &SchemaObject) -> Option<SettingKind> {
    if let Some(values) = &schema.enum_values {
        return Some(SettingKind::Choice(string_values(values)));
    }
    match single_type(schema)? {
        InstanceType::Object => map_kind(schema),
        InstanceType::Array => list_kind(schema),
        other => scalar_kind(other),
    }
}

const SCALAR_KINDS: &[(InstanceType, SettingKind)] = &[
    (InstanceType::Boolean, SettingKind::Bool),
    (InstanceType::Integer, SettingKind::Integer),
    (InstanceType::Number, SettingKind::Number),
    (InstanceType::String, SettingKind::Text),
];

fn scalar_kind(instance: InstanceType) -> Option<SettingKind> {
    SCALAR_KINDS
        .iter()
        .find(|(known, _)| *known == instance)
        .map(|(_, kind)| kind.clone())
}

fn map_kind(schema: &SchemaObject) -> Option<SettingKind> {
    let values = schema.object.as_ref()?.additional_properties.as_deref()?;
    let Schema::Object(values) = values else {
        return None;
    };
    Some(SettingKind::Map(Box::new(kind_of(values)?)))
}

fn list_kind(schema: &SchemaObject) -> Option<SettingKind> {
    let SingleOrVec::Single(items) = schema.array.as_ref()?.items.as_ref()? else {
        return None;
    };
    let Schema::Object(items) = items.as_ref() else {
        return None;
    };
    Some(SettingKind::List(Box::new(kind_of(items)?)))
}

fn single_type(schema: &SchemaObject) -> Option<InstanceType> {
    match schema.instance_type.as_ref()? {
        SingleOrVec::Single(instance) => Some(**instance),
        SingleOrVec::Vec(_) => None,
    }
}

fn string_values(values: &[Value]) -> Vec<String> {
    values
        .iter()
        .filter_map(|v| v.as_str().map(str::to_string))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn find(key: &str) -> SettingDescriptor {
        setting_descriptors()
            .into_iter()
            .find(|d| d.key == key)
            .unwrap_or_else(|| panic!("{key} missing from the schema"))
    }

    #[test]
    fn reveal_is_a_choice_of_three() {
        let reveal = find("sidebar.files.reveal");
        assert_eq!(
            reveal.kind,
            SettingKind::Choice(vec!["always".into(), "toggle".into(), "hover".into()])
        );
        assert_eq!(reveal.default, Value::from("toggle"));
        assert!(reveal.description.is_some());
    }

    #[test]
    fn overrides_are_a_map_of_modes() {
        let overrides = find("markdown.symbols.overrides");
        assert!(matches!(overrides.kind, SettingKind::Map(_)));
        let device_only = find("sync.device-only");
        assert_eq!(
            device_only.kind,
            SettingKind::List(Box::new(SettingKind::Text))
        );
    }

    #[test]
    fn numbers_and_flags_have_kinds() {
        assert_eq!(find("appearance.base-font-size").kind, SettingKind::Integer);
        assert_eq!(find("files.update-links-on-rename").kind, SettingKind::Bool);
        assert_eq!(find("files.attachments-folder").kind, SettingKind::Text);
    }
}
