//! The fixtures: worlds an app launches into in place of the machine it runs on, for its UI
//! tests and for looking at it with nothing real at stake. Either app's debug build launches
//! one by name, and both get the same world, since each is made here once.
//!
//! A world is the real core and the real model over a machine of the fixture's own: its
//! keychain, its vault, its process list and its scheduler in memory, its files in a folder
//! of its own in the temporary directory, Anthropic and OpenAI answering from a script, and
//! every account put there the way a person would have put it, signed in, enrolled, parked
//! and switched, each change logged when it would have been. So a world shows what the core
//! makes of it, and none of the core's rules is written twice. What the fixture plays is
//! only what is outside the core: each tool's own sign-in, the person at the browser, the
//! other apps on the machine, and the services. Nothing a world does reaches this machine's
//! homes, keychain, scheduler or network, and the model `fixture` makes posts no
//! notification.
//!
//! What is exported is the same in every build, so one set of generated bindings serves a
//! library built with the `fixture` feature and one built without: `PitboardModel::fixture`,
//! `PitboardModel::fixture_in`, `fixture_names` and `fixture_page`. Without the feature no
//! world is compiled at all, and each refuses, naming the feature, or answers that there is
//! none. The Linux app, which links the model as Rust, asks `fixture_refusal` before it owns a
//! name on the bus, so a launch refused there makes nothing.

use crate::Front;
use crate::model::{Listener, LocalTime, Notifier, OtherApps, PitboardModel};
use std::sync::Arc;

#[cfg(feature = "fixture")]
mod apps;
#[cfg(feature = "fixture")]
mod pages;
#[cfg(all(test, feature = "fixture"))]
mod tests;
#[cfg(feature = "fixture")]
mod tools;
#[cfg(feature = "fixture")]
mod worlds;

/// Why there is no fixture to launch into, or no stand-in page to serve.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, uniffi::Error)]
pub enum FixtureError {
    /// This library was built without the `fixture` feature, as every library an app ships
    /// is, so it has no fixtures. The reason says how to build one that has.
    #[error("{reason}")]
    Unavailable { reason: String },
    /// No fixture has that name. The reason names every one there is.
    #[error("{reason}")]
    Unknown { reason: String },
    /// The fixture's own folder could not be made, or its world could not be put in it.
    #[error("{reason}")]
    Failed { reason: String },
}

/// The name of every fixture, in the order the macOS app's `Fixture` listed them: what a
/// debug build reads from `PITBOARD_FIXTURE`. Empty for a library built without the
/// `fixture` feature, which has none.
#[uniffi::export]
pub fn fixture_names() -> Vec<String> {
    #[cfg(feature = "fixture")]
    {
        worlds::World::ALL
            .iter()
            .map(|world| world.name().to_owned())
            .collect()
    }
    #[cfg(not(feature = "fixture"))]
    {
        Vec::new()
    }
}

/// The page a fixture's account window is served for `url`, an address on the scheme its
/// stand-in sites are served under, `pitboard-fixture`: a stand-in for each site, a sign-in
/// stand-in for the hosts a site's sign-in goes to, which closes itself once it is done,
/// and the frame an artifact is in. HTML, in UTF-8. Both apps serve these, so a UI test sees
/// the same page on either. Refused by a library built without the `fixture` feature.
#[uniffi::export]
pub fn fixture_page(url: String) -> Result<String, FixtureError> {
    #[cfg(feature = "fixture")]
    {
        Ok(pages::page(&url))
    }
    #[cfg(not(feature = "fixture"))]
    {
        let _ = url;
        Err(library_unavailable())
    }
}

