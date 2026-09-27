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
    /// The split panes and each one's tabs. `open_tabs` lists the same
    /// tabs flat, for older versions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub panes: Option<PaneLayout>,
    /// The sidebar on the right: backlinks, outline and the rest.
    pub right_sidebar: RightSidebarState,
}

/// Whether the right sidebar is open, what it shows and how wide it is.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct RightSidebarState {
    pub open: bool,
    /// The view it shows, such as `backlinks`. Empty is the first one.
    pub view: String,
    /// Its width in pixels, when it isn't the default.
    pub width: Option<u32>,
}

/// A pane with its tabs, or a split of two layouts.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct PaneLayout {
    /// How a split arranges its two sides; none for a pane.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub split: Option<SplitAxis>,
    /// The first side's share of a split.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ratio: Option<f32>,
    /// A split's two sides, first (left or top) then second.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub sides: Vec<PaneLayout>,
    /// A pane's tabs, as vault-relative paths.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tabs: Vec<String>,
    /// Index into `tabs`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_tab: Option<usize>,
    /// Whether this pane had the keyboard.
    #[serde(skip_serializing_if = "is_false")]
    pub focused: bool,
}

fn is_false(value: &bool) -> bool {
    !value
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SplitAxis {
    /// Side by side.
    Row,
    /// Stacked.
    Column,
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

    #[test]
    fn nested_panes_round_trip() {
        let pane = |tab: &str, focused| PaneLayout {
            tabs: vec![tab.into()],
            active_tab: Some(0),
            focused,
            ..PaneLayout::default()
        };
        let column = PaneLayout {
            split: Some(SplitAxis::Column),
            ratio: Some(0.25),
            sides: vec![pane("b.md", true), pane("c.md", false)],
            ..PaneLayout::default()
        };
        let device = DeviceSettings {
            open_tabs: vec!["a.md".into()],
            panes: Some(PaneLayout {
                split: Some(SplitAxis::Row),
                ratio: Some(0.6),
                sides: vec![pane("a.md", false), column],
                ..PaneLayout::default()
            }),
            ..DeviceSettings::default()
        };
        let parsed: DeviceSettings = toml::from_str(&device.to_toml()).unwrap();
        assert_eq!(parsed, device);
    }
}
