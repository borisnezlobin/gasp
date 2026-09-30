//! The install window, as `--show-install` opens it: the breach plays to
//! its end, and its buttons move the app through a stand-in installer,
//! open it from where it is, or say it can't be moved here.

use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gasp_desktop::install::scene::Moment;
use gasp_desktop::install::view::open_install_window;
use gasp_desktop::install::{
    InstallError, InstallHooks, InstallView, Installer, Placement, Prepared, Progress, Source,
    Step, Unsupported,
};
use gpui::{Entity, Modifiers, TestAppContext, VisualTestContext, WindowHandle, point, px};

const TARGET: &str = "/Applications/Gasp.app";

/// Records each step it's asked to take.
#[derive(Default)]
struct FakeInstaller {
    calls: Mutex<Vec<&'static str>>,
    /// Whether a Gasp in Applications is open until asked to quit.
    running: bool,
}

impl FakeInstaller {
    fn calls(&self) -> Vec<&'static str> {
        self.calls.lock().unwrap().clone()
    }

    fn record(&self, call: &'static str) {
        self.calls.lock().unwrap().push(call);
    }
}

impl Installer for FakeInstaller {
    fn prepare(&self, _: &Placement) -> Result<Prepared, InstallError> {
        self.record("prepare");
        Ok(Prepared {
            target: PathBuf::from(TARGET),
            running: self.running,
        })
    }

    fn quit_running(&self, _: &Path) -> Result<(), InstallError> {
        self.record("quit");
        Ok(())
    }

    fn copy(&self, _: &Placement, target: &Path, progress: &Progress) -> Result<(), InstallError> {
        assert_eq!(target, Path::new(TARGET));
        self.record("copy");
        progress.set(1.);
        Ok(())
    }

    fn open_and_tidy(&self, placement: &Placement, _: &Path) -> Result<(), InstallError> {
        assert_eq!(placement.mount(), Some(Path::new("/Volumes/Gasp")));
        self.record("open");
        Ok(())
    }
}

/// What the hooks were asked to do.
#[derive(Default)]
struct Outcome {
    opened_here: Cell<bool>,
    remembered: Cell<bool>,
    finished: Cell<bool>,
}

fn from_disk_image() -> Placement {
    Placement {
        bundle: PathBuf::from("/Volumes/Gasp/Gasp.app"),
        original: None,
        source: Source::DiskImage {
            mount: PathBuf::from("/Volumes/Gasp"),
        },
    }
}

fn open(
    cx: &mut TestAppContext,
    installer: Arc<dyn Installer>,
) -> (Entity<InstallView>, VisualTestContext, Rc<Outcome>) {
    let outcome = Rc::new(Outcome::default());
    let (here, remembered, finished) = (outcome.clone(), outcome.clone(), outcome.clone());
    let hooks = InstallHooks {
        open_here: Rc::new(move |_, _| here.opened_here.set(true)),
        remember_not_now: Rc::new(move |placement| {
            assert_eq!(placement, &from_disk_image());
            remembered.remembered.set(true);
        }),
        finished: Rc::new(move |_, _| finished.finished.set(true)),
    };
    let handle: WindowHandle<InstallView> = cx
        .update(|cx| open_install_window(from_disk_image(), installer, hooks, Some(false), cx))
        .expect("the install window opens");
    let view = handle.root(cx).unwrap();
    let window = VisualTestContext::from_window(handle.into(), cx);
    (view, window, outcome)
}

fn step(view: &Entity<InstallView>, window: &mut VisualTestContext) -> Step {
    view.read_with(window, |view, _| view.step().clone())
}

/// Lets `duration` pass a frame at a time, so timers set as others fire
/// get their turn.
fn wait(window: &mut VisualTestContext, duration: Duration) {
    let frame = Duration::from_millis(16);
    for _ in 0..duration.as_millis() / frame.as_millis() {
        window.executor().advance_clock(frame);
        window.run_until_parked();
    }
}