#[uniffi::export]
impl PitboardModel {
    /// The model of the fixture called `name`, one of `fixture_names`, for the app `front`
    /// names, in place of the one `new` makes over this machine: the same model over the real
    /// core, on the fixture's own
    /// machine, which it empties and makes again, so every launch starts where the last one
    /// did. Its preferences and what it has told are that machine's too, and empty at each
    /// launch; only `firstLaunch` is a machine whose app has never been seen. Other apps are
    /// the fixture's, where ChatGPT runs Codex's login in `chatGPTOpen` and quits when asked,
    /// and Claude, a stand-in of the core's, runs in `claudeDesktopOpen`, quits when asked and,
    /// opened in front for a sign-in, signs in and quits; a GNOME model names none, as a live
    /// one names none. Nothing is posted as a notification.
    /// Clock times are said as `local_time` says them. Nothing runs until the app sends
    /// `Intent::Start`.
    ///
    /// The machine is the folder `pitboard-fixture` in this process's temporary directory,
    /// where the macOS app's own fixture code keeps what it makes, so one fixture at a time:
    /// a second made in the same process empties the first's, and so does one made by any
    /// other process with the same temporary directory. Refused, naming the feature, by a
    /// library built without `fixture`, and for a name that is none of them.
    #[uniffi::constructor]
    pub fn fixture(
        name: String,
        front: Front,
        listener: Arc<dyn Listener>,
        local_time: Arc<dyn LocalTime>,
    ) -> Result<Arc<Self>, FixtureError> {
        let world = launchable(front, &name)?;
        #[cfg(feature = "fixture")]
        {
            let folder = worlds::Folder::shared().map_err(failed)?;
            let launched = worlds::make(world, folder, front).map_err(failed)?;
            Ok(launched.model(listener, local_time))
        }
        #[cfg(not(feature = "fixture"))]
        {
            let _ = (listener, local_time);
            match world {}
        }
    }

    /// The model of the fixture called `name`, for a test of an app's bindings, over the app's
    /// own platform as `new` takes it: it asks `other_apps` about other apps and posts through
    /// `notifier`, in place of the fixture's own apps and posting nothing. The world still
    /// supplies the machine, and its process list follows what `other_apps` says runs, so in
    /// `chatGPTOpen` the core sees ChatGPT running Codex's login until `other_apps` says it
    /// has quit. With no `other_apps`, the app names none, and ChatGPT runs until the end. The
    /// machine is the folder `pitboard-fixture` in `temporary_directory`, emptied and made
    /// again as `fixture` makes its own, and left there, so a test empties no debug build's
    /// fixture, nor that of another run of the same tests. An app launching into a fixture
    /// calls `fixture`. Refused as `fixture` is.
    #[uniffi::constructor]
    pub fn fixture_in(
        name: String,
        front: Front,
        temporary_directory: String,
        listener: Arc<dyn Listener>,
        other_apps: Option<Arc<dyn OtherApps>>,
        notifier: Arc<dyn Notifier>,
        local_time: Arc<dyn LocalTime>,
    ) -> Result<Arc<Self>, FixtureError> {
        let world = launchable(front, &name)?;
        #[cfg(feature = "fixture")]
        {
            let folder = worlds::Folder::within(std::path::Path::new(&temporary_directory))
                .map_err(failed)?;
            let launched = worlds::make(world, folder, front).map_err(failed)?;
            let other_apps = other_apps.map(|apps| launched.apps.following(apps));
            Ok(launched.model_over(listener, other_apps, notifier, local_time))
        }
        #[cfg(not(feature = "fixture"))]
        {
            let _ = (
                temporary_directory,
                listener,
                other_apps,
                notifier,
                local_time,
            );
            match world {}
        }
    }
}

/// Why the fixture called `name` cannot be launched for the app `front` names, asked before
/// anything is made: a name that is none of `fixture_names`, or a library built without the
/// `fixture` feature, each refused as `fixture` and `fixture_in` refuse them. `None` for a
/// fixture they would make. Plain Rust, as the Linux app alone asks it: Swift and C# make the
/// model at once.
pub fn fixture_refusal(front: Front, name: &str) -> Option<FixtureError> {
    launchable(front, name).err()
}

/// The world called `name`, which `fixture` and `fixture_in` make.
#[cfg(feature = "fixture")]
fn launchable(_front: Front, name: &str) -> Result<worlds::World, FixtureError> {
    worlds::World::named(name)
}

/// None, in a library built without the `fixture` feature.
#[cfg(not(feature = "fixture"))]
fn launchable(front: Front, _name: &str) -> Result<std::convert::Infallible, FixtureError> {
    Err(unavailable(front))
}

