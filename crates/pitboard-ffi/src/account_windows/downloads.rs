//! What a download is called, where it comes from, and what asking before one says.

use super::AlertText;
use pitboard_core::host::{OS, Os};
use pitboard_sites::WebAddress;
use std::path::{Path, PathBuf};
use unicode_normalization::UnicodeNormalization;

/// The file a download suggested as `suggested` is saved as in `folder`: its name, then that
/// name numbered as the system numbers a copy, while a name is one of `reserved`, given to a
/// download still running, or a file already in `folder`. Looks at the file system for each
/// name it tries.
///
/// The web view wants a file that does not exist yet, and two downloads can ask at once. It
/// has already made the name safe: measured on macOS 27, WebKit turns `../../evil:name.txt`
/// into `_.._evil_name.txt`.
///
/// A path in `reserved` may come back in another Unicode form than it was given in: the macOS
/// app keeps each as a `URL`, whose `path` Foundation gives decomposed, measured on macOS 27.0.
#[uniffi::export]
pub fn download_destination(folder: String, suggested: String, reserved: Vec<String>) -> String {
    let taken = |candidate: &Path| {
        reserved
            .iter()
            .any(|path| same_file(OS, Path::new(path), candidate))
            || candidate.exists()
    };
    destination(OS, Path::new(&folder), &suggested, taken)
        .to_string_lossy()
        .into_owned()
}

/// Where a download suggested as `suggested` goes in `folder` on `os`: the first name
/// `taken` does not say is taken.
fn destination(os: Os, folder: &Path, suggested: &str, taken: impl Fn(&Path) -> bool) -> PathBuf {
    let name = match last_component(suggested) {
        "" | "/" => "Download",
        name => name,
    };
    let (base, extension) = split_extension(name);
    (1u64..)
        .map(|nth| folder.join(numbered(os, name, base, extension, nth)))
        .find(|candidate| !taken(candidate))
        .unwrap_or_else(|| folder.join(name))
}

/// The `nth` file named `name` in one folder, `base` and `extension` being its parts: the
/// name itself first, then the name as `os` numbers a copy of a file.
fn numbered(os: Os, name: &str, base: &str, extension: Option<&str>, nth: u64) -> String {
    if nth == 1 {
        return name.to_owned();
    }
    match os {
        // As Finder numbers one: `report 2.pdf`, then `report 3.pdf`. No app runs on Linux,
        // and it numbers as the Mac does.
        Os::MacOs | Os::Linux => match extension {
            Some(extension) => format!("{base} {nth}.{extension}"),
            None => format!("{base} {nth}"),
        },
        // As Explorer numbers one, as the owner decided: `report (1).pdf`, then
        // `report (2).pdf`. The name is split as everywhere else here; how WebView2 and
        // Explorer split a name with two extensions, such as `archive.tar.gz`, is the
        // Windows app's to measure.
        Os::Windows => {
            let copy = nth - 1;
            match extension {
                Some(extension) => format!("{base} ({copy}).{extension}"),
                None => format!("{base} ({copy})"),
            }
        }
    }
}

/// Whether `os` takes the paths `one` and `other` for one file. A Mac's file system takes
/// names that are canonically equivalent in Unicode for one, measured on APFS on macOS 27.0,
/// so they are compared decomposed. No app runs on Linux, which compares them as the Mac
/// does; where its file system takes them for two, a name is numbered that need not be.
/// Windows takes names in any case for one, and does not normalise them, so two forms of one
/// name are two files there ([`pitboard_core::host::same_path_in_any_case`]).
fn same_file(os: Os, one: &Path, other: &Path) -> bool {
    match os {
        Os::MacOs | Os::Linux => {
            let decomposed =
                |path: &Path| PathBuf::from(path.to_string_lossy().nfd().collect::<String>());
            decomposed(one) == decomposed(other)
        }
        Os::Windows => pitboard_core::host::same_path_in_any_case(one, other),
    }
}

/// The last part of `path` as `NSString.lastPathComponent` reads it: slashes at the end are
/// dropped, and a path of slashes alone is `/`. Measured on macOS 27.
fn last_component(path: &str) -> &str {
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() {
        return if path.is_empty() { "" } else { "/" };
    }
    trimmed.rsplit('/').next().unwrap_or(trimmed)
}

