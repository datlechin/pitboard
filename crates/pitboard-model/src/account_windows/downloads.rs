//! What a download is called, where it comes from, and what asking before one says.

use crate::present::{Question, words};
use crate::{Engine, Front};
use pitboard_core::host::Os;
use pitboard_sites::WebAddress;
use std::path::{Path, PathBuf};
use unicode_normalization::UnicodeNormalization;

/// Where a download suggested as `suggested` is saved in `folder`, past its files and `reserved`.
// Numbered as the system numbers a copy, past the downloads still running, since the web view
// wants a file that is not there yet and two can ask at once. The web view has made the name safe
// (docs/facts/webkit.md). A path in `reserved` may come in another Unicode form: the macOS app
// keeps each as a `URL`, whose `path` Foundation gives decomposed (docs/facts/foundation-urls.md).
#[uniffi::export]
pub fn download_destination(
    front: Front,
    folder: String,
    suggested: String,
    reserved: Vec<String>,
) -> String {
    let taken = |candidate: &Path| {
        reserved
            .iter()
            .any(|path| same_file(front.os(), Path::new(path), candidate))
            || candidate.exists()
    };
    destination(front.engine(), Path::new(&folder), &suggested, taken)
        .to_string_lossy()
        .into_owned()
}

/// Where a download `engine` suggested as `suggested` goes in `folder`: the first name
/// `taken` does not say is taken.
fn destination(
    engine: Engine,
    folder: &Path,
    suggested: &str,
    taken: impl Fn(&Path) -> bool,
) -> PathBuf {
    let name = match last_component(suggested) {
        "" | "/" => "Download",
        name => name,
    };
    let (base, extension) = match engine {
        Engine::WebKit | Engine::WebKitGtk => split_extension(name),
        Engine::WebView2 => split_as_chromium(name),
    };
    (1u64..)
        .map(|nth| folder.join(numbered(engine, name, base, extension, nth)))
        .find(|candidate| !taken(candidate))
        .unwrap_or_else(|| folder.join(name))
}

/// The `nth` file named `name` in one folder, `base` and `extension` being its parts: the
/// name itself first, then as a copy of it is numbered in an app whose pages `engine` loads.
fn numbered(engine: Engine, name: &str, base: &str, extension: Option<&str>, nth: u64) -> String {
    if nth == 1 {
        return name.to_owned();
    }
    match engine {
        // As Finder numbers one: `report 2.pdf`, then `report 3.pdf`. No app runs on Linux,
        // and it numbers as the Mac does.
        Engine::WebKit | Engine::WebKitGtk => match extension {
            Some(extension) => format!("{base} {nth}.{extension}"),
            None => format!("{base} {nth}"),
        },
        // In brackets from one, as the owner decided: `report (1).pdf`, then `report (2).pdf`,
        // and `archive (1).tar.gz`, as WebView2 numbers a download.
        Engine::WebView2 => {
            let copy = nth - 1;
            match extension {
                Some(extension) => format!("{base} ({copy}).{extension}"),
                None => format!("{base} ({copy})"),
            }
        }
    }
}

/// Whether `os` takes the paths `one` and `other` for one file.
fn same_file(os: Os, one: &Path, other: &Path) -> bool {
    match os {
        // APFS takes names canonically equivalent in Unicode for one
        // (docs/facts/foundation-urls.md). No app runs on Linux, compared as the Mac.
        Os::MacOs | Os::Linux => {
            let decomposed =
                |path: &Path| PathBuf::from(path.to_string_lossy().nfd().collect::<String>());
            decomposed(one) == decomposed(other)
        }
        // Two Unicode forms of one name are two files there.
        Os::Windows => pitboard_core::host::same_path_in_any_case(one, other),
    }
}

