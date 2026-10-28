//! Start at sign-in, as an XDG Autostart 0.5 entry in the person's own autostart folder,
//! `$XDG_CONFIG_HOME/autostart/<app id>.desktop`, written and read through `glib::KeyFile` as
//! the desktop reads it.
//!
//! The entry starts the program held with no window, with `--sign-in`, and only where the
//! program is still there (`TryExec`), so an entry a removed package left behind starts nothing.
//! It is the app's own only where it names this program: another copy's entry under the same
//! name reads as off, and turning it off leaves that entry where it is.

use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};

use glib::KeyFile;

use super::Failed;
use crate::config;

/// The flag the entry starts the program with, which holds it with no window.
pub const SIGN_IN: &str = "--sign-in";

const NAME: &str = "Pitboard";

/// Whether the app starts at sign-in: the desktop's autostart entry, or a fixture's stand-in.
pub trait StartAtSignIn {
    fn is_on(&self) -> bool;
    fn turn_on(&self) -> Result<(), Failed>;
    /// Whether an entry of the app's own went.
    fn turn_off(&self) -> Result<bool, Failed>;
}

/// The entry in an autostart folder for one installed program.
pub struct Autostart {
    entry: PathBuf,
    icon: String,
    program: String,
}

impl Autostart {
    pub fn new(folder: &Path, app_id: &str, program: &str) -> Autostart {
        Autostart {
            entry: folder.join(format!("{app_id}.desktop")),
            icon: app_id.to_owned(),
            program: program.to_owned(),
        }
    }

    /// In this person's autostart folder, under this build's id. GLib reads
    /// `XDG_CONFIG_HOME` once for the process.
    pub fn of_this_user(program: &str) -> Autostart {
        Autostart::new(
            &glib::user_config_dir().join("autostart"),
            config::APP_ID,
            program,
        )
    }

    pub fn entry(&self) -> &Path {
        &self.entry
    }

    fn exec(&self) -> String {
        format!("{} {SIGN_IN}", exec_argument(&self.program))
    }

    fn read(&self) -> Option<KeyFile> {
        let entry = KeyFile::new();
        entry
            .load_from_file(&self.entry, glib::KeyFileFlags::NONE)
            .ok()
            .map(|()| entry)
    }

    fn names_this_program(&self, entry: &KeyFile) -> bool {
        string(entry, glib::KEY_FILE_DESKTOP_KEY_TRY_EXEC)
            .is_some_and(|tried| tried == self.program)
    }

    fn failed(&self, error: impl std::fmt::Display) -> Failed {
        Failed(format!("{}: {error}", self.entry.display()))
    }
}

impl StartAtSignIn for Autostart {
    fn is_on(&self) -> bool {
        self.read().is_some_and(|entry| {
            self.names_this_program(&entry)
                && string(&entry, glib::KEY_FILE_DESKTOP_KEY_EXEC)
                    .is_some_and(|exec| exec == self.exec())
                && !hidden(&entry)
        })
    }

    fn turn_on(&self) -> Result<(), Failed> {
        // XDG Base Directory 0.8: a folder not there yet is made 0700.
        if let Some(folder) = self.entry.parent() {
            std::fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(folder)
                .map_err(|e| Failed(format!("{}: {e}", folder.display())))?;
        }
        let entry = KeyFile::new();
        let exec = self.exec();
        for (key, value) in [
            (
                glib::KEY_FILE_DESKTOP_KEY_TYPE,
                glib::KEY_FILE_DESKTOP_TYPE_APPLICATION.as_str(),
            ),
            (glib::KEY_FILE_DESKTOP_KEY_NAME, NAME),
            (glib::KEY_FILE_DESKTOP_KEY_ICON, self.icon.as_str()),
            (glib::KEY_FILE_DESKTOP_KEY_EXEC, exec.as_str()),
            (glib::KEY_FILE_DESKTOP_KEY_TRY_EXEC, self.program.as_str()),
        ] {
            entry.set_string(glib::KEY_FILE_DESKTOP_GROUP, key, value);
        }
        entry.save_to_file(&self.entry).map_err(|e| self.failed(e))
    }

    fn turn_off(&self) -> Result<bool, Failed> {
        if !self
            .read()
            .is_some_and(|entry| self.names_this_program(&entry))
        {
            return Ok(false);
        }
        std::fs::remove_file(&self.entry)
            .map(|()| true)
            .map_err(|e| self.failed(e))
    }
}

fn string(entry: &KeyFile, key: &str) -> Option<String> {
    entry
        .string(glib::KEY_FILE_DESKTOP_GROUP, key)
        .ok()
        .map(String::from)
}

// A Hidden that does not read as a boolean hides the entry, as systemd's autostart generator
// hides an entry it cannot read (docs/facts/linux-app.md, Start at sign-in).
fn hidden(entry: &KeyFile) -> bool {
    entry
        .boolean(
            glib::KEY_FILE_DESKTOP_GROUP,
            glib::KEY_FILE_DESKTOP_KEY_HIDDEN,
        )
        .unwrap_or_else(|e| !e.matches(glib::KeyFileError::KeyNotFound))
}

