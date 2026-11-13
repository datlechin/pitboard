use std::path::{Path, PathBuf};

const QUOTE: u16 = b'"' as u16;
const SPACE: u16 = b' ' as u16;
const TAB: u16 = b'\t' as u16;

// What follows the program on a command line, as it was given, read as `CommandLineToArgvW`
// reads the program: up to the next `"` where it starts with one, else up to a space or a tab.
pub(crate) fn after_program(line: &[u16]) -> &[u16] {
    let end = match line.first() {
        Some(&QUOTE) => line[1..]
            .iter()
            .position(|&c| c == QUOTE)
            .map_or(line.len(), |at| at + 2),
        _ => line
            .iter()
            .position(|&c| c == SPACE || c == TAB)
            .unwrap_or(line.len()),
    };
    let rest = &line[end..];
    let start = rest
        .iter()
        .position(|&c| c != SPACE && c != TAB)
        .unwrap_or(rest.len());
    &rest[start..]
}

// The program beside the launcher whose name is the launcher's without the `w` before its
// extension, as the core names `pitboard.exe`'s launcher `pitboardw.exe`.
pub(crate) fn launched(launcher: &Path) -> Option<PathBuf> {
    let stem = launcher.file_stem()?.to_str()?;
    let mut name =
        std::ffi::OsString::from(stem.strip_suffix(['w', 'W']).filter(|p| !p.is_empty())?);
    if let Some(extension) = launcher.extension() {
        name.push(".");
        name.push(extension);
    }
    Some(launcher.with_file_name(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn after(line: &str) -> String {
        let wide: Vec<u16> = line.encode_utf16().collect();
        String::from_utf16(after_program(&wide)).expect("UTF-16")
    }

    #[test]
    fn the_arguments_go_on_as_they_were_given() {
        for (line, arguments) in [
            (
                r#""C:\Program Files\Pitboard\bin\pitboardw.exe" renew --scheduled"#,
                "renew --scheduled",
            ),
            (
                r"C:\Pitboard\bin\pitboardw.exe watch --scheduled --at 90",
                "watch --scheduled --at 90",
            ),
            (
                "pitboardw.exe \t  \"two words\" \"a \\\"quote\\\"\"  x",
                "\"two words\" \"a \\\"quote\\\"\"  x",
            ),
            ("pitboardw.exe\trenew", "renew"),
            (r#""C:\x y\pitboardw.exe""#, ""),
            (r#""C:\x y\pitboardw.exe"renew"#, "renew"),
            ("pitboardw.exe", ""),
            ("pitboardw.exe   ", ""),
            (r#""C:\never closed\pitboardw.exe renew"#, ""),
            ("", ""),
        ] {
            assert_eq!(after(line), arguments, "{line}");
        }
    }

    #[test]
    fn the_launcher_starts_the_program_named_as_it_is_without_its_w() {
        let bin = Path::new("bin");
        for (launcher, program) in [
            ("pitboardw.exe", Some("pitboard.exe")),
            ("PITBOARDW.EXE", Some("PITBOARD.EXE")),
            ("pitboard.v2w.exe", Some("pitboard.v2.exe")),
            ("pitboard.exe", None),
            ("w.exe", None),
        ] {
            assert_eq!(
                launched(&bin.join(launcher)),
                program.map(|name| bin.join(name)),
                "{launcher}"
            );
        }
    }
}
