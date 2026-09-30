//! The once-a-day usage ping: the app's version, the platform, the
//! system's version and the chip type, sent to gaspmd.com so the owner
//! can count how many people use Gasp. Nothing else goes with it, and it
//! stops when any open vault turns `telemetry.enabled` off.
//!
//! It only runs from [`crate::app::launch`], never in a snapshot, a bench
//! or a test, and never in a debug build, so development runs don't count.

use std::time::Duration;

use gpui::{App, AsyncApp};
use reqwest::blocking::Client;
use serde::Serialize;

use crate::workspace::Workspace;
use crate::workspace::state::AppState;

pub const PING_URL: &str = "https://gaspmd.com/api/ping";

/// How long after launch the ping waits, so it never competes with the
/// first frames or with opening the vault.
const SETTLE_DELAY: Duration = Duration::from_secs(20);

const TIMEOUT: Duration = Duration::from_secs(5);

const SYSTEM_VERSION_FILE: &str = "/System/Library/CoreServices/SystemVersion.plist";

/// Exactly what the ping sends.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Ping {
    pub version: String,
    pub platform: &'static str,
    pub os: String,
    pub arch: &'static str,
}

impl Ping {
    pub fn for_this_mac(os: String) -> Ping {
        Ping {
            version: env!("CARGO_PKG_VERSION").to_string(),
            platform: "mac",
            os,
            arch: chip_name(std::env::consts::ARCH),
        }
    }

    pub fn body(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}

/// The server's name for a chip: `arm64` for Apple silicon, `x86_64` for
/// Intel.
pub fn chip_name(rust_arch: &str) -> &'static str {
    match rust_arch {
        "aarch64" => "arm64",
        _ => "x86_64",
    }
}

/// Whether a ping is due today, given the day the last one went out.
pub fn is_due(last_ping: Option<&str>, today: &str) -> bool {
    last_ping != Some(today)
}

/// Today's date on this Mac, such as `2026-09-30`.
pub fn today() -> String {
    jiff::Zoned::now().date().to_string()
}

/// The `ProductVersion` in the text of `SystemVersion.plist`.
pub fn product_version(plist: &str) -> Option<String> {
    let after_key = plist.split("<key>ProductVersion</key>").nth(1)?;
    let value = after_key.split("<string>").nth(1)?.split("</string>").next()?;
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn system_version() -> Option<String> {
    let plist = std::fs::read_to_string(SYSTEM_VERSION_FILE).ok()?;
    product_version(&plist)
}

/// Sends today's ping a little after launch, when one is due and allowed.
pub fn schedule(cx: &mut App) {
    if cfg!(debug_assertions) || !crate::sandbox::reaches_outside() {
        return;
    }
    cx.spawn(async move |cx| send_when_settled(cx).await)
        .detach();
}

async fn send_when_settled(cx: &mut AsyncApp) {
    cx.background_executor().timer(SETTLE_DELAY).await;
    let allowed = cx.update(every_open_vault_allows).unwrap_or(false);
    let Some(state_path) = AppState::default_path() else {
        return;
    };
    let today = today();
    if !allowed || !is_due(AppState::load(&state_path).last_ping.as_deref(), &today) {
        return;
    }
    let sent = cx
        .background_executor()
        .spawn(async { send(&Ping::for_this_mac(system_version().unwrap_or_default())) })
        .await;
    if sent {
        remember_ping(&state_path, today);
    }
}

/// Whether no open vault has turned the ping off. The welcome tour, with
/// no vault open yet, keeps the default.
fn every_open_vault_allows(cx: &mut App) -> bool {
    cx.windows()
        .into_iter()
        .filter_map(|handle| handle.downcast::<Workspace>())
        .all(|workspace| {
            workspace
                .read_with(cx, |workspace, _| workspace.config().settings.telemetry.enabled)
                .unwrap_or(true)
        })
}

fn send(ping: &Ping) -> bool {
    let Ok(client) = Client::builder().timeout(TIMEOUT).build() else {
        return false;
    };
    client
        .post(PING_URL)
        .header("content-type", "application/json")
        .body(ping.body())
        .send()
        .is_ok_and(|response| response.status().is_success())
}

fn remember_ping(state_path: &std::path::Path, today: String) {
    let mut state = AppState::load(state_path);
    state.last_ping = Some(today);
    let _ = state.save(state_path);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_ping_is_due_once_a_day() {
        assert!(is_due(None, "2026-09-30"));
        assert!(is_due(Some("2026-09-29"), "2026-09-30"));
        assert!(!is_due(Some("2026-09-30"), "2026-09-30"));
    }

    #[test]
    fn a_clock_set_back_still_pings_once() {
        assert!(is_due(Some("2026-10-02"), "2026-09-30"));
    }

    #[test]
    fn the_payload_carries_only_the_four_fields() {
        let ping = Ping {
            version: "0.1.0".to_string(),
            platform: "mac",
            os: "15.1".to_string(),
            arch: chip_name("aarch64"),
        };
        assert_eq!(
            ping.body(),
            r#"{"version":"0.1.0","platform":"mac","os":"15.1","arch":"arm64"}"#
        );
        let this_mac = Ping::for_this_mac("26.0".to_string());
        assert_eq!(this_mac.version, env!("CARGO_PKG_VERSION"));
        assert!(["arm64", "x86_64"].contains(&this_mac.arch));
    }

    #[test]
    fn intel_macs_report_x86_64() {
        assert_eq!(chip_name("x86_64"), "x86_64");
        assert_eq!(chip_name("aarch64"), "arm64");
    }

    #[test]
    fn the_system_version_comes_from_the_plist() {
        let plist = "<dict>\n\t<key>ProductName</key>\n\t<string>macOS</string>\n\t<key>ProductVersion</key>\n\t<string>15.1</string>\n</dict>";
        assert_eq!(product_version(plist).as_deref(), Some("15.1"));
        assert_eq!(product_version("<dict></dict>"), None);
    }

    #[test]
    fn today_is_a_calendar_date() {
        let today = today();
        assert_eq!(today.len(), 10);
        assert_eq!(today.as_bytes()[4], b'-');
    }
}
