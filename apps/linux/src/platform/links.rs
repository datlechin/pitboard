//! Links that arrive, as the text the model reads a link from: what GApplication's `open`
//! hands over for a `pitboard://` link, and text pasted or dropped on a window. The glue reads
//! nothing in a link itself; the model takes any text and decides. And the browser a link the
//! model hands out opens in.
//!
//! GApplication makes a `gio::File` of each link, on the command line and over the bus alike,
//! and GVfs, which GNOME desktops run, writes a link of a scheme it does not know again:
//! `pitboard://open?url=…` as `pitboard://open/?url=…` (docs/facts/linux-app.md, Links). So
//! the app makes the files of its own scheme itself, GLib's way for a scheme of one app's.

use gtk::prelude::*;
use gtk::{gdk, gio, glib};

use super::Failed;

/// Has GIO make each file of `scheme` in this process as a file that gives its link back as
/// written, before anything makes one: called before the app runs. False where the scheme
/// already has its maker.
pub fn hand_on_as_written(scheme: &str) -> bool {
    // GLib's own VFS keeps a link of a scheme it does not know as it was given. Made here,
    // before any lookup: made first inside one, it did not return in nine minutes (GLib 2.84.4).
    let local = gio::Vfs::local();
    gio::Vfs::default().register_uri_scheme(
        scheme,
        Some(Box::new(move |vfs, link| {
            // Where GLib's own is the default, as for a setuid program, it makes the file.
            (vfs != &local).then(|| local.file_for_uri(link))
        })),
        None,
    )
}

/// What `open` was handed, each as written where [`hand_on_as_written`] took its scheme.
pub fn opened(files: &[gio::File]) -> Vec<String> {
    files.iter().map(|file| file.uri().into()).collect()
}

/// The clipboard's text, handed to `arrived` once it is read, where the clipboard holds text.
pub fn paste(clipboard: &gdk::Clipboard, arrived: impl FnOnce(String) + 'static) {
    clipboard.read_text_async(gio::Cancellable::NONE, move |read| {
        if let Ok(Some(text)) = read {
            arrived(text.into());
        }
    });
}

/// A drop target for a window that hands each text dropped on it to `arrived`.
pub fn drop_target(arrived: impl Fn(String) + 'static) -> gtk::DropTarget {
    let target = gtk::DropTarget::new(glib::Type::STRING, gdk::DragAction::COPY);
    target.connect_drop(move |_, value, _, _| match value.get::<String>() {
        Ok(text) => {
            arrived(text);
            true
        }
        Err(_) => false,
    });
    target
}

/// Opens a link the model hands out: the desktop's browser, or a fixture's stand-in.
pub trait Opener {
    fn open(&self, link: &str, parent: Option<&gtk::Window>) -> Result<(), Failed>;
}

/// The desktop's own handler for the link's scheme, through the OpenURI portal where one runs.
pub struct Browser;

impl Opener for Browser {
    // As the other apps open one: whether a browser took it is not the model's to hear.
    fn open(&self, link: &str, parent: Option<&gtk::Window>) -> Result<(), Failed> {
        gtk::UriLauncher::new(link).launch(parent, gio::Cancellable::NONE, |_| {});
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;
    use std::time::{Duration, Instant};

    const LINK: &str = "pitboard-debug://open?url=https%3A%2F%2Fclaude.ai%2Fchat%2F1f2e";

    fn collector() -> (Rc<RefCell<Vec<String>>>, impl Fn(String) + Clone + 'static) {
        let handed = Rc::new(RefCell::new(Vec::new()));
        let hand = {
            let handed = handed.clone();
            move |text| handed.borrow_mut().push(text)
        };
        (handed, hand)
    }

    // The main context's iterations until `done`, for ten seconds at most.
    fn iterate_until(done: impl Fn() -> bool) {
        let context = glib::MainContext::default();
        let deadline = Instant::now() + Duration::from_secs(10);
        while !done() {
            assert!(Instant::now() < deadline, "not done in ten seconds");
            if !context.iteration(false) {
                std::thread::sleep(Duration::from_millis(5));
            }
        }
    }

    // As a launch's command line or a second launch over the bus hands it to `open`: an app
    // with no id is never on the bus.
    #[test]
    fn a_link_opened_is_handed_on_as_written() {
        let links = [
            LINK,
            "pitboard-debug://open?url=https://chatgpt.com/c/68a9#reply",
            "pitboard-debug:open",
        ];
        assert!(hand_on_as_written("pitboard-debug"));
        let app = gio::Application::new(None, gio::ApplicationFlags::HANDLES_OPEN);
        let (handed, hand) = collector();
        app.connect_open(move |_, files, _| opened(files).into_iter().for_each(&hand));

        let arguments: Vec<_> = std::iter::once("pitboard-app").chain(links).collect();
        assert_eq!(app.run_with_args(&arguments), glib::ExitCode::SUCCESS);

        assert_eq!(*handed.borrow(), links);
    }

    #[gtk::test]
    fn pasted_text_is_handed_on_once() {
        let display = gdk::Display::default().expect("headless.sh gives a display");
        let clipboard = display.clipboard();
        clipboard.set_text(LINK);
        let (handed, hand) = collector();

        paste(&clipboard, hand);
        iterate_until(|| !handed.borrow().is_empty());
        clipboard.set_text("pitboard-debug://open?url=https%3A%2F%2Fchatgpt.com");
        while glib::MainContext::default().iteration(false) {}

        assert_eq!(*handed.borrow(), [LINK]);
    }

    #[gtk::test]
    fn a_clipboard_without_text_hands_on_nothing() {
        let clipboard = gdk::Display::default().unwrap().clipboard();
        let texture = gdk::MemoryTexture::new(
            1,
            1,
            gdk::MemoryFormat::R8g8b8a8,
            &glib::Bytes::from_static(&[0, 0, 0, 0]),
            4,
        );
        clipboard.set_texture(&texture);
        let (handed, hand) = collector();
        let refused = Rc::new(Cell::new(false));

        paste(&clipboard, hand);
        // A read of its own, refused as the paste's is.
        let told = refused.clone();
        clipboard.read_text_async(gio::Cancellable::NONE, move |read| told.set(read.is_err()));
        iterate_until(|| refused.get());
        while glib::MainContext::default().iteration(false) {}

        assert!(handed.borrow().is_empty());
    }

    #[gtk::test]
    fn dropped_text_is_handed_on_as_dropped() {
        let (handed, hand) = collector();
        let target = drop_target(hand);
        assert_eq!(target.types().as_slice(), [glib::Type::STRING]);
        assert!(target.actions().contains(gdk::DragAction::COPY));

        let accepted: bool =
            target.emit_by_name("drop", &[&glib::BoxedValue(LINK.to_value()), &0.0, &0.0]);

        assert!(accepted);
        assert_eq!(*handed.borrow(), [LINK]);
    }
}