/// What a build without the `fixture` feature answers the app `front` names: the library the
/// shipping apps link, or the Linux app, which links the model itself.
#[cfg(not(feature = "fixture"))]
fn unavailable(front: Front) -> FixtureError {
    match front {
        Front::MacMenuBar | Front::WindowsNotificationArea => library_unavailable(),
        Front::Gnome => FixtureError::Unavailable {
            reason: "This build of the Linux app was built without the `fixture` feature, so it \
                     has no fixtures. Build it with `cargo build -p pitboard-linux --features \
                     fixture`."
                .into(),
        },
    }
}

/// What a library built without the `fixture` feature answers.
#[cfg(not(feature = "fixture"))]
fn library_unavailable() -> FixtureError {
    FixtureError::Unavailable {
        reason: "This library was built without the `fixture` feature, so it has no fixtures. \
                 Build it with `cargo build -p pitboard-ffi --features fixture`, or the macOS \
                 app's with `apps/macos/scripts/build-xcframework.sh --fixture`."
            .into(),
    }
}

/// A world that could not be made, said with why.
#[cfg(feature = "fixture")]
fn failed(error: impl std::fmt::Display) -> FixtureError {
    FixtureError::Failed {
        reason: format!("The fixture could not be made: {error}"),
    }
}

/// What a library built without the `fixture` feature exports of the fixtures, as every
/// library an app ships is built.
#[cfg(all(test, not(feature = "fixture")))]
mod without {
    use super::*;
    use crate::model::testing::platform::{Posted, StandInApps};
    use crate::model::{PlatformError, Snapshot};

    struct Unheard;

    impl Listener for Unheard {
        fn changed(&self, _snapshot: Snapshot) -> Result<(), PlatformError> {
            Ok(())
        }
    }

    /// There is no fixture to name, none to launch, and no page to serve, and each refusal
    /// names the feature that has them, so somebody launching a debug build against a
    /// library built without it is told what to build.
    #[test]
    fn a_library_without_the_feature_has_no_fixture_and_says_which_feature_has() {
        assert!(fixture_names().is_empty());
        let refused = PitboardModel::fixture(
            "twoTools".into(),
            Front::MacMenuBar,
            Arc::new(Unheard),
            Arc::new(crate::present::testing::Utc),
        );
        let Err(FixtureError::Unavailable { reason }) = refused else {
            panic!("{:?}", refused.map(|_| ()));
        };
        assert!(reason.contains("`fixture` feature"), "{reason}");
        assert!(reason.contains("--features fixture"), "{reason}");
        assert!(
            reason.contains("build-xcframework.sh --fixture"),
            "{reason}"
        );
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let directory = scratch.path().join("fixture");
        let refused_in = PitboardModel::fixture_in(
            "twoTools".into(),
            Front::WindowsNotificationArea,
            directory.to_string_lossy().into_owned(),
            Arc::new(Unheard),
            Some(StandInApps::new(&[], true)),
            Arc::new(Posted::default()),
            Arc::new(crate::present::testing::Utc),
        );
        assert!(
            matches!(refused_in, Err(FixtureError::Unavailable { .. })),
            "{:?}",
            refused_in.map(|_| ())
        );
        assert!(!directory.exists(), "nothing made where it was asked");
        assert_eq!(
            fixture_page("pitboard-fixture://claude.ai/".into()),
            Err(library_unavailable())
        );
    }

    /// The Linux app asks before it makes anything, and is told the command that builds its
    /// own debug build with the fixtures, not the library's.
    #[test]
    fn the_linux_app_is_told_how_to_build_itself_with_the_fixtures() {
        let Some(FixtureError::Unavailable { reason }) = fixture_refusal(Front::Gnome, "twoTools")
        else {
            panic!("a build without the feature launches no fixture");
        };
        assert!(reason.contains("`fixture` feature"), "{reason}");
        assert!(
            reason.contains("`cargo build -p pitboard-linux --features fixture`"),
            "{reason}"
        );
        assert!(!reason.contains("pitboard-ffi"), "{reason}");
        for front in [Front::MacMenuBar, Front::WindowsNotificationArea] {
            assert_eq!(
                fixture_refusal(front, "twoTools"),
                Some(library_unavailable()),
                "{front:?}"
            );
        }
    }
}
