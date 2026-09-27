//! Device-local state from `device.toml`. This file never syncs.

use serde::{Deserialize, Serialize};

/// Settings that belong to one machine, such as window size and open tabs.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct DeviceSettings {
    pub window: WindowState,
    /// Vault-relative paths of the open tabs, in order.
    pub open_tabs: Vec<String>,
    /// Index into `open_tabs`.
    pub active_tab: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct WindowState {
    pub width: u32,
    pub height: u32,
    pub x: Option<i32>,
    pub y: Option<i32>,
    pub maximized: bool,
}

impl Default for WindowState {
    fn default() -> Self {
        WindowState {
            width: 1200,
            height: 800,
            x: None,
            y: None,
            maximized: false,
        }
    }
}

impl DeviceSettings {
    /// Serializes for writing back to `device.toml`.
    pub fn to_toml(&self) -> String {
        toml::to_string_pretty(self).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_toml() {
        let device = DeviceSettings {
            open_tabs: vec!["notes/a.md".into(), "b.md".into()],
            active_tab: Some(1),
            ..DeviceSettings::default()
        };
        let parsed: DeviceSettings = toml::from_str(&device.to_toml()).unwrap();
        assert_eq!(parsed, device);
    }
}
