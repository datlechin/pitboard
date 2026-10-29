//! Each surface drawn in-process, as GTK's own reftests draw theirs: the window's paintable,
//! rendered by the window's own renderer into a texture and saved as PNG, light and then dark,
//! as `linux-<surface>-<state>[-dark].png` in `PITBOARD_LINUX_PICTURES`, which CI uploads. Each is
//! drawn over a fixture's model, with every clock time said as one fixed time and animations
//! off, so a picture is the same from one run to the next, which each holds
//! (docs/facts/linux-app.md, The pictures).
#![cfg(target_os = "linux")]

use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use adw::prelude::*;
use gtk::glib;
use pitboard_linux::data_folder::DataFolder;
use pitboard_linux::shown::Shown;
use pitboard_linux::window::{Host, Window};
use pitboard_model::{
    AccountsShown, Front, Intent, Listener, LocalTime, Notice, Notifier, PitboardModel,
    PlatformError, Snapshot,
};

const PATIENCE: Duration = Duration::from_secs(30);

const SCALE: f32 = 2.0;

struct Told(Mutex<mpsc::Sender<Snapshot>>);

impl Listener for Told {
    fn changed(&self, snapshot: Snapshot) -> Result<(), PlatformError> {
        let _ = self.0.lock().unwrap().send(snapshot);
        Ok(())
    }
}

struct Unposted;

impl Notifier for Unposted {
    fn post(&self, _: Notice) -> Result<(), PlatformError> {
        Ok(())
    }
}

// Every clock time is 5:26 AM, on one day: the time a picture is drawn at is not in it.
struct Fixed;

impl LocalTime for Fixed {
    fn clock(&self, _: i64, with_weekday: bool) -> Result<String, PlatformError> {
        Ok(if with_weekday {
            "Fri 5:26 AM"
        } else {
            "5:26 AM"
        }
        .into())
    }

    fn same_day(&self, _: i64, _: i64) -> Result<bool, PlatformError> {
        Ok(true)
    }

    fn date_and_time(&self, _: i64) -> Result<String, PlatformError> {
        Ok("Nov 20, 2026, 5:26 AM".into())
    }

    fn time_of_day(&self, minutes: u16) -> Result<String, PlatformError> {
        Ok(format!("{}:{:02}", minutes / 60, minutes % 60))
    }

    fn date(&self, _: i64) -> Result<String, PlatformError> {
        Ok("Nov 20".into())
    }
}

// The window asks nothing of an app while it is drawn.
struct Drawn(DataFolder);

impl Host for Drawn {
    fn send(&self, _: Intent) {}
    fn open(&self, _: &str, _: &gtk::Window) {}
    fn keeps_running(&self) -> bool {
        false
    }
    fn quit(&self) {}
    fn data(&self) -> &DataFolder {
        &self.0
    }
}

#[allow(
    clippy::disallowed_methods,
    reason = "CI names the folder the pictures are kept in, and a run without it keeps them in the test's own"
)]
fn folder(scratch: &Path) -> PathBuf {
    std::env::var_os("PITBOARD_LINUX_PICTURES")
        .map_or_else(|| scratch.join("pictures"), PathBuf::from)
}

