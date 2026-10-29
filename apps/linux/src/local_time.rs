//! The person's clock for the model: the time zone and the locale GLib reads, and 12 or 24 hours
//! as GNOME's `org.gnome.desktop.interface` `clock-format` says, where that schema is installed,
//! read as GSettings reads it, which starts no `dconf-service` (docs/facts/linux-app.md, The
//! clock). Elsewhere the clock is 24 hours, the schema's own default.
//!
//! The model asks on its own threads, as each snapshot is made, and is answered at once:
//! `glib::DateTime` needs no main loop, and the choice is an atomic the main loop sets as the
//! desktop changes it.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use gio::prelude::*;
use pitboard_model::PlatformError;

const SCHEMA: &str = "org.gnome.desktop.interface";
const KEY: &str = "clock-format";
const TWELVE_HOURS: &str = "12h";

/// The desktop's choice of 12 or 24 hours, followed on the main context until dropped.
pub struct DesktopClock {
    twelve_hours: Arc<AtomicBool>,
    _followed: Option<(gio::Settings, glib::SignalHandlerId)>,
}

impl DesktopClock {
    /// The choice `schemas` hold, read through `backend`: the system's own schemas and GSettings'
    /// default backend, in the app.
    pub fn following(
        schemas: Option<&gio::SettingsSchemaSource>,
        backend: Option<&gio::SettingsBackend>,
    ) -> DesktopClock {
        let twelve_hours = Arc::new(AtomicBool::new(false));
        let followed = schemas
            .and_then(|schemas| schemas.lookup(SCHEMA, true))
            .filter(|schema| schema.has_key(KEY))
            .map(|schema| {
                let settings = gio::Settings::new_full(&schema, backend, None);
                let set = |settings: &gio::Settings, twelve_hours: &AtomicBool| {
                    twelve_hours.store(settings.string(KEY) == TWELVE_HOURS, Ordering::Relaxed);
                };
                set(&settings, &twelve_hours);
                let handler = settings.connect_changed(Some(KEY), {
                    let twelve_hours = Arc::clone(&twelve_hours);
                    move |settings, _| set(settings, &twelve_hours)
                });
                (settings, handler)
            });
        DesktopClock {
            twelve_hours,
            _followed: followed,
        }
    }

    pub fn local_time(&self) -> Arc<LocalTime> {
        Arc::new(LocalTime {
            twelve_hours: Arc::clone(&self.twelve_hours),
        })
    }
}

/// What the model asks of the person's clock.
pub struct LocalTime {
    twelve_hours: Arc<AtomicBool>,
}

impl LocalTime {
    // GNOME's own English forms: "14:05" and "2:05 PM", the hour unpadded.
    fn time(&self) -> &'static str {
        if self.twelve_hours.load(Ordering::Relaxed) {
            "%-l:%M %p"
        } else {
            "%H:%M"
        }
    }
}

fn local(epoch: i64) -> Result<glib::DateTime, PlatformError> {
    glib::DateTime::from_unix_local(epoch).map_err(failed)
}

fn formatted(at: &glib::DateTime, format: &str) -> Result<String, PlatformError> {
    at.format(format).map(String::from).map_err(failed)
}

fn failed(error: impl std::fmt::Display) -> PlatformError {
    PlatformError::Failed {
        reason: error.to_string(),
    }
}

impl pitboard_model::LocalTime for LocalTime {
    fn clock(&self, epoch: i64, with_weekday: bool) -> Result<String, PlatformError> {
        let time = self.time();
        let format = if with_weekday {
            format!("%a {time}")
        } else {
            time.to_owned()
        };
        formatted(&local(epoch)?, &format)
    }

    fn same_day(&self, first: i64, second: i64) -> Result<bool, PlatformError> {
        Ok(local(first)?.ymd() == local(second)?.ymd())
    }

    fn date_and_time(&self, epoch: i64) -> Result<String, PlatformError> {
        formatted(&local(epoch)?, &format!("%b %-e, %Y, {}", self.time()))
    }

    fn time_of_day(&self, minutes: u16) -> Result<String, PlatformError> {
        let (hour, minute) = (i32::from(minutes / 60), i32::from(minutes % 60));
        let at = glib::DateTime::new(&glib::TimeZone::utc(), 2000, 1, 1, hour, minute, 0.0)
            .map_err(failed)?;
        formatted(&at, self.time())
    }