// As `FilePath::Extension` splits a download's name for WebView2 to number it, read in
// Chromium 155.0.8059.87's `base/files/file_path.cc`. Its distance between the two dots
// counts the UTF-16 units of a Windows path.
fn split_as_chromium(name: &str) -> (&str, Option<&str>) {
    const COMPRESSED: [&str; 9] = ["bz", "bz2", "gz", "lz", "lzma", "lzo", "xz", "z", "zst"];
    if matches!(name, "." | "..") {
        return (name, None);
    }
    let Some(last) = name.rfind('.') else {
        return (name, None);
    };
    let dot = match name[..last].rfind('.') {
        Some(before) if name[before + 1..].eq_ignore_ascii_case("user.js") => before,
        Some(before)
            if (2..=5).contains(&name[before..last].encode_utf16().count())
                && COMPRESSED
                    .iter()
                    .any(|suffix| name[last + 1..].eq_ignore_ascii_case(suffix)) =>
        {
            before
        }
        _ => last,
    };
    (&name[..dot], Some(&name[dot + 1..]))
}

// As `NSString.lastPathComponent` reads it (docs/facts/foundation-urls.md).
fn last_component(path: &str) -> &str {
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() {
        return if path.is_empty() { "" } else { "/" };
    }
    trimmed.rsplit('/').next().unwrap_or(trimmed)
}

// As `NSString` splits a file name into its base and extension (docs/facts/foundation-urls.md).
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

