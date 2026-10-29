//! The app's own folder, never in Pitboard's directory, so `pitboard uninstall` leaves it and
//! Uninstall in the app takes it back (`platform::leftovers`): `$XDG_DATA_HOME/<app id>`, each
//! build its own, as the desktop's apps keep theirs. What the window remembers is kept there,
//! and the account windows' stores will be. In a fixture it is the stand-ins' folder in the
//! fixture's, so a fixture writes nothing outside the fixture's folder.

use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};

use crate::config;
use crate::platform::Failed;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataFolder(PathBuf);

impl DataFolder {
    /// This person's, under this build's id. GLib reads `XDG_DATA_HOME` once for the process.
    pub fn of_this_user() -> DataFolder {
        DataFolder(glib::user_data_dir().join(config::APP_ID))
    }

    /// The fixture's, in `fixture`, the folder the fixture's world is in.
    #[cfg(feature = "fixture")]
    pub fn in_fixture(fixture: &Path) -> DataFolder {
        DataFolder(fixture.join(crate::platform::stand_in::FOLDER))
    }

    /// At `path`, for a test's own.
    pub fn at(path: &Path) -> DataFolder {
        DataFolder(path.to_path_buf())
    }

    pub fn path(&self) -> &Path {
        &self.0
    }

    /// Where the file `name` is kept, which reading it makes nothing for.
    pub fn file(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }

    /// The file `name`, to be written, in the folder made first where it is not there: 0700, as
    /// XDG Base Directory 0.8 has a folder of the person's data made.
    pub fn file_to_write(&self, name: &str) -> Result<PathBuf, Failed> {
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&self.0)
            .map_err(|e| Failed(format!("{}: {e}", self.0.display())))?;
        Ok(self.0.join(name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn a_file_is_kept_in_a_folder_made_for_the_person_alone_once_it_is_written() {
        let scratch = tempfile::tempdir().unwrap();
        let folder = DataFolder::at(&scratch.path().join("data").join(config::APP_ID));
        assert_eq!(
            folder.file("window.json"),
            folder.path().join("window.json")
        );
        assert!(!folder.path().exists(), "made to read a file");

        let file = folder.file_to_write("window.json").unwrap();

        assert_eq!(file, folder.path().join("window.json"));
        let mode = std::fs::metadata(folder.path())
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o700);
        assert!(!file.exists(), "made by its writer");
    }

    #[cfg(feature = "fixture")]
    #[test]
    fn a_fixtures_folder_is_the_one_its_stand_ins_write_in() {
        let fixture = Path::new("/tmp/scratch/pitboard-fixture");

        assert_eq!(
            DataFolder::in_fixture(fixture).path(),
            fixture.join(crate::platform::stand_in::FOLDER)
        );
    }
}