/// `name` as `NSString` splits a file name into its base and its extension, measured on
/// macOS 27: the extension follows the last `.`, and is none where it would be empty or hold
/// a space, or where the base would be empty, `.` or `..`, as in `.hidden` or `..a`.
fn split_extension(name: &str) -> (&str, Option<&str>) {
    match name.rsplit_once('.') {
        Some((base, extension))
            if !extension.is_empty()
                && !extension.contains(' ')
                && !matches!(base, "" | "." | "..") =>
        {
            (base, Some(extension))
        }
        _ => (name, None),
    }
}

/// The host a download comes from, as asking about it names it, from its address: the host
/// of a `blob:` link is the one inside it. The app asks the frame that started a download
/// first, where its web view says.
#[uniffi::export]
pub fn download_host(url: Option<String>) -> Option<String> {
    let url = url?;
    let address = WebAddress::parse(&url);
    if address.scheme() == "blob" {
        return WebAddress::parse(url.get("blob:".len()..)?)
            .host()
            .map(str::to_owned);
    }
    address.host().map(str::to_owned)
}

/// What asking before a download of `name` from `host` says, when a page inside the site
/// rather than the site's own page started it.
#[uniffi::export]
pub fn download_question(name: String, host: Option<String>) -> AlertText {
    AlertText {
        title: match host.filter(|host| !host.is_empty()) {
            Some(host) => format!("Download “{name}” from {host}?"),
            None => format!("Download “{name}”?"),
        },
        message: "A page inside the site, such as an artifact, asked to save this file in your \
                  Downloads folder."
            .into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn on_a_mac(suggested: &str, taken: &[&str]) -> String {
        let taken: HashSet<String> = taken.iter().map(|name| downloads(name)).collect();
        destination(Os::MacOs, Path::new("/Downloads"), suggested, |candidate| {
            taken.contains(candidate.to_str().expect("UTF-8"))
        })
        .to_string_lossy()
        .into_owned()
    }

    fn downloads(name: &str) -> String {
        Path::new("/Downloads")
            .join(name)
            .to_string_lossy()
            .into_owned()
    }

    /// A download never overwrites a file, and two downloads never get one name. From
    /// AccountWindowsTests.swift.
    #[test]
    fn a_download_is_numbered_past_names_taken() {
        assert_eq!(
            on_a_mac("report.pdf", &["report.pdf", "report 2.pdf"]),
            downloads("report 3.pdf")
        );
        assert_eq!(on_a_mac("notes", &[]), downloads("notes"));
        assert_eq!(on_a_mac("", &[]), downloads("Download"));
        assert_eq!(on_a_mac("a/b.txt", &[]), downloads("b.txt"));
    }

    /// Each name as the Swift `DownloadCenter.destination` named it, free and with its own
    /// name taken, measured on macOS 27 with `NSString`'s path rules.
    #[test]
    fn a_name_is_split_and_numbered_as_finder_numbers_one() {
        for (suggested, free, second) in [
            ("report.pdf", "report.pdf", "report 2.pdf"),
            ("/", "Download", "Download 2"),
            ("//", "Download", "Download 2"),
            ("a/", "a", "a 2"),
            ("a//", "a", "a 2"),
            ("/a/b/", "b", "b 2"),
            ("///x", "x", "x 2"),
            (".hidden", ".hidden", ".hidden 2"),
            (".hidden.txt", ".hidden.txt", ".hidden 2.txt"),
            ("archive.tar.gz", "archive.tar.gz", "archive.tar 2.gz"),
            ("name.", "name.", "name. 2"),
            ("..", "..", ".. 2"),
            ("a.b c", "a.b c", "a.b c 2"),
            ("a b.c", "a b.c", "a b 2.c"),
            ("a.b\u{a0}c", "a.b\u{a0}c", "a 2.b\u{a0}c"),
            ("a.b\tc", "a.b\tc", "a 2.b\tc"),
            ("my file.PDF", "my file.PDF", "my file 2.PDF"),
            ("a .txt", "a .txt", "a  2.txt"),
            ("..a", "..a", "..a 2"),
            ("...a", "...a", "...a 2"),
            ("....a", "....a", "... 2.a"),
            ("..a.b", "..a.b", "..a 2.b"),
            ("a...b", "a...b", "a.. 2.b"),
            (". .b", ". .b", ".  2.b"),
            ("x.\u{301}", "x.\u{301}", "x 2.\u{301}"),
            ("a.😀", "a.😀", "a 2.😀"),
            ("~/x.txt", "x.txt", "x 2.txt"),
            ("a\\b.txt", "a\\b.txt", "a\\b 2.txt"),
        ] {
            assert_eq!(on_a_mac(suggested, &[]), downloads(free), "{suggested:?}");
            assert_eq!(
                on_a_mac(suggested, &[free]),
                downloads(second),
                "{suggested:?}"
            );
        }
    }

    /// No app runs on Linux, which numbers a copy as the Mac does.
    #[test]
    fn linux_numbers_a_copy_as_the_mac_does() {
        let name = destination(Os::Linux, Path::new("/d"), "report.pdf", |candidate| {
            candidate == Path::new("/d/report.pdf")
        });
        assert_eq!(name, Path::new("/d/report 2.pdf"));
    }

    /// Windows numbers a copy as Explorer does, as the owner decided: in brackets, from one,
    /// so the second `report.pdf` is `report (1).pdf`.
    #[test]
    fn windows_numbers_a_copy_in_brackets_from_one() {
        let on_windows = |suggested: &str, taken: &[&str]| {
            destination(Os::Windows, Path::new("/d"), suggested, |candidate| {
                taken
                    .iter()
                    .any(|name| Path::new("/d").join(name) == candidate)
            })
        };
        assert_eq!(on_windows("report.pdf", &[]), Path::new("/d/report.pdf"));
        assert_eq!(
            on_windows("report.pdf", &["report.pdf"]),
            Path::new("/d/report (1).pdf")
        );
        assert_eq!(
            on_windows("report.pdf", &["report.pdf", "report (1).pdf"]),
            Path::new("/d/report (2).pdf")
        );
        assert_eq!(on_windows("notes", &["notes"]), Path::new("/d/notes (1)"));
        assert_eq!(on_windows("", &["Download"]), Path::new("/d/Download (1)"));
    }

    /// Windows takes names in any case for one file, so `Report.PDF` and `report.pdf` are one,
    /// and a name in two Unicode forms for two, since it compares a name without normalising
    /// it.
    #[test]
    fn windows_takes_names_in_any_case_for_one_and_in_two_forms_for_two() {
        let same =
            |one: &str, other: &str| same_file(Os::Windows, Path::new(one), Path::new(other));
        assert!(same("/d/Report.PDF", "/d/report.pdf"));
        assert!(same("/D//REPORT.pdf", "/d/report.pdf"));
        assert!(!same(
            "/d/B\u{e1}o c\u{e1}o.pdf",
            "/d/Ba\u{301}o ca\u{301}o.pdf"
        ));
        assert!(!same("/d/report.pdf", "/d/report (1).pdf"));
        let name = destination(Os::Windows, Path::new("/d"), "report.pdf", |candidate| {
            same_file(Os::Windows, Path::new("/d/Report.PDF"), candidate)
        });
        assert_eq!(name, Path::new("/d/report (1).pdf"));
    }

    /// The export looks at the folder as well as the names reserved for downloads running.
    #[test]
    fn a_file_already_there_or_a_name_reserved_is_taken() {
        let folder = std::env::temp_dir().join(format!(
            "pitboard-download-destination-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&folder).expect("a scratch folder");
        std::fs::write(folder.join("report.pdf"), b"").expect("a file");
        let reserved = folder.join("report 2.pdf").to_string_lossy().into_owned();
        let chosen = download_destination(
            folder.to_string_lossy().into_owned(),
            "report.pdf".into(),
            vec![reserved],
        );
        let expected = destination(OS, &folder, "report.pdf", |candidate| {
            candidate == folder.join("report.pdf") || candidate == folder.join("report 2.pdf")
        });
        assert_eq!(chosen, expected.to_string_lossy());
        assert_ne!(Path::new(&chosen), folder.join("report.pdf"));
        std::fs::remove_dir_all(&folder).expect("removed");
    }

    /// A name given to a download still running is taken whatever Unicode form the app hands
    /// it back in. WebKit suggests `Báo cáo.pdf` precomposed, and the app keeps the path it
    /// is given as a `URL`, whose `path` Foundation gives decomposed, measured on macOS 27.0.
    #[test]
    fn a_name_reserved_in_another_unicode_form_is_taken() {
        let folder = std::env::temp_dir().join(format!(
            "pitboard-download-reserved-{}-not-made",
            std::process::id()
        ));
        let at = |name: &str| folder.join(name).to_string_lossy().into_owned();
        let choose = |suggested: &str, reserved: &[&str]| {
            download_destination(
                folder.to_string_lossy().into_owned(),
                suggested.into(),
                reserved.iter().map(|path| (*path).to_owned()).collect(),
            )
        };
        let precomposed = "B\u{e1}o c\u{e1}o.pdf";
        let decomposed = "Ba\u{301}o ca\u{301}o.pdf";
        let first = choose(precomposed, &[]);
        assert_eq!(first, at(precomposed));
        // On Windows the other Unicode form is another name, so it is free (`same_file`).
        let copy = |name: &str, of: &str| match OS {
            Os::MacOs | Os::Linux => at(name),
            Os::Windows => at(of),
        };
        assert_eq!(
            choose(precomposed, &[&at(decomposed)]),
            copy("B\u{e1}o c\u{e1}o 2.pdf", precomposed),
            "reserved as Foundation gives its path"
        );
        assert_eq!(
            choose(precomposed, &[&first]),
            match OS {
                Os::MacOs | Os::Linux => at("B\u{e1}o c\u{e1}o 2.pdf"),
                Os::Windows => at("B\u{e1}o c\u{e1}o (1).pdf"),
            }
        );
        assert_eq!(
            choose(decomposed, &[&first]),
            copy("Ba\u{301}o ca\u{301}o 2.pdf", decomposed)
        );
        assert!(!folder.exists());
    }

    /// A Mac's file system takes canonically equivalent names for one file, measured on APFS
    /// on macOS 27.0: a file made as `Báo cáo.pdf` in either form is found in the other, and
    /// one made with U+F900 is found as U+8C48, its canonical decomposition.
    #[test]
    fn a_mac_takes_canonically_equivalent_names_for_one() {
        for (one, other) in [
            ("/d/B\u{e1}o c\u{e1}o.pdf", "/d/Ba\u{301}o ca\u{301}o.pdf"),
            ("/d/\u{f900}.pdf", "/d/\u{8c48}.pdf"),
            ("/d//x.pdf", "/d/x.pdf"),
        ] {
            assert!(
                same_file(Os::MacOs, Path::new(one), Path::new(other)),
                "{one:?}"
            );
            assert!(
                same_file(Os::MacOs, Path::new(other), Path::new(one)),
                "{other:?}"
            );
        }
        assert!(!same_file(
            Os::MacOs,
            Path::new("/d/a\u{301}.pdf"),
            Path::new("/d/a.pdf")
        ));
        let reserved = ["/d/Ba\u{301}o ca\u{301}o.pdf"];
        let name = destination(
            Os::MacOs,
            Path::new("/d"),
            "B\u{e1}o c\u{e1}o.pdf",
            |candidate| {
                reserved
                    .iter()
                    .any(|path| same_file(Os::MacOs, Path::new(path), candidate))
            },
        );
        assert_eq!(name, Path::new("/d/B\u{e1}o c\u{e1}o 2.pdf"));
    }

    /// From AccountWindowsTests.swift, and an address's host as `URL.host` gives it.
    #[test]
    fn a_downloads_host_is_read_from_inside_a_blob_link() {
        let host = |url: Option<&str>| download_host(url.map(str::to_owned));
        assert_eq!(
            host(Some("blob:https://claude.ai/1-2")).as_deref(),
            Some("claude.ai")
        );
        assert_eq!(
            host(Some("BLOB:https://claude.ai/1-2")).as_deref(),
            Some("claude.ai")
        );
        assert_eq!(
            host(Some("https://example.com/x.zip")).as_deref(),
            Some("example.com")
        );
        assert_eq!(host(Some("data:text/plain,x")), None);
        assert_eq!(host(Some("blob:null/abc")), None);
        assert_eq!(host(None), None);
        assert_eq!(
            host(Some("https://аpple.com/x.zip")).as_deref(),
            Some("xn--pple-43d.com"),
            "a look-alike host is named as the one the request goes to"
        );
        assert_eq!(
            host(Some("https://xn--claude-.ai/x.zip")).as_deref(),
            Some("xn--claude-.ai"),
            "a punycode label IDNA refuses, as URL.host keeps it"
        );
    }

    /// As PageDialogs.swift asked it.
    #[test]
    fn asking_before_a_download_names_the_file_and_its_host() {
        let asked = download_question("report.pdf".into(), Some("claude.ai".into()));
        assert_eq!(asked.title, "Download “report.pdf” from claude.ai?");
        assert_eq!(
            asked.message,
            "A page inside the site, such as an artifact, asked to save this file in your \
             Downloads folder."
        );
        assert_eq!(
            download_question("x".into(), Some(String::new())).title,
            "Download “x”?"
        );
        assert_eq!(download_question("x".into(), None).title, "Download “x”?");
    }
}