// The Desktop Entry Specification 1.5's quoting of an Exec argument (The Exec key): one that
// holds a reserved character in double quotes, with `"`, `` ` ``, `$` and `\` escaped, and each
// `%` doubled, since a lone one starts a field code.
fn exec_argument(argument: &str) -> String {
    const RESERVED: &[char] = &[
        ' ', '\t', '\n', '"', '\'', '\\', '>', '<', '~', '|', '&', ';', '$', '*', '?', '#', '(',
        ')', '`',
    ];
    let argument = argument.replace('%', "%%");
    if !argument.contains(RESERVED) {
        return argument;
    }
    let mut quoted = String::from('"');
    for c in argument.chars() {
        if matches!(c, '"' | '`' | '$' | '\\') {
            quoted.push('\\');
        }
        quoted.push(c);
    }
    quoted.push('"');
    quoted
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    const ID: &str = "com.usepitboard.Pitboard";
    const GROUP: &str = "Desktop Entry";

    fn read(path: &Path) -> KeyFile {
        let entry = KeyFile::new();
        entry
            .load_from_file(path, glib::KeyFileFlags::NONE)
            .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        entry
    }

    fn key(entry: &KeyFile, key: &str) -> Option<String> {
        entry.string(GROUP, key).ok().map(String::from)
    }

    // An entry as Autostart 0.5 has one, written by hand.
    fn write_entry(path: &Path, lines: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            path,
            format!("[{GROUP}]\nType=Application\nName=Pitboard\n{lines}"),
        )
        .unwrap();
    }

    #[test]
    fn turning_it_on_writes_an_entry_that_tries_its_program_and_starts_it_held() {
        let scratch = tempfile::tempdir().unwrap();
        // Not there yet: the first entry makes the folder.
        let folder = scratch.path().join("config/autostart");
        let program = scratch.path().join("bin/pitboard-app");
        let program = program.to_str().unwrap();
        let autostart = Autostart::new(&folder, ID, program);
        assert!(!autostart.is_on());

        autostart.turn_on().unwrap();

        assert_eq!(autostart.entry(), folder.join(format!("{ID}.desktop")));
        assert!(autostart.is_on());
        let entry = read(autostart.entry());
        assert_eq!(key(&entry, "Type").as_deref(), Some("Application"));
        assert_eq!(key(&entry, "Name").as_deref(), Some("Pitboard"));
        assert_eq!(key(&entry, "Icon").as_deref(), Some(ID));
        assert_eq!(key(&entry, "Exec"), Some(format!("{program} {SIGN_IN}")));
        assert_eq!(key(&entry, "TryExec").as_deref(), Some(program));
        assert_eq!(key(&entry, "Hidden"), None);
        // XDG Base Directory 0.8: a folder not there yet is made 0700.
        let mode = std::fs::metadata(&folder).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o700);
    }

    // The Desktop Entry Specification 1.5, The Exec key: such an argument in double quotes,
    // with `"`, `` ` ``, `$` and `\` escaped, and `%` doubled as a field code's escape.
    #[test]
    fn a_program_whose_path_holds_a_reserved_character_is_quoted_in_exec() {
        let scratch = tempfile::tempdir().unwrap();
        let folder = scratch.path().join("autostart");
        let program = "/opt/My \"Apps\"/$HOME`s 100%/pitboard-app";
        let autostart = Autostart::new(&folder, ID, program);

        autostart.turn_on().unwrap();

        let entry = read(autostart.entry());
        assert_eq!(
            key(&entry, "Exec").as_deref(),
            Some(r#""/opt/My \"Apps\"/\$HOME\`s 100%%/pitboard-app" --sign-in"#)
        );
        assert_eq!(key(&entry, "TryExec").as_deref(), Some(program));
        assert!(autostart.is_on());
    }

    // Another copy of the app, installed elsewhere, turned it on after this one did.
    #[test]
    fn an_entry_another_program_wrote_reads_as_off_and_is_left_alone() {
        let scratch = tempfile::tempdir().unwrap();
        let folder = scratch.path().join("autostart");
        let ours = Autostart::new(&folder, ID, "/usr/bin/pitboard-app");
        let theirs = Autostart::new(&folder, ID, "/usr/local/bin/pitboard-app");
        ours.turn_on().unwrap();
        theirs.turn_on().unwrap();
        let before = std::fs::read(theirs.entry()).unwrap();

        assert!(!ours.is_on());
        assert!(!ours.turn_off().unwrap());

        assert_eq!(std::fs::read(theirs.entry()).unwrap(), before);
        assert!(theirs.is_on());
    }

    #[test]
    fn a_hidden_entry_reads_as_off() {
        let scratch = tempfile::tempdir().unwrap();
        let folder = scratch.path().join("autostart");
        let program = "/usr/bin/pitboard-app";
        let autostart = Autostart::new(&folder, ID, program);
        let hidden = format!("TryExec={program}\nExec={program} --sign-in\nHidden=true\n");
        write_entry(autostart.entry(), &hidden);

        assert!(!autostart.is_on());

        autostart.turn_on().unwrap();
        assert!(autostart.is_on());
        assert_eq!(key(&read(autostart.entry()), "Hidden"), None);
    }

    #[test]
    fn turning_it_off_removes_only_its_own_entry() {
        let scratch = tempfile::tempdir().unwrap();
        let folder = scratch.path().join("autostart");
        let ours = Autostart::new(&folder, ID, "/usr/bin/pitboard-app");
        let devel = Autostart::new(&folder, &format!("{ID}.Devel"), "/usr/bin/pitboard-app");
        let other = folder.join("org.example.Other.desktop");
        ours.turn_on().unwrap();
        devel.turn_on().unwrap();
        write_entry(&other, "Exec=/usr/bin/other\n");

        assert!(ours.turn_off().unwrap());

        assert!(!ours.is_on());
        assert!(!ours.entry().exists());
        assert!(devel.is_on());
        assert!(other.exists());
        assert!(!ours.turn_off().unwrap(), "a second turn off finds nothing");
    }
}