fn iterate_until(what: &str, done: impl Fn() -> bool) {
    let context = glib::MainContext::default();
    let deadline = Instant::now() + PATIENCE;
    while !done() {
        assert!(Instant::now() < deadline, "{what}: not within {PATIENCE:?}");
        if !context.iteration(false) {
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}

// What `widget` draws, at twice its own size, as a display at 2x shows it, by the renderer of the
// window it is in.
fn drawn(widget: &impl IsA<gtk::Widget>) -> glib::Bytes {
    let widget = widget.as_ref();
    let paintable = gtk::WidgetPaintable::new(Some(widget));
    let (width, height) = (paintable.intrinsic_width(), paintable.intrinsic_height());
    let snapshot = gtk::Snapshot::new();
    snapshot.scale(SCALE, SCALE);
    paintable.snapshot(&snapshot, f64::from(width), f64::from(height));
    let node = snapshot.to_node().expect("the window draws something");
    let renderer = widget
        .native()
        .and_then(|native| native.renderer())
        .expect("a renderer");
    renderer.render_texture(&node, None).save_to_png_bytes()
}

// A spinner the window shows is drawn at the angle it starts at: AdwSpinnerPaintable follows the
// frame clock of the widget it is given whatever the animations setting, and with none keeps
// still (libadwaita 1.7.6 `adw-spinner-paintable.c`). The time a picture is drawn at is not in it.
fn hold_still(widget: &gtk::Widget) {
    if let Some(status) = widget.downcast_ref::<adw::StatusPage>()
        && let Some(spinner) = status.paintable().and_downcast::<adw::SpinnerPaintable>()
    {
        spinner.set_widget(None::<&gtk::Widget>);
    }
    let mut child = widget.first_child();
    while let Some(widget) = child {
        hold_still(&widget);
        child = widget.next_sibling();
    }
}

// The app's style sheet, which AdwApplication loads from the resource as the app starts, and
// which a window drawn here has no application to load it.
fn styled() {
    let css = gtk::CssProvider::new();
    css.load_from_resource(&format!("{}/style.css", pitboard_linux::resources::BASE));
    gtk::style_context_add_provider_for_display(
        &gtk::gdk::Display::default().expect("headless.sh's display"),
        &css,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}

/// The window over the fixture `world`, as the model shows it once `shows` holds, light and
/// dark, saved as `linux-accounts-<state>`.
fn picture(world: &str, state: &str, start: bool, shows: impl Fn(&AccountsShown) -> bool) {
    adw::init().unwrap();
    pitboard_linux::resources::register();
    styled();
    gtk::Settings::default()
        .unwrap()
        .set_gtk_enable_animations(false);
    // The same folder on every run, since a failure to read names its path.
    let scratch = Path::new("/tmp/pitboard-linux-pictures").join(state);
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let (told, heard) = mpsc::channel();
    let model = PitboardModel::fixture_in(
        world.into(),
        Front::Gnome,
        scratch.to_string_lossy().into_owned(),
        Arc::new(Told(Mutex::new(told))),
        None,
        Arc::new(Unposted),
        Arc::new(Fixed),
    )
    .unwrap();
    let mut shown = model.snapshot();
    if start {
        model.send(Intent::Start);
        let deadline = Instant::now() + PATIENCE;
        while !shows(&shown.accounts_shown) || shown.reading {
            shown = heard
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .unwrap_or_else(|_| panic!("{world}: {state} not shown within {PATIENCE:?}"));
        }
    }
    assert!(
        shows(&shown.accounts_shown),
        "{world}: {:?}",
        shown.accounts_shown
    );
    let store = Shown::new(shown);
    let data = DataFolder::at(&scratch.join("data"));
    let window = Window::new(None, &store, Rc::new(Drawn(data)));
    window.present();
    iterate_until("the window drawn", || window.is_mapped());
    hold_still(window.upcast_ref());
    let folder = folder(&scratch);
    std::fs::create_dir_all(&folder).unwrap();
    let style = adw::StyleManager::default();
    for (scheme, suffix) in [
        (adw::ColorScheme::ForceLight, ""),
        (adw::ColorScheme::ForceDark, "-dark"),
    ] {
        style.set_color_scheme(scheme);
        let dark = !suffix.is_empty();
        iterate_until("the scheme drawn", || style.is_dark() == dark);
        while glib::MainContext::default().iteration(false) {}
        let first = drawn(&window);
        let name = format!("linux-accounts-{state}{suffix}");
        std::fs::write(folder.join(format!("{name}.png")), &first).unwrap();
        // Drawn again a moment later, as the next run would draw it, and kept where it differs.
        std::thread::sleep(Duration::from_millis(300));
        while glib::MainContext::default().iteration(false) {}
        let again = drawn(&window);
        if again != first {
            std::fs::write(folder.join(format!("{name}-again.png")), &again).unwrap();
            panic!("{name} differs from one drawing to the next");
        }
    }
    window.destroy();
    model.shutdown();
    std::fs::remove_dir_all(&scratch).unwrap();
}

#[gtk::test]
fn accounts_reading() {
    picture("oneTool", "reading", false, |shown| {
        matches!(shown, AccountsShown::Reading { .. })
    });
}

#[gtk::test]
fn accounts_no_tool() {
    picture("noClaudeCode", "noTool", true, |shown| {
        matches!(shown, AccountsShown::NoTool { .. })
    });
}

#[gtk::test]
fn accounts_no_accounts() {
    picture("empty", "noAccounts", true, |shown| {
        matches!(shown, AccountsShown::NoAccounts { .. })
    });
}

#[gtk::test]
fn accounts_read_failed() {
    picture("readFailure", "readFailed", true, |shown| {
        matches!(shown, AccountsShown::ReadFailed { .. })
    });
}

#[gtk::test]
fn accounts_two_tools() {
    picture("twoTools", "twoTools", true, |shown| {
        matches!(shown, AccountsShown::List)
    });
}

#[gtk::test]
fn accounts_notices() {
    picture("stuck", "notices", true, |shown| {
        matches!(shown, AccountsShown::List)
    });
}

#[gtk::test]
fn accounts_setup() {
    picture("onlyOne", "setup", true, |shown| {
        matches!(shown, AccountsShown::List)
    });
}
