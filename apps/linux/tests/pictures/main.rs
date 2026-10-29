//! Each surface drawn in-process, as GTK's own reftests draw theirs: the window's paintable,
//! rendered by the window's own renderer into a texture and saved as PNG, light and then dark,
//! as `linux-<surface>-<state>[-dark].png` in `PITBOARD_LINUX_PICTURES`, which CI uploads. Each is
//! drawn over a fixture's model, with every clock time said as one fixed time and animations
//! off, so a picture is the same from one run to the next, which each holds
//! (docs/facts/linux-app.md, The pictures). A dialog is drawn in the window it comes up over,
//! as the model shows it once it has taken what the picture sends it. A header bar's title cut
//! short fails its picture.
#![cfg(target_os = "linux")]

use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use adw::prelude::*;
use gtk::glib;
use pitboard_linux::data_folder::DataFolder;
use pitboard_linux::platform::autostart::StartAtSignIn;
use pitboard_linux::platform::stand_in;
use pitboard_linux::preferences::Preferences;
use pitboard_linux::shown::Shown;
use pitboard_linux::window::{AccountGroup, Host, Window};
use pitboard_model::{
    AccountsShown, Front, Intent, Listener, LocalTime, Notice, Notifier, Pane, PitboardModel,
    Place, PlatformError, Sheet, Snapshot, Tool,
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

// The window asks nothing of an app while it is drawn, and its start at sign-in is a
// fixture's stand-in, off.
struct Drawn {
    data: DataFolder,
    start_at_sign_in: stand_in::Autostart,
}

impl Host for Drawn {
    fn send(&self, _: Intent) {}
    fn open(&self, _: &str, _: &gtk::Window) {}
    fn keeps_running(&self) -> bool {
        false
    }
    fn quit(&self) {}
    fn data(&self) -> &DataFolder {
        &self.data
    }
    fn token(&self) -> Option<String> {
        None
    }
    fn local_time(&self) -> Arc<dyn LocalTime> {
        Arc::new(Fixed)
    }
    fn start_at_sign_in(&self) -> Option<&dyn StartAtSignIn> {
        Some(&self.start_at_sign_in)
    }
    fn own_folders(&self) -> Vec<PathBuf> {
        Vec::new()
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

// Runs the main loop until `window` has drawn `count` frames more, each asked for once: one
// asked for and not drawn yet leaves the window with nothing to snapshot.
fn frames(window: &Window, count: i64) {
    let clock = window.frame_clock().expect("a mapped window's clock");
    for _ in 0..count {
        let next = clock.frame_counter() + 1;
        window.queue_draw();
        iterate_until("its frame drawn", || clock.frame_counter() >= next);
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
    let paintable = match (
        widget.downcast_ref::<adw::StatusPage>(),
        widget.downcast_ref::<gtk::Image>(),
    ) {
        (Some(status), _) => status.paintable(),
        (_, Some(image)) => image.paintable(),
        _ => None,
    };
    if let Some(spinner) = paintable.and_downcast::<adw::SpinnerPaintable>() {
        spinner.set_widget(None::<&gtk::Widget>);
    }
    let mut child = widget.first_child();
    while let Some(widget) = child {
        hold_still(&widget);
        child = widget.next_sibling();
    }
}

// What a header bar under `widget` shows cut short: a title GTK ellipsizes for want of room.
fn cut_short(widget: &gtk::Widget, in_header: bool) -> Vec<String> {
    let in_header = in_header || widget.is::<adw::HeaderBar>();
    let mut cut = Vec::new();
    if let Some(label) = widget.downcast_ref::<gtk::Label>()
        && in_header
        && label.is_mapped()
        && label.layout().is_ellipsized()
    {
        cut.push(label.label().into());
    }
    let mut child = widget.first_child();
    while let Some(widget) = child {
        cut.extend(cut_short(&widget, in_header));
        child = widget.next_sibling();
    }
    cut
}

// The app's style sheet and icons, which AdwApplication and GtkApplication take from the
// resource as the app starts, and which a window drawn here has no application to take.
fn styled() {
    let display = gtk::gdk::Display::default().expect("headless.sh's display");
    let css = gtk::CssProvider::new();
    css.load_from_resource(&format!("{}/style.css", pitboard_linux::resources::BASE));
    gtk::style_context_add_provider_for_display(
        &display,
        &css,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
    gtk::IconTheme::for_display(&display)
        .add_resource_path(&format!("{}/icons", pitboard_linux::resources::BASE));
}

// What a step of a picture sends the model, and what the model shows once it has taken it.
type Step = (Option<Intent>, fn(&Snapshot) -> bool);

/// The window over the fixture `world`, as the model shows it once it has taken each of
/// `steps` in turn, with `then` done in it, light and dark, saved as `linux-<name>`. Each step's
/// intent is sent once the step before shows what it waits for, from the first read on where
/// `start`.
fn picture(
    name: &str,
    world: &str,
    start: bool,
    steps: &[Step],
    then: impl Fn(&Window),
    dialog: bool,
) {
    adw::init().unwrap();
    pitboard_linux::resources::register();
    styled();
    let settings = gtk::Settings::default().unwrap();
    settings.set_gtk_enable_animations(false);
    // A focused field's cursor would be in one drawing and not the next.
    settings.set_gtk_cursor_blink(false);
    // The same folder on every run, since a failure to read names its path.
    let scratch = Path::new("/tmp/pitboard-linux-pictures").join(name);
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
        let wait = |done: &dyn Fn(&Snapshot) -> bool, shown: &mut Snapshot| {
            while !done(shown) || shown.reading {
                *shown = heard
                    .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                    .unwrap_or_else(|_| panic!("{world}: {name} not shown within {PATIENCE:?}"));
            }
        };
        wait(
            &|shown| !matches!(shown.accounts_shown, AccountsShown::Reading { .. }),
            &mut shown,
        );
        for (intent, done) in steps {
            if let Some(intent) = intent {
                model.send(intent.clone());
            }
            wait(done, &mut shown);
        }
    }
    let store = Shown::new(shown);
    let drawn = Drawn {
        data: DataFolder::at(&scratch.join("data")),
        start_at_sign_in: stand_in::Autostart::new(&scratch),
    };
    let window = Window::new(None, &store, Rc::new(drawn));
    window.present();
    iterate_until("the window drawn", || window.is_mapped());
    then(&window);
    iterate_until("its dialog drawn", || {
        window.visible_dialog().is_some_and(|dialog| {
            dialog.is_mapped()
                && dialog
                    .default_widget()
                    .is_none_or(|button| button.is_mapped())
        }) == dialog
    });
    // A dialog comes up over the frames after it is mapped, at once with animations off.
    frames(&window, 3);
    let cut = cut_short(window.upcast_ref(), false);
    assert!(cut.is_empty(), "{name}: {cut:?} cut short");
    hold_still(window.upcast_ref());
    draw(&window, &format!("linux-{name}"), &folder(&scratch));
    window.destroy();
    model.shutdown();
    std::fs::remove_dir_all(&scratch).unwrap();
}

/// `window` light and then dark, as `<name>.png` and `<name>-dark.png` in `folder`, each drawn
/// twice and held the same.
fn draw(window: &Window, name: &str, folder: &Path) {
    std::fs::create_dir_all(folder).unwrap();
    let style = adw::StyleManager::default();
    for (scheme, suffix) in [
        (adw::ColorScheme::ForceLight, ""),
        (adw::ColorScheme::ForceDark, "-dark"),
    ] {
        style.set_color_scheme(scheme);
        let dark = !suffix.is_empty();
        iterate_until("the scheme drawn", || style.is_dark() == dark);
        frames(window, 1);
        let first = drawn(window);
        let name = format!("{name}{suffix}");
        std::fs::write(folder.join(format!("{name}.png")), &first).unwrap();
        // Drawn again a moment later, as the next run would draw it, and kept where it differs.
        std::thread::sleep(Duration::from_millis(300));
        frames(window, 1);
        let again = drawn(window);
        if again != first {
            std::fs::write(folder.join(format!("{name}-again.png")), &again).unwrap();
            panic!("{name} differs from one drawing to the next");
        }
    }
}

/// The accounts page over `world`, once what it shows in place of the list or the list is
/// `shows`, saved as `linux-accounts-<state>`.
fn accounts(world: &str, state: &str, start: bool, shows: fn(&Snapshot) -> bool) {
    picture(
        &format!("accounts-{state}"),
        world,
        start,
        &[(None, shows)],
        |_| {},
        false,
    );
}

/// The code a Claude Code sign-in in a fixture takes, as its browser page shows one.
const CODE: &str = "fixture-code#state";

fn adding() -> Step {
    (
        Some(Intent::PresentSheet {
            sheet: Sheet::Add { tool: None },
        }),
        |shown| shown.sheet_text.is_some(),
    )
}

fn signing_in() -> Step {
    (
        Some(Intent::SignIn {
            tool: Tool::Claude,
            again: None,
        }),
        |shown| shown.signing_in_text.is_some(),
    )
}

fn asked_for_the_code() -> Step {
    (None, |shown| {
        shown
            .signing_in
            .as_ref()
            .is_some_and(|running| running.wants_code)
    })
}

#[gtk::test]
fn accounts_reading() {
    accounts("oneTool", "reading", false, |shown| {
        matches!(shown.accounts_shown, AccountsShown::Reading { .. })
    });
}

#[gtk::test]
fn accounts_no_tool() {
    accounts("noClaudeCode", "noTool", true, |shown| {
        matches!(shown.accounts_shown, AccountsShown::NoTool { .. })
    });
}

#[gtk::test]
fn accounts_no_accounts() {
    accounts("empty", "noAccounts", true, |shown| {
        matches!(shown.accounts_shown, AccountsShown::NoAccounts { .. })
    });
}

#[gtk::test]
fn accounts_read_failed() {
    accounts("readFailure", "readFailed", true, |shown| {
        matches!(shown.accounts_shown, AccountsShown::ReadFailed { .. })
    });
}

#[gtk::test]
fn accounts_two_tools() {
    accounts("twoTools", "twoTools", true, |shown| {
        matches!(shown.accounts_shown, AccountsShown::List)
    });
}

#[gtk::test]
fn accounts_notices() {
    accounts("stuck", "notices", true, |shown| {
        matches!(shown.accounts_shown, AccountsShown::List)
    });
}

#[gtk::test]
fn accounts_setup() {
    accounts("onlyOne", "setup", true, |shown| {
        matches!(shown.accounts_shown, AccountsShown::List)
    });
}

#[gtk::test]
fn sign_in_tool() {
    picture("signIn-tool", "twoTools", true, &[adding()], |_| {}, true);
}

#[gtk::test]
fn sign_in_waiting() {
    let waiting: Step = (None, |shown| {
        shown
            .signing_in
            .as_ref()
            .is_some_and(|running| !running.wants_code)
    });
    picture(
        "signIn-waiting",
        "oneTool",
        true,
        &[adding(), signing_in(), waiting],
        |_| {},
        true,
    );
}

#[gtk::test]
fn sign_in_code() {
    picture(
        "signIn-code",
        "oneTool",
        true,
        &[adding(), signing_in(), asked_for_the_code()],
        |_| {},
        true,
    );
}

// No world's sign-in fails to start, so the sign-in fails as a person's can in each: Claude
// Code refuses a code typed back without its state.
#[gtk::test]
fn sign_in_failed() {
    let refused: Step = (
        Some(Intent::PasteCode {
            code: "fixture-code".into(),
        }),
        |shown| (shown.signing_in_text.as_ref()).is_some_and(|text| text.refused.is_some()),
    );
    picture(
        "signIn-failed",
        "oneTool",
        true,
        &[adding(), signing_in(), asked_for_the_code(), refused],
        |_| {},
        true,
    );
}

#[gtk::test]
fn name() {
    let named: Step = (Some(Intent::PasteCode { code: CODE.into() }), |shown| {
        matches!(shown.sheet, Some(Sheet::Name { .. })) && shown.sheet_text.is_some()
    });
    picture(
        "name",
        "oneTool",
        true,
        &[adding(), signing_in(), asked_for_the_code(), named],
        |_| {},
        true,
    );
}

#[gtk::test]
fn question_remove() {
    let read: Step = (None, |shown| {
        matches!(shown.accounts_shown, AccountsShown::List)
    });
    picture(
        "question-remove",
        "oneTool",
        true,
        &[read],
        |window| {
            let group = every_group(window)
                .into_iter()
                .find(|group| group.title() == "personal")
                .expect("personal's group");
            group
                .activate_action("account.remove", None)
                .expect("its menu's Remove");
        },
        true,
    );
}

#[gtk::test]
fn open_link_empty() {
    let read: Step = (None, |shown| {
        matches!(shown.accounts_shown, AccountsShown::List)
    });
    picture(
        "openLink-empty",
        "oneTool",
        true,
        &[read],
        |window| {
            window
                .activate_action("pitboard.open-link", None)
                .expect("the main menu's Open Link");
        },
        true,
    );
}

#[gtk::test]
fn open_link_in_browser() {
    let arrived: Step = (
        Some(Intent::LinkArrived {
            text: "pitboard-debug://open?url=https%3A%2F%2Fclaude.ai%2Fchat%2F1f2e".into(),
        }),
        |shown| shown.account_windows.picker.is_some(),
    );
    picture(
        "openLink-inBrowser",
        "oneTool",
        true,
        &[arrived],
        |_| {},
        true,
    );
}

fn listed() -> Step {
    (None, |shown| {
        matches!(shown.accounts_shown, AccountsShown::List)
    })
}

// Preferences over the accounts, on its page `page`, with what `steps` read.
fn preferences(name: &str, page: &'static str, steps: &[Step]) {
    picture(
        name,
        "oneTool",
        true,
        steps,
        move |window| {
            window
                .activate_action("pitboard.preferences", None)
                .expect("the main menu's Preferences");
            let preferences = window
                .visible_dialog()
                .and_downcast::<Preferences>()
                .expect("Preferences");
            preferences.set_visible_page_name(page);
        },
        true,
    );
}

#[gtk::test]
fn preferences_general() {
    preferences("preferences-general", "general", &[listed()]);
}

#[gtk::test]
fn preferences_limits() {
    preferences("preferences-limits", "limits", &[listed()]);
}

#[gtk::test]
fn preferences_advanced() {
    let read: Step = (Some(Intent::ReadSchedule), |_| true);
    let found: Step = (Some(Intent::LookForCommandLine), |shown| {
        shown.machine.command_line.in_terminal.is_some()
    });
    preferences("preferences-advanced", "advanced", &[listed(), read, found]);
}

// A page of the window's, pushed over the accounts by its menu's `action`, once `shown` reads
// what it shows.
fn page(name: &str, action: &'static str, shown: Step) {
    picture(
        name,
        "oneTool",
        true,
        &[listed(), shown],
        move |window| {
            window
                .activate_action(action, None)
                .expect("the main menu's item");
        },
        false,
    );
}

#[gtk::test]
fn diagnostics() {
    let checked: Step = (Some(Intent::PaneShown { pane: Pane::Checks }), |shown| {
        let checks = &shown.machine.checks;
        checks.report.is_some() && !checks.checking
    });
    page("diagnostics", "pitboard.diagnostics", checked);
}

#[gtk::test]
fn activity() {
    let read: Step = (
        Some(Intent::PaneShown {
            pane: Pane::Activity,
        }),
        |shown| !shown.machine.activity.lines.is_empty(),
    );
    page("activity", "pitboard.activity", read);
}

#[gtk::test]
fn welcome() {
    let welcomed: Step = (None, |shown| {
        shown.window_request.place == Some(Place::Welcome) && !shown.welcome.tools.is_empty()
    });
    picture("welcome", "firstLaunch", true, &[welcomed], |_| {}, true);
}

// A dialog the main menu's `action` brings up over the accounts.
fn from_the_menu(name: &str, action: &'static str) {
    picture(
        name,
        "oneTool",
        true,
        &[listed()],
        move |window| {
            window
                .activate_action(action, None)
                .expect("the main menu's item");
        },
        true,
    );
}

#[gtk::test]
fn about() {
    from_the_menu("about", "pitboard.about");
}

#[gtk::test]
fn shortcuts() {
    from_the_menu("shortcuts", "pitboard.shortcuts");
}

// The window's account groups, in the order drawn.
fn every_group(window: &Window) -> Vec<AccountGroup> {
    let mut found = Vec::new();
    let mut next = vec![window.clone().upcast::<gtk::Widget>()];
    while let Some(widget) = next.pop() {
        if let Ok(group) = widget.clone().downcast::<AccountGroup>() {
            found.push(group);
        }
        let mut child = widget.last_child();
        while let Some(widget) = child {
            child = widget.prev_sibling();
            next.push(widget);
        }
    }
    found
}