fn click(window: &mut VisualTestContext, label: &str) {
    let selector: &'static str = format!("button-{label}").leak();
    let bounds = window
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("a {label} button"));
    window.simulate_click(bounds.center(), Modifiers::none());
    window.run_until_parked();
}

#[gpui::test]
fn the_breach_plays_to_the_icon_then_the_button_moves_the_app(cx: &mut TestAppContext) {
    let installer = Arc::new(FakeInstaller::default());
    let (view, mut window, outcome) = open(cx, installer.clone());
    assert_eq!(step(&view, &mut window), Step::Breaching);
    assert_eq!(
        view.read_with(&window, |view, _| view.moment()),
        None,
        "it's animating"
    );

    wait(&mut window, Duration::from_secs(2));
    assert_eq!(step(&view, &mut window), Step::Ready);
    assert_eq!(
        view.read_with(&window, |view, _| view.moment()),
        Some(Moment::REST)
    );

    click(&mut window, "Move to Applications");
    assert_eq!(installer.calls(), ["prepare", "copy"]);
    assert_eq!(
        step(&view, &mut window),
        Step::Moving(PathBuf::from(TARGET))
    );
    wait(&mut window, Duration::from_secs(3));
    assert_eq!(installer.calls(), ["prepare", "copy", "open"]);
    assert_eq!(
        step(&view, &mut window),
        Step::Opening(PathBuf::from(TARGET))
    );
    assert!(
        outcome.finished.get(),
        "the app quits, leaving the moved copy open"
    );
    assert!(!outcome.remembered.get());
}

#[gpui::test]
fn any_key_skips_the_breach(cx: &mut TestAppContext) {
    let (view, mut window, _) = open(cx, Arc::new(FakeInstaller::default()));
    window.simulate_keystrokes("a");
    assert_eq!(step(&view, &mut window), Step::Ready);
}

#[gpui::test]
fn a_click_on_the_page_skips_the_breach(cx: &mut TestAppContext) {
    let installer = Arc::new(FakeInstaller::default());
    let (view, mut window, _) = open(cx, installer.clone());
    window.simulate_click(point(px(30.), px(30.)), Modifiers::none());
    assert_eq!(step(&view, &mut window), Step::Ready);
    assert!(installer.calls().is_empty(), "skipping moves nothing");
}

#[gpui::test]
fn not_now_opens_from_here_and_is_remembered(cx: &mut TestAppContext) {
    let installer = Arc::new(FakeInstaller::default());
    let (_, mut window, outcome) = open(cx, installer.clone());
    click(&mut window, "Not now");
    assert!(outcome.remembered.get());
    assert!(outcome.opened_here.get());
    assert!(installer.calls().is_empty());
}

#[gpui::test]
fn an_open_copy_in_applications_quits_before_it_is_replaced(cx: &mut TestAppContext) {
    let installer = Arc::new(FakeInstaller {
        running: true,
        ..FakeInstaller::default()
    });
    let (view, mut window, outcome) = open(cx, installer.clone());
    window.simulate_keystrokes("space");
    window.simulate_keystrokes("enter");
    assert_eq!(
        step(&view, &mut window),
        Step::AskToQuit(PathBuf::from(TARGET))
    );
    click(&mut window, "Quit it and replace");
    wait(&mut window, Duration::from_secs(3));
    assert_eq!(installer.calls(), ["prepare", "quit", "copy", "open"]);
    assert!(outcome.finished.get());
}

#[gpui::test]
fn off_a_mac_the_button_says_it_only_works_on_one(cx: &mut TestAppContext) {
    let (view, mut window, outcome) = open(cx, Arc::new(Unsupported));
    window.simulate_keystrokes("space");
    click(&mut window, "Move to Applications");
    assert_eq!(
        step(&view, &mut window),
        Step::Failed(InstallError::Unsupported)
    );
    assert!(window.debug_bounds("button-Try again").is_none());
    click(&mut window, "Open Gasp");
    assert!(outcome.opened_here.get());
    assert!(!outcome.remembered.get());
}
