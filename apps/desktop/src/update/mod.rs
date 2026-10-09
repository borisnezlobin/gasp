//! Updates for the Mac and Linux apps. A little after launch, and then
//! once a day, the app asks gaspmd.com for the newest published version
//! and offers it in a notice; "Check for updates" in the app menu or the
//! command palette asks right away.
//!
//! On the Mac, updating downloads the release's disk image, checks the
//! app in it ([`verify`]), copies it beside the running bundle, and on
//! restart a small helper swaps the two once the app has quit and saved.
//! On Linux a copy installed from the tarball replaces its own binary
//! ([`linux`]), and one a package manager installed opens the download
//! page.
//!
//! None of it runs outside an installed app (a `.app` bundle, or the
//! Linux tarball's or package's folders), so `cargo run`, tests and
//! snapshot runs never ask, and the daily check stops when any open vault
//! turns `updates.check` off.

pub mod bundle;
pub mod install;
#[cfg(target_os = "linux")]
pub mod linux;
pub mod release;
pub mod tools;
pub mod verify;

use std::io::Read;
use std::path::PathBuf;
use std::time::Duration;

use futures::StreamExt;
use futures::channel::mpsc;
use gpui::{App, AsyncApp, Global};
use semver::Version;

use crate::keymap::RunCommand;
use crate::notices::{self, Notice};
use bundle::CheckGate;
#[cfg(target_os = "macos")]
use bundle::running_bundle;
use install::{InstallError, Prepared};
use release::{Release, VERSION_URL, is_newer, parse_release, running_version};

pub const CHECK_COMMAND: &str = "app.check-for-updates";
pub const INSTALL_COMMAND: &str = "app.install-update";
pub const RESTART_COMMAND: &str = "app.restart-to-update";
pub const NOTES_COMMAND: &str = "app.release-notes";

/// The commands this module runs, for the menus.
pub const COMMANDS: [&str; 4] = [
    CHECK_COMMAND,
    INSTALL_COMMAND,
    RESTART_COMMAND,
    NOTES_COMMAND,
];

/// How long after the first frame the first check waits, so it never
/// competes with opening the vault.
const FIRST_CHECK_DELAY: Duration = Duration::from_secs(5);
const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
const CHECK_TIMEOUT: Duration = Duration::from_secs(10);
/// More than the site's answer ever needs.
const MAX_ANSWER_BYTES: u64 = 16 * 1024;

/// Whether this module runs `id`.
pub fn handles(id: &str) -> bool {
    COMMANDS.contains(&id)
}

/// What a check found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CheckOutcome {
    Newer(Release),
    UpToDate,
    Failed,
}

/// What a check's answer means for the app running `running`. The site
/// answers 404, read here as `Ok(None)`, while no release is published.
pub fn outcome_of(answer: Result<Option<Release>, String>, running: &Version) -> CheckOutcome {
    match answer {
        Ok(Some(release)) if is_newer(&release.version, running) => CheckOutcome::Newer(release),
        Ok(_) => CheckOutcome::UpToDate,
        Err(_) => CheckOutcome::Failed,
    }
}

/// What the update in progress reports back to the main thread.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InstallProgress {
    Downloaded(u8),
    Verifying,
    Finished(Result<Prepared, InstallError>),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
enum Phase {
    #[default]
    Idle,
    Checking,
    Installing,
    Staged,
}

#[derive(Default)]
struct Updater {
    phase: Phase,
    /// The newest release a check found.
    found: Option<Release>,
    /// The newest version offered without being asked for since launch,
    /// so a dismissed notice doesn't come back until the next launch.
    offered: Option<Version>,
    notice: Option<u64>,
}

impl Global for Updater {}

/// Runs the update commands and starts the daily check. Only the app's
/// launch calls this.
pub fn install(cx: &mut App) {
    register_commands(cx);
    if crate::first_frame::is_waiting() {
        crate::first_frame::defer(start_daily_checks);
    } else {
        start_daily_checks(cx);
    }
}

/// Runs the update commands from any window, and from the menu bar when
/// no window is open.
pub fn register_commands(cx: &mut App) {
    cx.on_action(|action: &RunCommand, cx| {
        if !run_command(&action.id, cx) {
            cx.propagate();
        }
    });
}

