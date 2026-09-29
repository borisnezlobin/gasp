//! Every setting appears in the schema with a type and a default.

use gasp_config::Settings;
use gasp_config::merge::flatten;
use gasp_config::schema::setting_descriptors;

#[test]
fn every_settings_field_is_described() {
    let defaults = toml::Table::try_from(Settings::default()).unwrap();
    let descriptors = setting_descriptors();
    let leaves = flatten(&defaults);
    assert!(leaves.len() >= 11);
    for (key, value) in leaves {
        let descriptor = descriptors
            .iter()
            .find(|d| d.key == key)
            .unwrap_or_else(|| panic!("{key} is missing from the schema"));
        let expected = serde_json::to_value(&value).unwrap();
        assert_eq!(descriptor.default, expected, "default of {key}");
    }
}

#[test]
fn every_descriptor_is_a_real_setting() {
    let defaults = toml::Table::try_from(Settings::default()).unwrap();
    let keys: Vec<String> = flatten(&defaults).into_iter().map(|(k, _)| k).collect();
    for descriptor in setting_descriptors() {
        assert!(
            keys.contains(&descriptor.key),
            "{} isn't a setting",
            descriptor.key
        );
        assert!(
            descriptor.description.is_some(),
            "{} has no description",
            descriptor.key
        );
    }
}

#[test]
fn json_schema_serializes() {
    let schema = gasp_config::schema::settings_json_schema();
    let json = serde_json::to_string(&schema).unwrap();
    assert!(json.contains("sidebar"));
}