    fn date(&self, epoch: i64) -> Result<String, PlatformError> {
        formatted(&local(epoch)?, "%b %-e")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pitboard_model::LocalTime as _;

    // 2026-11-20 14:05:00 UTC, a Friday.
    const AT: i64 = 1_795_183_500;

    fn in_utc(twelve_hours: bool) -> LocalTime {
        LocalTime {
            twelve_hours: Arc::new(AtomicBool::new(twelve_hours)),
        }
    }

    // The desktop's interface settings in a backend of the test's own, so a run on a desktop
    // writes nothing through its dconf.
    fn interface(backend: &gio::SettingsBackend) -> gio::Settings {
        let schema = gio::SettingsSchemaSource::default()
            .and_then(|schemas| schemas.lookup(SCHEMA, true))
            .expect("every image installs GNOME's desktop schemas");
        gio::Settings::new_full(&schema, Some(backend), None)
    }

    fn iterate() {
        while glib::MainContext::default().iteration(false) {}
    }

    #[test]
    fn the_clock_follows_the_desktops_choice() {
        let backend = gio::memory_settings_backend_new();
        let desktop = interface(&backend);
        desktop.set_string(KEY, "12h").unwrap();
        let clock = DesktopClock::following(
            gio::SettingsSchemaSource::default().as_ref(),
            Some(&backend),
        );
        let local_time = clock.local_time();
        let noon = |time: &LocalTime| time.time_of_day(12 * 60 + 5).unwrap();
        assert_eq!(noon(&local_time), "12:05 PM");

        desktop.set_string(KEY, "24h").unwrap();
        iterate();
        assert_eq!(noon(&local_time), "12:05");
        assert_eq!(local_time.time_of_day(6 * 60).unwrap(), "06:00");

        desktop.set_string(KEY, "12h").unwrap();
        iterate();
        assert_eq!(local_time.time_of_day(6 * 60).unwrap(), "6:00 AM");

        drop(clock);
        desktop.set_string(KEY, "24h").unwrap();
        iterate();
        assert_eq!(
            noon(&local_time),
            "12:05 PM",
            "followed only while the clock is kept"
        );
    }

    #[test]
    fn without_the_desktops_schema_the_clock_is_twenty_four_hours() {
        let clock = DesktopClock::following(None, None);

        assert_eq!(
            clock.local_time().time_of_day(18 * 60 + 30).unwrap(),
            "18:30"
        );
    }

    #[test]
    fn a_moment_is_said_as_gnome_says_it() {
        let (twelve, twenty_four) = (in_utc(true), in_utc(false));
        // headless.sh's container keeps UTC, and the moment is said in the person's zone.
        let utc = glib::DateTime::from_unix_local(AT).unwrap().utc_offset();
        assert_eq!(utc.as_seconds(), 0, "the test's zone is UTC");

        assert_eq!(twenty_four.clock(AT, false).unwrap(), "14:05");
        assert_eq!(twelve.clock(AT, false).unwrap(), "2:05 PM");
        assert_eq!(twelve.clock(AT, true).unwrap(), "Fri 2:05 PM");
        assert_eq!(twenty_four.date(AT).unwrap(), "Nov 20");
        assert_eq!(twelve.date_and_time(AT).unwrap(), "Nov 20, 2026, 2:05 PM");
        assert!(twelve.same_day(AT, AT + 9 * 3600 + 54 * 60).unwrap());
        assert!(!twelve.same_day(AT, AT + 9 * 3600 + 55 * 60).unwrap());
    }

    // A snapshot is made while the main loop waits on the model, as it does while the app quits.
    #[test]
    fn local_time_answers_without_the_main_loop() {
        let backend = gio::memory_settings_backend_new();
        let clock = DesktopClock::following(
            gio::SettingsSchemaSource::default().as_ref(),
            Some(&backend),
        );
        let local_time = clock.local_time();

        let answered = std::thread::spawn(move || local_time.clock(AT, true))
            .join()
            .unwrap()
            .unwrap();

        assert!(
            answered.starts_with("Fri ") && answered.contains(":05"),
            "{answered}"
        );
    }
}