fn run_command(id: &str, cx: &mut App) -> bool {
    let run: fn(&mut App) = match id {
        CHECK_COMMAND => |cx| check(true, cx),
        INSTALL_COMMAND => start_install,
        RESTART_COMMAND => restart,
        NOTES_COMMAND => open_release_notes,
        _ => return false,
    };
    // A command from the palette arrives inside its window's update, and
    // the check reads every window's settings, its own included, which
    // GPUI only allows once that update is over.
    cx.defer(run);
    true
}

fn start_daily_checks(cx: &mut App) {
    cx.spawn(async move |cx| {
        cx.background_executor().timer(FIRST_CHECK_DELAY).await;
        loop {
            cx.update(|cx| check(false, cx)).ok();
            cx.background_executor().timer(CHECK_INTERVAL).await;
        }
    })
    .detach();
}

/// Where to say a check can be asked for.
const WHERE_TO_CHECK: &str = if cfg!(target_os = "macos") {
    "Check for updates in the Gasp menu"
} else {
    "Check for updates in the command palette"
};

/// Whether this copy runs installed, so it may check for updates.
#[cfg(target_os = "macos")]
fn is_installed() -> bool {
    running_bundle().is_some()
}

#[cfg(target_os = "linux")]
fn is_installed() -> bool {
    linux::running_install().is_some()
}

fn gate(cx: &mut App) -> CheckGate {
    CheckGate {
        in_bundle: is_installed(),
        reaches_outside: crate::sandbox::reaches_outside(),
        enabled: crate::telemetry::every_open_vault_allows(cx, |settings| settings.updates.check),
    }
}

/// Asks the site for the newest version: `manual` when asked from the
/// menu, which always says what it found.
fn check(manual: bool, cx: &mut App) {
    let gate = gate(cx);
    if manual && !gate.allows_any_check() {
        say(
            Notice::done("Gasp can only update itself when it runs as an installed app."),
            cx,
        );
        return;
    }
    if !manual && !gate.allows_daily_check() {
        return;
    }
    if updater(cx).phase == Phase::Staged {
        if manual {
            show_ready(cx);
        }
        return;
    }
    if updater(cx).phase != Phase::Idle {
        return;
    }
    updater(cx).phase = Phase::Checking;
    cx.spawn(async move |cx| {
        let answer = cx
            .background_executor()
            .spawn(async { fetch_newest() })
            .await;
        let outcome = outcome_of(answer, &running_version());
        cx.update(|cx| {
            updater(cx).phase = Phase::Idle;
            report(outcome, manual, cx);
        })
        .ok();
    })
    .detach();
}

fn fetch_newest() -> Result<Option<Release>, String> {
    let response = reqwest::blocking::Client::builder()
        .timeout(CHECK_TIMEOUT)
        .build()
        .and_then(|client| client.get(VERSION_URL).send())
        .map_err(|error| error.to_string())?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    if !response.status().is_success() {
        return Err(response.status().to_string());
    }
    let mut body = String::new();
    response
        .take(MAX_ANSWER_BYTES)
        .read_to_string(&mut body)
        .map_err(|error| error.to_string())?;
    parse_release(&body)
        .map(Some)
        .map_err(|error| error.to_string())
}

/// Says what a check found: a newer version once per launch unless
/// `manual`, and anything else only when `manual`.
pub fn report(outcome: CheckOutcome, manual: bool, cx: &mut App) {
    match outcome {
        CheckOutcome::Newer(release) => offer(release, manual, cx),
        CheckOutcome::UpToDate if manual => say(
            Notice::done(format!("Gasp {} is up to date.", running_version())),
            cx,
        ),
        CheckOutcome::Failed if manual => say(
            Notice::problem("Gasp couldn’t reach gaspmd.com to check for updates."),
            cx,
        ),
        _ => {}
    }
}

fn offer(release: Release, manual: bool, cx: &mut App) {
    let updater = updater(cx);
    let already_offered = updater
        .offered
        .as_ref()
        .is_some_and(|offered| *offered >= release.version);
    updater.found = Some(release.clone());
    if already_offered && !manual {
        return;
    }
    updater.offered = Some(release.version.clone());
    say(
        Notice::offer(format!("Gasp {} is out.", release.version))
            .with_link("See what’s new", NOTES_COMMAND)
            .with_action("Update", INSTALL_COMMAND),
        cx,
    );
}

