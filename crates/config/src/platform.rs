//! Platforms, platform filters and input contexts used by rules.

use serde::{Deserialize, Serialize};

/// The operating system the app is running on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    Macos,
    Windows,
    Linux,
    Ios,
}

impl Platform {
    /// Every platform, in a stable order.
    pub const ALL: [Platform; 4] = [
        Platform::Macos,
        Platform::Windows,
        Platform::Linux,
        Platform::Ios,
    ];

    /// The platform this binary was compiled for (Linux for anything unknown).
    pub fn current() -> Platform {
        if cfg!(target_os = "macos") {
            Platform::Macos
        } else if cfg!(target_os = "windows") {
            Platform::Windows
        } else if cfg!(target_os = "ios") {
            Platform::Ios
        } else {
            Platform::Linux
        }
    }

    /// True for the Apple platforms, where `Mod` means Cmd.
    pub fn is_apple(self) -> bool {
        matches!(self, Platform::Macos | Platform::Ios)
    }
}

/// The `platform` field of a rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PlatformFilter {
    Desktop,
    Mobile,
    Macos,
    Windows,
    Linux,
    Ios,
}

const FILTER_TABLE: &[(PlatformFilter, &[Platform])] = &[
    (
        PlatformFilter::Desktop,
        &[Platform::Macos, Platform::Windows, Platform::Linux],
    ),
    (PlatformFilter::Mobile, &[Platform::Ios]),
    (PlatformFilter::Macos, &[Platform::Macos]),
    (PlatformFilter::Windows, &[Platform::Windows]),
    (PlatformFilter::Linux, &[Platform::Linux]),
    (PlatformFilter::Ios, &[Platform::Ios]),
];

impl PlatformFilter {
    /// Whether a rule with this filter applies on `platform`.
    pub fn matches(self, platform: Platform) -> bool {
        FILTER_TABLE
            .iter()
            .find(|(filter, _)| *filter == self)
            .is_some_and(|(_, platforms)| platforms.contains(&platform))
    }
}

/// Whether an optional filter admits `platform` (no filter admits everything).
pub fn filter_admits(filter: Option<PlatformFilter>, platform: Platform) -> bool {
    filter.is_none_or(|filter| filter.matches(platform))
}

/// The kind of text the cursor is in, used by a rule's `when` field.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InputContext {
    Text,
    Math,
    Code,
    Link,
    Frontmatter,
    Table,
    Html,
    Comment,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_filter_covers_the_three_desktop_platforms() {
        let desktop = PlatformFilter::Desktop;
        assert!(desktop.matches(Platform::Macos));
        assert!(desktop.matches(Platform::Windows));
        assert!(desktop.matches(Platform::Linux));
        assert!(!desktop.matches(Platform::Ios));
    }

    #[test]
    fn mobile_filter_is_only_ios() {
        assert!(PlatformFilter::Mobile.matches(Platform::Ios));
        assert!(!PlatformFilter::Mobile.matches(Platform::Linux));
    }

    #[test]
    fn missing_filter_admits_every_platform() {
        assert!(Platform::ALL.iter().all(|p| filter_admits(None, *p)));
    }
}