// Asked where a page inside the site, rather than the site's own page, started the download.
#[uniffi::export]
pub fn download_question(name: String, host: Option<String>) -> Question {
    Question::asking(
        match host.filter(|host| !host.is_empty()) {
            Some(host) => format!("Download “{name}” from {host}?"),
            None => format!("Download “{name}”?"),
        },
        "A page inside the site, such as an artifact, asked to save this file in your Downloads \
         folder."
            .into(),
        words::DOWNLOAD.into(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn on_a_mac(suggested: &str, taken: &[&str]) -> String {
        let taken: HashSet<String> = taken.iter().map(|name| downloads(name)).collect();
        destination(
            Engine::WebKit,
            Path::new("/Downloads"),
            suggested,
            |candidate| taken.contains(candidate.to_str().expect("UTF-8")),
        )
        .to_string_lossy()
        .into_owned()
    }

    fn downloads(name: &str) -> String {
        Path::new("/Downloads")
            .join(name)
            .to_string_lossy()
            .into_owned()
    }

    /// A download never overwrites a file, and two downloads never get one name.
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
        let name = destination(
            Engine::WebKitGtk,
            Path::new("/d"),
            "report.pdf",
            |candidate| candidate == Path::new("/d/report.pdf"),
        );
        assert_eq!(name, Path::new("/d/report 2.pdf"));
    }

    /// Windows numbers a copy in brackets from one, as the owner decided and as WebView2
    /// numbers a download, so the second `report.pdf` is `report (1).pdf`.
    #[test]
    fn windows_numbers_a_copy_in_brackets_from_one() {
        let on_windows = |suggested: &str, taken: &[&str]| {
            destination(Engine::WebView2, Path::new("/d"), suggested, |candidate| {
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

    // A compressor's extension keeps one of up to four UTF-16 units before it.
    #[test]
    fn windows_splits_a_name_as_webview2_does() {
        for (suggested, second) in [
            ("archive.tar.gz", "archive (1).tar.gz"),
            ("photo.TAR.GZ", "photo (1).TAR.GZ"),
            ("notes.txt.zst", "notes (1).txt.zst"),
            ("a.b.lzma", "a (1).b.lzma"),
            ("script.user.js", "script (1).user.js"),
            ("dump.sql5.gz", "dump (1).sql5.gz"),
            ("dump.mysql.gz", "dump.mysql (1).gz"),
            ("a.日本.gz", "a (1).日本.gz"),
            ("a.абвг.gz", "a (1).абвг.gz"),
            ("a.абвгд.gz", "a.абвгд (1).gz"),
            ("a..gz", "a. (1).gz"),
            ("archive.tar.zip", "archive.tar (1).zip"),
            ("a.b c", "a (1).b c"),
            ("name.", "name (1)."),
            (".hidden", " (1).hidden"),
            ("..", ".. (1)"),
        ] {
            let name = destination(Engine::WebView2, Path::new("/d"), suggested, |candidate| {
                candidate == Path::new("/d").join(suggested)
            });
            assert_eq!(name, Path::new("/d").join(second), "{suggested:?}");
        }
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
        let name = destination(
            Engine::WebView2,
            Path::new("/d"),
            "report.pdf",
            |candidate| same_file(Os::Windows, Path::new("/d/Report.PDF"), candidate),
        );
        assert_eq!(name, Path::new("/d/report (1).pdf"));
    }

    /// The export looks at the folder as well as the names reserved for downloads running.
    #[test]
    fn a_file_already_there_or_a_name_reserved_is_taken() {
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let folder = scratch.path();
        std::fs::write(folder.join("report.pdf"), b"").expect("a file");
        for (front, reserved, chosen) in [
            (Front::MacMenuBar, "report 2.pdf", "report 3.pdf"),
            (
                Front::WindowsNotificationArea,
                "report (1).pdf",
                "report (2).pdf",
            ),
        ] {
            assert_eq!(
                download_destination(
                    front,
                    folder.to_string_lossy().into_owned(),
                    "report.pdf".into(),
                    vec![folder.join(reserved).to_string_lossy().into_owned()],
                ),
                folder.join(chosen).to_string_lossy(),
                "{front:?}"
            );
        }
    }

    /// A name given to a download still running is taken whatever Unicode form the app hands
    /// it back in. WebKit suggests `Báo cáo.pdf` precomposed, and the app keeps the path it
    /// is given as a `URL`, whose `path` Foundation gives decomposed, measured on macOS 27.0.
    #[test]
    fn a_name_reserved_in_another_unicode_form_is_taken() {
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let folder = scratch.path().join("not-made");
        let at = |name: &str| folder.join(name).to_string_lossy().into_owned();
        let precomposed = "B\u{e1}o c\u{e1}o.pdf";
        let decomposed = "Ba\u{301}o ca\u{301}o.pdf";
        for front in [Front::MacMenuBar, Front::WindowsNotificationArea] {
            let choose = |suggested: &str, reserved: &[&str]| {
                download_destination(
                    front,
                    folder.to_string_lossy().into_owned(),
                    suggested.into(),
                    reserved.iter().map(|path| (*path).to_owned()).collect(),
                )
            };
            let first = choose(precomposed, &[]);
            assert_eq!(first, at(precomposed));
            // On Windows the other Unicode form is another name, so it is free (`same_file`).
            let copy = |name: &str, of: &str| match front {
                Front::MacMenuBar | Front::Gnome => at(name),
                Front::WindowsNotificationArea => at(of),
            };
            assert_eq!(
                choose(precomposed, &[&at(decomposed)]),
                copy("B\u{e1}o c\u{e1}o 2.pdf", precomposed),
                "reserved as Foundation gives its path"
            );
            assert_eq!(
                choose(precomposed, &[&first]),
                match front {
                    Front::MacMenuBar | Front::Gnome => at("B\u{e1}o c\u{e1}o 2.pdf"),
                    Front::WindowsNotificationArea => at("B\u{e1}o c\u{e1}o (1).pdf"),
                }
            );
            assert_eq!(
                choose(decomposed, &[&first]),
                copy("Ba\u{301}o ca\u{301}o 2.pdf", decomposed)
            );
        }
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
            Engine::WebKit,
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

    /// An address's host as `URL.host` gives it.
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

    // As PageDialogs.swift asked it.
    #[test]
    fn asking_before_a_download_names_the_file_and_its_host() {
        let asked = download_question("report.pdf".into(), Some("claude.ai".into()));
        assert_eq!(asked.title, "Download “report.pdf” from claude.ai?");
        assert_eq!(
            (
                asked.confirm.as_str(),
                asked.destructive,
                asked.confirm_is_default
            ),
            ("Download", false, true)
        );
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