fn start_install(cx: &mut App) {
    let Some(release) = updater(cx).found.clone() else {
        return check(true, cx);
    };
    if updater(cx).phase != Phase::Idle {
        return check(true, cx);
    }
    let Some(target) = install_target(&release, cx) else {
        return;
    };
    updater(cx).phase = Phase::Installing;
    apply_progress(InstallProgress::Downloaded(0), cx);
    let (sender, mut receiver) = mpsc::unbounded();
    cx.background_executor()
        .spawn(async move { download_and_prepare(&release, target, &sender) })
        .detach();
    cx.spawn(async move |cx: &mut AsyncApp| {
        while let Some(progress) = receiver.next().await {
            cx.update(|cx| apply_progress(progress, cx)).ok();
        }
    })
    .detach();
}

/// What an update replaces: the running bundle on the Mac, the running
/// binary on Linux. `None` once it's said why there's nothing to replace.
#[cfg(target_os = "macos")]
fn install_target(_: &Release, cx: &mut App) -> Option<PathBuf> {
    let bundle = running_bundle();
    if bundle.is_none() {
        check(true, cx);
    }
    bundle
}

#[cfg(target_os = "linux")]
fn install_target(release: &Release, cx: &mut App) -> Option<PathBuf> {
    match linux::running_install() {
        Some(linux::Install::Tarball { binary }) => Some(binary),
        Some(linux::Install::Managed) => {
            crate::sandbox::open_url(linux::DOWNLOAD_PAGE, cx);
            say(
                Notice::offer(format!(
                    "Install Gasp {} the way you installed this copy.",
                    release.version
                )),
                cx,
            );
            None
        }
        None => {
            check(true, cx);
            None
        }
    }
}

fn download_and_prepare(
    release: &Release,
    target: PathBuf,
    sender: &mpsc::UnboundedSender<InstallProgress>,
) {
    let folder = install::download_folder(&release.version);
    let downloaded = install::download(release, &folder, |percent| {
        let _ = sender.unbounded_send(InstallProgress::Downloaded(percent));
    });
    let prepared = downloaded.and_then(|file| {
        let _ = sender.unbounded_send(InstallProgress::Verifying);
        prepare_download(&file, release, &target)
    });
    if prepared.is_err() {
        let _ = std::fs::remove_dir_all(&folder);
    }
    let _ = sender.unbounded_send(InstallProgress::Finished(prepared));
}

#[cfg(target_os = "macos")]
fn prepare_download(
    dmg: &std::path::Path,
    release: &Release,
    bundle: &std::path::Path,
) -> Result<Prepared, InstallError> {
    install::prepare(dmg, release, bundle, &running_version(), &tools::MacTools)
}

#[cfg(target_os = "linux")]
fn prepare_download(
    tarball: &std::path::Path,
    release: &Release,
    binary: &std::path::Path,
) -> Result<Prepared, InstallError> {
    linux::prepare(tarball, release, binary, &running_version()).map(Prepared::Staged)
}

/// Shows how the update in progress is going, in one notice that changes
/// in place.
pub fn apply_progress(progress: InstallProgress, cx: &mut App) {
    let version = updater(cx)
        .found
        .as_ref()
        .map_or_else(String::new, |release| release.version.to_string());
    match progress {
        InstallProgress::Downloaded(percent) => say(
            Notice::offer(format!("Downloading Gasp {version}…")).with_progress(percent),
            cx,
        ),
        InstallProgress::Verifying => say(
            Notice::offer(format!("Checking Gasp {version}…")).with_progress(100),
            cx,
        ),
        InstallProgress::Finished(prepared) => finish(prepared, cx),
    }
}

fn finish(prepared: Result<Prepared, InstallError>, cx: &mut App) {
    match prepared {
        Ok(Prepared::Staged(_)) => {
            updater(cx).phase = Phase::Staged;
            show_ready(cx);
        }
        Ok(Prepared::DragToFinish(dmg)) => {
            updater(cx).phase = Phase::Idle;
            crate::sandbox::open_with_system(&dmg, cx);
            say(Notice::offer("Drag Gasp into Applications to finish."), cx);
        }
        Err(error) => {
            updater(cx).phase = Phase::Idle;
            say(
                Notice::problem("Gasp didn’t update").with_detail(update_refusal(&error)),
                cx,
            );
        }
    }
}

fn show_ready(cx: &mut App) {
    let version = updater(cx)
        .found
        .as_ref()
        .map_or_else(String::new, |release| format!(" {}", release.version));
    say(
        Notice::offer(format!("Gasp{version} is ready."))
            .with_action("Restart to update", RESTART_COMMAND),
        cx,
    );
}

/// Why the restart helper didn't start.
enum RestartFailure {
    NotInstalled,
    /// The new version has gone from where the update put it.
    Missing,
    Spawn(std::io::Error),
}

/// Starts the helper that swaps the staged version in and opens it once
/// the app has quit.
#[cfg(target_os = "macos")]
fn start_restart_helper() -> Result<(), RestartFailure> {
    let bundle = running_bundle().ok_or(RestartFailure::NotInstalled)?;
    let plan = install::RestartPlan::for_this_process(&bundle);
    if !plan.staged.exists() {
        return Err(RestartFailure::Missing);
    }
    install::spawn_restart_helper(&plan).map_err(RestartFailure::Spawn)
}

/// Starts the helper that opens the new binary, already in place, once
/// the app has quit.
#[cfg(target_os = "linux")]
fn start_restart_helper() -> Result<(), RestartFailure> {
    let Some(linux::Install::Tarball { binary }) = linux::running_install() else {
        return Err(RestartFailure::NotInstalled);
    };
    if !binary.is_file() {
        return Err(RestartFailure::Missing);
    }
    linux::spawn_restart_helper(&binary).map_err(RestartFailure::Spawn)
}

/// Starts the helper that swaps the staged version in, then quits the
/// usual way, which saves every open note first.
fn restart(cx: &mut App) {
    if updater(cx).phase != Phase::Staged {
        return check(true, cx);
    }
    match start_restart_helper() {
        Ok(()) => cx.quit(),
        Err(RestartFailure::NotInstalled) => check(true, cx),
        Err(RestartFailure::Missing) => {
            updater(cx).phase = Phase::Idle;
            say(
                Notice::problem("Gasp couldn’t find the update it downloaded. Try updating again."),
                cx,
            );
        }
        Err(RestartFailure::Spawn(error)) => {
            say(
                crate::notices::failure("Gasp couldn’t restart to update", error),
                cx,
            );
        }
    }
}

fn open_release_notes(cx: &mut App) {
    match updater(cx).found.clone() {
        Some(release) => crate::sandbox::open_url(&release.notes, cx),
        None => check(true, cx),
    }
}

/// Shows `notice` in place of the last update notice.
/// Why an update didn't install, in the notice's words. A refusal (the
/// download isn't Gasp, or isn't newer) is said as it is; anything else
/// goes to the log, and the notice says how to try again.
fn update_refusal(error: &install::InstallError) -> String {
    match error {
        install::InstallError::Refused(refusal) => {
            let reason = refusal.to_string();
            let mut letters = reason.chars();
            letters.next().map_or(String::new(), |first| {
                format!("{}{}.", first.to_uppercase(), letters.as_str())
            })
        }
        install::InstallError::Failed(message) => {
            eprintln!("update failed: {message}");
            format!("The download didn’t finish. Try again with {WHERE_TO_CHECK}.")
        }
    }
}

fn say(notice: Notice, cx: &mut App) {
    let previous = updater(cx).notice;
    let id = notices::replace(previous, notice, cx);
    updater(cx).notice = Some(id);
}

fn updater(cx: &mut App) -> &mut Updater {
    cx.default_global::<Updater>()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(version: &str) -> Release {
        Release {
            version: Version::parse(version).unwrap(),
            url: "https://example.com/Gasp.dmg".into(),
            notes: "https://example.com/notes".into(),
            published: "2026-10-01T09:00:00Z".into(),
            size: 1,
            sha256: None,
        }
    }

    #[test]
    fn a_check_reads_its_answer_against_the_running_version() {
        let running = Version::new(0, 2, 0);
        assert_eq!(
            outcome_of(Ok(Some(release("0.3.0"))), &running),
            CheckOutcome::Newer(release("0.3.0"))
        );
        assert_eq!(
            outcome_of(Ok(Some(release("0.2.0"))), &running),
            CheckOutcome::UpToDate
        );
        assert_eq!(
            outcome_of(Ok(Some(release("0.1.0"))), &running),
            CheckOutcome::UpToDate
        );
        assert_eq!(outcome_of(Ok(None), &running), CheckOutcome::UpToDate);
        assert_eq!(
            outcome_of(Err("offline".into()), &running),
            CheckOutcome::Failed
        );
    }

    #[test]
    fn the_menus_know_every_update_command() {
        for id in COMMANDS {
            assert!(handles(id));
            assert!(gasp_config::commands::command_spec(id).is_some(), "{id}");
        }
        assert!(!handles("app.print"));
    }
}
