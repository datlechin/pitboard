//! A shallow drift alarm: it reads the literals each fact in a tool's register is readable
//! by out of a build of that tool and says which are still there. A literal being present
//! does not prove the behaviour around it is unchanged; one disappearing proves something
//! moved. A test against a running tool would need a real sign-in, so it cannot run
//! unattended.
//!
//! A tool's builds for different systems carry different code, so the system is read from
//! the binary's own header and a fact is read only from the builds its register's table
//! reads it on. Read from the Linux build alone, Claude Code's keychain facts looked gone on
//! every build from 2.1.278 on, while the one real change, in 2.1.281, showed only in the
//! macOS build. Claude Desktop's facts are read from its app's archive, `app.asar`, which
//! its register's table ties to macOS.

use clap::Parser;
use clap::builder::{PossibleValuesParser, TypedValueParser};
use pitboard_core::assumptions::{self, Assumption, OnSystem, Platform, Reading};
use pitboard_core::provider::ProviderId;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// Check Pitboard's facts about Claude Code, Codex or Claude Desktop against a build of that
/// tool.
///
/// Exits 1 when a fact has moved, and 2 when the arguments or the build cannot be read.
#[derive(Parser, Debug, PartialEq)]
#[command(name = "pitboard-conformance")]
struct Args {
    /// The tool's native binary, not its npm wrapper; for Claude Desktop, its app's
    /// `Contents/Resources/app.asar`.
    build: PathBuf,
    /// The tool whose register the build is read against.
    #[arg(long, default_value = ProviderId::Claude.code(), value_parser = provider())]
    provider: ProviderId,
    /// Print the report as JSON.
    #[arg(long)]
    json: bool,
}

fn provider() -> impl TypedValueParser<Value = ProviderId> {
    PossibleValuesParser::new(ProviderId::ALL.iter().map(|p| p.code()))
        .map(|code| ProviderId::parse(&code).expect("a code ProviderId::ALL gives"))
}

fn main() -> ExitCode {
    let args = Args::parse();
    let path = &args.build;
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("cannot read {}: {e}", path.display());
            return ExitCode::from(2);
        }
    };
    let platform = match platform_of(&bytes, args.provider) {
        Ok(platform) => platform,
        Err(refused) => {
            eprintln!("{}", refused.said(path, args.provider));
            return ExitCode::from(2);
        }
    };
    let strings = assumptions::printable_runs(&bytes, assumptions::SHORTEST_RUN);
    let sorted = Sorted::of(args.provider, platform, &strings);

    if args.json {
        println!("{}", json_report(path, args.provider, platform, &sorted));
    } else {
        print!("{}", text_report(path, args.provider, platform, &sorted));
    }

    if sorted.moved().is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// One tool's facts as one build reads them, each where its line in the register's table
/// puts it on the build's system.
///
/// A fact not read from this system's builds is left out of the readings, not reported:
/// its literals being absent here says nothing about it. What the register says of it
/// instead is kept, so nothing is skipped without a reason.
struct Sorted {
    readings: Vec<(&'static Assumption, Reading)>,
    not_read: Vec<(&'static Assumption, &'static str)>,
    /// The facts whose reading on this system waits on the Windows work, with the pull
    /// requests that read them and what those read.
    pending: Vec<(&'static Assumption, &'static [&'static str], &'static str)>,
}

impl Sorted {
    fn of(provider: ProviderId, platform: Platform, strings: &str) -> Sorted {
        let mut sorted = Sorted {
            readings: Vec::new(),
            not_read: Vec::new(),
            pending: Vec::new(),
        };
        for a in assumptions::of(provider) {
            match assumptions::on(provider, a.name, platform)
                .expect("every fact has a line in its register's table")
            {
                OnSystem::Read(_) => sorted
                    .readings
                    .push((a, assumptions::read_from_build(a, strings))),
                OnSystem::NotRead(why) => sorted.not_read.push((a, why)),
                OnSystem::Pending { by, reads } => sorted.pending.push((a, by, reads)),
            }
        }
        sorted
    }

    /// The facts read here that moved. Both kinds of drift count: a fact that rested on a
    /// keyring backend not existing is as broken by one arriving as a service name is by
    /// being renamed.
    fn moved(&self) -> Vec<&'static Assumption> {
        self.readings
            .iter()
            .filter(|(_, r)| matches!(r, Reading::Moved(_) | Reading::Appeared(_)))
            .map(|(a, _)| *a)
            .collect()
    }
}

fn json_report(
    path: &Path,
    provider: ProviderId,
    platform: Platform,
    sorted: &Sorted,
) -> serde_json::Value {
    serde_json::json!({
        "build": path.display().to_string(),
        "platform": platform.code(),
        "verified_against": assumptions::verified_against(provider, platform),
        "assumptions": sorted.readings.iter().map(|(a, r)| serde_json::json!({
            "name": a.name,
            "reading": match r {
                Reading::Holds => "holds",
                Reading::NotReadable => "not_readable",
                Reading::Moved(_) => "moved",
                Reading::Appeared(_) => "appeared",
            },
            "gone": match r {
                Reading::Moved(gone) => gone.clone(),
                _ => Vec::new(),
            },
            "appeared": match r {
                Reading::Appeared(found) => found.clone(),
                _ => Vec::new(),
            },
            "verified_against": assumptions::verified_on(provider, a.name, platform),
            "depends": a.depends,
        })).collect::<Vec<_>>(),
        "moved": sorted.moved().iter().map(|a| a.name).collect::<Vec<_>>(),
        "not_read_here": sorted.not_read.iter().map(|(a, _)| a.name).collect::<Vec<_>>(),
        "pending_here": sorted.pending.iter().map(|(a, _, _)| a.name).collect::<Vec<_>>(),
    })
}

// Writing to a String cannot fail, so what writeln! returns is dropped.
fn text_report(path: &Path, provider: ProviderId, platform: Platform, sorted: &Sorted) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "{}\na {} build; Pitboard's facts about {} were read from {}\n",
        path.display(),
        platform.code(),
        provider.code(),
        assumptions::verified_against(provider, platform)
    );
    for (a, reading) in &sorted.readings {
        match reading {
            Reading::Holds => {
                let _ = writeln!(out, "  ok       {}", a.name);
            }
            Reading::NotReadable => {
                let _ = writeln!(
                    out,
                    "  no probe {}  (a fact about behaviour, not a name)",
                    a.name
                );
            }
            Reading::Moved(gone) => {
                let _ = writeln!(out, "  MOVED    {}", a.name);
                for needle in gone {
                    let _ = writeln!(out, "             gone: {needle}");
                }
                let _ = writeln!(out, "             this breaks: {}", a.depends);
            }
            Reading::Appeared(found) => {
                let _ = writeln!(out, "  APPEARED {}", a.name);
                for needle in found {
                    let _ = writeln!(out, "             now present: {needle}");
                }
                let _ = writeln!(out, "             this breaks: {}", a.depends);
            }
        }
    }
    for (a, why) in &sorted.not_read {
        let _ = writeln!(
            out,
            "  skipped  {}  (not read from {} builds)",
            a.name,
            platform.code()
        );
        let _ = writeln!(out, "             {why}");
    }
    for (a, by, reads) in &sorted.pending {
        let _ = writeln!(
            out,
            "  pending  {}  (its {} reading waits on {})",
            a.name,
            platform.code(),
            by.join(" and ")
        );
        let _ = writeln!(out, "             {reads}");
    }
    let _ = writeln!(out);
    let moved = sorted.moved();
    if moved.is_empty() {
        let _ = writeln!(
            out,
            "Everything Pitboard can read from a build is still there."
        );
    } else {
        let _ = writeln!(
            out,
            "{} of Pitboard's facts can no longer be read from this build. Re-measure them \
             against it before trusting a switch.",
            moved.len()
        );
    }
    out
}

fn platform_of(bytes: &[u8], provider: ProviderId) -> Result<Platform, Refused> {
    let program = match bytes.get(..4).ok_or(Refused::Unknown)? {
        [0x7f, b'E', b'L', b'F'] => Platform::Linux,
        [0xcf, 0xfa, 0xed, 0xfe]
        | [0xce, 0xfa, 0xed, 0xfe]
        | [0xfe, 0xed, 0xfa, 0xcf]
        | [0xfe, 0xed, 0xfa, 0xce]
        | [0xca, 0xfe, 0xba, 0xbe] => Platform::MacOs,
        [b'M', b'Z', ..] => windows_machine(bytes).ok_or(Refused::Unknown)?,
        // Electron's asar: a pickle holding the index's size, then the JSON index at byte 16.
        [4, 0, 0, 0] if bytes.get(16..24) == Some(b"{\"files\"".as_slice()) => {
            return assumptions::archive_platform(provider).ok_or(Refused::Archive);
        }
        _ => return Err(Refused::Unknown),
    };
    match assumptions::archive_platform(provider) {
        Some(_) => Err(Refused::Program),
        None => Ok(program),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Refused {
    Archive,
    Program,
    Unknown,
}

impl Refused {
    fn said(self, path: &Path, provider: ProviderId) -> String {
        match self {
            Refused::Archive => format!(
                "{} is an Electron app's archive, which {}'s register reads on no system",
                path.display(),
                provider.name()
            ),
            Refused::Program => format!(
                "{} is a program, and {}'s register is read from its app's archive, \
                 `Contents/Resources/app.asar`",
                path.display(),
                provider.name()
            ),
            Refused::Unknown => format!(
                "{} is not an ELF, a Mach-O, an x64 or ARM64 PE binary or an Electron app's \
                 archive, so its system is unknown",
                path.display()
            ),
        }
    }
}

/// `IMAGE_FILE_MACHINE_AMD64` and `IMAGE_FILE_MACHINE_ARM64`, the two machines Claude Code
/// and Codex ship Windows builds for.
const PE_MACHINES: [u16; 2] = [0x8664, 0xaa64];

/// A Windows binary starts `MZ`, and the four bytes at 0x3C give where its `PE\0\0`
/// signature is. The machine it is for is the two bytes after that. An `MZ` file with no
/// signature where its header says, or one for another machine, is not one this reads.
fn windows_machine(bytes: &[u8]) -> Option<Platform> {
    let at = bytes.get(0x3c..0x40)?;
    let at = usize::try_from(u32::from_le_bytes([at[0], at[1], at[2], at[3]])).ok()?;
    let header = bytes.get(at..at.checked_add(6)?)?;
    if header[..4] != *b"PE\0\0" {
        return None;
    }
    let machine = u16::from_le_bytes([header[4], header[5]]);
    PE_MACHINES.contains(&machine).then_some(Platform::Windows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    /// The system comes from the binary itself, so a build cannot be checked against the
    /// wrong list of facts. Headers as the 2.1.284 packages have them.
    #[test]
    fn the_system_is_read_from_the_binary() {
        let system = |bytes: &[u8]| platform_of(bytes, ProviderId::Claude);
        assert_eq!(system(b"\x7fELF\x02\x01"), Ok(Platform::Linux));
        assert_eq!(system(b"\xcf\xfa\xed\xfe\x0c"), Ok(Platform::MacOs));
        assert_eq!(system(b"\xca\xfe\xba\xbe"), Ok(Platform::MacOs));
        assert_eq!(system(b"#!/bin/sh"), Err(Refused::Unknown));
        assert_eq!(system(b"\x7fE"), Err(Refused::Unknown));
    }

    /// A PE header: `MZ`, the signature's offset at 0x3C, then the signature and the
    /// machine there.
    fn pe(signature_at: u32, signature: &[u8; 4], machine: u16) -> Vec<u8> {
        let at = signature_at as usize;
        let mut bytes = vec![0; at + 6];
        bytes[..2].copy_from_slice(b"MZ");
        bytes[0x3c..0x40].copy_from_slice(&signature_at.to_le_bytes());
        bytes[at..at + 4].copy_from_slice(signature);
        bytes[at + 4..].copy_from_slice(&machine.to_le_bytes());
        bytes
    }

    /// A Windows build is told by its PE header, for x64 and for ARM64. Before, a Windows
    /// build was refused as neither ELF nor Mach-O. Claude Code 2.1.289's and Codex
    /// 0.160.0's Windows builds, x64 and ARM64, all put the signature at 0x78; the header
    /// says where, and another place is followed as well.
    #[test]
    fn a_windows_build_is_read_from_its_pe_header() {
        let system = |bytes: &[u8]| platform_of(bytes, ProviderId::Claude);
        assert_eq!(system(&pe(0x78, b"PE\0\0", 0x8664)), Ok(Platform::Windows));
        assert_eq!(system(&pe(0x78, b"PE\0\0", 0xaa64)), Ok(Platform::Windows));
        assert_eq!(system(&pe(0x118, b"PE\0\0", 0xaa64)), Ok(Platform::Windows));
    }

    /// Anything that is not quite a PE header for one of the two machines is unknown, never
    /// taken for Windows on its first two bytes.
    #[test]
    fn what_is_not_an_x64_or_arm64_pe_binary_is_unknown() {
        let unknown =
            |bytes: &[u8]| platform_of(bytes, ProviderId::Claude) == Err(Refused::Unknown);
        // A DOS program: `MZ`, and no signature at the offset its header gives.
        let mut dos = vec![0u8; 0x80];
        dos[..2].copy_from_slice(b"MZ");
        assert!(unknown(&dos));
        // Something else where the signature should be.
        assert!(unknown(&pe(0x80, b"NE\0\0", 0x8664)));
        // Cut short: before the offset, inside the signature, and inside the machine.
        assert!(unknown(b"MZ\x90\x00"));
        assert!(unknown(&pe(0x80, b"PE\0\0", 0x8664)[..0x82]));
        assert!(unknown(&pe(0x80, b"PE\0\0", 0x8664)[..0x85]));
        // An offset past the end, and one at the very top of the range.
        let mut far = pe(0x80, b"PE\0\0", 0x8664);
        far[0x3c..0x40].copy_from_slice(&0x1000u32.to_le_bytes());
        assert!(unknown(&far));
        far[0x3c..0x40].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(unknown(&far));
        // 32-bit x86 and 32-bit ARM, which neither tool ships.
        assert!(unknown(&pe(0x80, b"PE\0\0", 0x014c)));
        assert!(unknown(&pe(0x80, b"PE\0\0", 0x01c4)));
    }

    // As Electron writes one: the index's size, the JSON index at byte 16, then the files as is.
    fn asar(files: &[(&str, &str)]) -> Vec<u8> {
        let mut offset = 0;
        let mut index = serde_json::Map::new();
        for (name, body) in files {
            index.insert(
                (*name).to_owned(),
                serde_json::json!({ "size": body.len(), "offset": offset.to_string() }),
            );
            offset += body.len();
        }
        let index = serde_json::json!({ "files": index }).to_string();
        let padded = index.len().next_multiple_of(4);
        let payload = u32::try_from(4 + padded).expect("a small index");
        let mut bytes = Vec::new();
        bytes.extend(4u32.to_le_bytes());
        bytes.extend((payload + 4).to_le_bytes());
        bytes.extend(payload.to_le_bytes());
        bytes.extend(
            u32::try_from(index.len())
                .expect("a small index")
                .to_le_bytes(),
        );
        bytes.extend(index.as_bytes());
        bytes.resize(16 + padded, 0);
        for (_, body) in files {
            bytes.extend(body.as_bytes());
        }
        bytes
    }

    #[test]
    fn an_electron_archive_is_read_for_the_tool_whose_register_reads_one() {
        let probes = every_probe(ProviderId::Desktop, Platform::MacOs);
        let archive = asar(&[("package.json", "{}"), (".vite/build/index.js", &probes)]);
        assert_eq!(
            platform_of(&archive, ProviderId::Desktop),
            Ok(Platform::MacOs)
        );
        let strings = assumptions::printable_runs(&archive, assumptions::SHORTEST_RUN);
        let sorted = Sorted::of(ProviderId::Desktop, Platform::MacOs, &strings);
        assert!(sorted.moved().is_empty(), "{:?}", names(&sorted.readings));

        // A pickle's first word with no index at byte 16 is no archive.
        let unknown =
            |bytes: &[u8]| platform_of(bytes, ProviderId::Desktop) == Err(Refused::Unknown);
        assert!(unknown(&archive[..20]));
        assert!(unknown(b"\x04\x00\x00\x00"));
        let mut other = archive.clone();
        other[16..24].copy_from_slice(b"{\"links\"");
        assert!(unknown(&other));
        let mut other = archive;
        other[0] = 8;
        assert!(unknown(&other));
    }

    #[test]
    fn an_electron_archive_for_a_tool_that_ships_binaries_is_refused() {
        let archive = asar(&[("index.js", "")]);
        for provider in [ProviderId::Claude, ProviderId::Codex] {
            assert_eq!(
                platform_of(&archive, provider),
                Err(Refused::Archive),
                "{provider:?}"
            );
        }
        assert_eq!(
            Refused::Archive.said(Path::new("app.asar"), ProviderId::Codex),
            "app.asar is an Electron app's archive, which Codex's register reads on no system"
        );
        assert_eq!(
            Refused::Unknown.said(Path::new("app.asar"), ProviderId::Desktop),
            "app.asar is not an ELF, a Mach-O, an x64 or ARM64 PE binary or an Electron app's \
             archive, so its system is unknown"
        );
    }

    // Of the archive's literals, Claude's program holds only the bundle id, so nearly every
    // fact would read moved.
    #[test]
    fn a_program_of_a_tool_whose_register_reads_its_archive_is_refused() {
        for program in [
            b"\xcf\xfa\xed\xfe\x0c".to_vec(),
            b"\xca\xfe\xba\xbe".to_vec(),
            b"\x7fELF\x02\x01".to_vec(),
            pe(0x78, b"PE\0\0", 0xaa64),
        ] {
            assert_eq!(
                platform_of(&program, ProviderId::Desktop),
                Err(Refused::Program),
                "{program:x?}"
            );
            assert!(platform_of(&program, ProviderId::Claude).is_ok());
        }
        assert_eq!(
            platform_of(b"#!/bin/sh", ProviderId::Desktop),
            Err(Refused::Unknown)
        );
        assert_eq!(
            Refused::Program.said(
                Path::new("/Applications/Claude.app/Contents/MacOS/Claude"),
                ProviderId::Desktop
            ),
            "/Applications/Claude.app/Contents/MacOS/Claude is a program, and Claude Desktop's \
             register is read from its app's archive, `Contents/Resources/app.asar`"
        );
    }

    #[test]
    fn every_desktop_fact_is_read_on_macos_alone() {
        assert_eq!(
            assumptions::archive_platform(ProviderId::Desktop),
            Some(Platform::MacOs)
        );
        for a in assumptions::of(ProviderId::Desktop) {
            assert_eq!(
                assumptions::read_on(ProviderId::Desktop, a.name),
                [Platform::MacOs],
                "{}",
                a.name
            );
        }
        let sorted = Sorted::of(ProviderId::Desktop, Platform::MacOs, "");
        assert!(sorted.not_read.is_empty() && sorted.pending.is_empty());
    }

    const SYSTEMS: [Platform; 3] = [Platform::MacOs, Platform::Linux, Platform::Windows];

    /// Every literal the facts read on one system probe for, as a build that still holds
    /// them all would show them.
    fn every_probe(provider: ProviderId, platform: Platform) -> String {
        assumptions::of(provider)
            .iter()
            .filter(|a| assumptions::verified_on(provider, a.name, platform).is_some())
            .flat_map(|a| a.probe.iter().copied())
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn names<T>(list: &[(&'static Assumption, T)]) -> Vec<&'static str> {
        list.iter().map(|(a, _)| a.name).collect()
    }

    /// Each fact lands once, where its line in the register's table puts it on the build's
    /// system: read, not read with the line's reason, or waiting on the pull requests the
    /// line names.
    #[test]
    fn each_fact_is_read_skipped_or_pending_as_its_line_says() {
        for &provider in ProviderId::ALL {
            for platform in SYSTEMS {
                let sorted = Sorted::of(provider, platform, "");
                let on = |name| assumptions::on(provider, name, platform);
                for (a, _) in &sorted.readings {
                    assert!(matches!(on(a.name), Some(OnSystem::Read(_))), "{}", a.name);
                }
                for &(a, why) in &sorted.not_read {
                    assert_eq!(on(a.name), Some(OnSystem::NotRead(why)), "{}", a.name);
                }
                for &(a, by, reads) in &sorted.pending {
                    assert_eq!(
                        on(a.name),
                        Some(OnSystem::Pending { by, reads }),
                        "{}",
                        a.name
                    );
                }
                let mut seen = names(&sorted.readings);
                seen.extend(names(&sorted.not_read));
                seen.extend(sorted.pending.iter().map(|(a, _, _)| a.name));
                seen.sort_unstable();
                let mut facts: Vec<&str> =
                    assumptions::of(provider).iter().map(|a| a.name).collect();
                facts.sort_unstable();
                assert_eq!(seen, facts, "{provider:?} on {}", platform.code());
            }
        }
    }

    /// Claude Code's Windows build carries `Bun.secrets` for its Credential Manager store,
    /// and none of the keychain facts' literals need be there. Neither is drift, since
    /// neither fact is read there; on Linux the same `Bun.secrets` is a keyring arrived.
    #[test]
    fn what_a_build_is_not_read_for_is_never_drift() {
        let windows = format!(
            "{}\nBun.secrets.get",
            every_probe(ProviderId::Claude, Platform::Windows)
        );
        let sorted = Sorted::of(ProviderId::Claude, Platform::Windows, &windows);
        assert!(sorted.moved().is_empty(), "{:?}", names(&sorted.readings));
        assert!(names(&sorted.not_read).contains(&"no_keyring_off_macos"));
        assert!(names(&sorted.not_read).contains(&"keychain_write_route"));

        let linux = format!(
            "{}\nBun.secrets.get",
            every_probe(ProviderId::Claude, Platform::Linux)
        );
        let sorted = Sorted::of(ProviderId::Claude, Platform::Linux, &linux);
        let moved: Vec<&str> = sorted.moved().iter().map(|a| a.name).collect();
        assert_eq!(moved, ["no_keyring_off_macos"]);
    }

    // A warm-up on Windows rests on each (`windows_token_env`); macOS and Linux hand a descriptor.
    #[test]
    fn a_windows_build_that_moves_the_token_variable_the_scrub_or_the_git_lookup_has_moved() {
        let windows = every_probe(ProviderId::Claude, Platform::Windows);
        for gone in [
            r#"{source:"CLAUDE_CODE_OAUTH_TOKEN",hasToken:!0}"#,
            ".CLAUDE_CODE_OAUTH_TOKEN,refreshToken:null,expiresAt:null",
            "process.env.CLAUDE_CODE_SUBPROCESS_ENV_SCRUB",
            "windowsHide:!0,env:process.env}).trim().split(",
        ] {
            assert!(windows.contains(gone), "{gone}");
            let build = windows.replace(gone, "");
            let sorted = Sorted::of(ProviderId::Claude, Platform::Windows, &build);
            let moved: Vec<&str> = sorted.moved().iter().map(|a| a.name).collect();
            assert_eq!(moved, ["windows_token_env"], "{gone}");
        }
        for platform in [Platform::MacOs, Platform::Linux] {
            let sorted = Sorted::of(ProviderId::Claude, platform, "");
            assert!(
                names(&sorted.not_read).contains(&"windows_token_env"),
                "{}",
                platform.code()
            );
        }
    }

    /// The text lists every fact it does not read, a skipped one with the reason its line
    /// gives and a pending one with the pull requests it waits on and what they read, so
    /// nothing is left out without a word.
    #[test]
    fn the_text_says_why_each_fact_left_out_is_left_out() {
        for &provider in ProviderId::ALL {
            for platform in SYSTEMS {
                let sorted = Sorted::of(provider, platform, &every_probe(provider, platform));
                let text = text_report(Path::new("/b"), provider, platform, &sorted);
                let system = platform.code();
                for (a, why) in &sorted.not_read {
                    let line = format!(
                        "  skipped  {}  (not read from {system} builds)\n             {why}\n",
                        a.name
                    );
                    assert!(text.contains(&line), "{text}");
                }
                for (a, by, reads) in &sorted.pending {
                    let line = format!(
                        "  pending  {}  (its {system} reading waits on {})\n             {reads}\n",
                        a.name,
                        by.join(" and ")
                    );
                    assert!(text.contains(&line), "{text}");
                }
                assert!(
                    text.ends_with("Everything Pitboard can read from a build is still there.\n")
                );
            }
        }
        let sorted = Sorted::of(ProviderId::Claude, Platform::Windows, "");
        let text = text_report(
            Path::new("claude.exe"),
            ProviderId::Claude,
            Platform::Windows,
            &sorted,
        );
        assert!(text.starts_with(
            "claude.exe\na windows build; Pitboard's facts about claude were read from 2.1.289\n\n"
        ));
        assert!(
            text.contains("  pending  windows_credman_cache  (its windows reading waits on W27)\n")
        );
        assert!(!text.contains("MOVED    windows_credman_cache"));
    }

    /// `--json` lists the facts it does not read in two keys, `not_read_here` and
    /// `pending_here`, beside the readings, and dates each reading by the build its system
    /// was read from.
    #[test]
    fn the_json_lists_what_is_not_read_and_what_waits() {
        for &provider in ProviderId::ALL {
            for platform in SYSTEMS {
                let sorted = Sorted::of(provider, platform, "");
                let report = json_report(Path::new("/b"), provider, platform, &sorted);
                let listed = |key: &str| -> Vec<&str> {
                    report[key]
                        .as_array()
                        .expect(key)
                        .iter()
                        .map(|v| v.as_str().expect(key))
                        .collect()
                };
                assert_eq!(listed("not_read_here"), names(&sorted.not_read));
                let pending: Vec<&str> = sorted.pending.iter().map(|(a, _, _)| a.name).collect();
                assert_eq!(listed("pending_here"), pending);
                let read = report["assumptions"].as_array().expect("assumptions");
                assert_eq!(read.len(), sorted.readings.len());
                for (entry, (a, _)) in read.iter().zip(&sorted.readings) {
                    assert_eq!(entry["name"], a.name);
                    assert_eq!(
                        entry["verified_against"],
                        assumptions::verified_on(provider, a.name, platform).expect("read here")
                    );
                }
                assert_eq!(report["platform"], platform.code());
            }
        }
        let sorted = Sorted::of(ProviderId::Codex, Platform::Windows, "");
        let report = json_report(
            Path::new("codex.exe"),
            ProviderId::Codex,
            Platform::Windows,
            &sorted,
        );
        assert_eq!(report["verified_against"], "0.160.0");
        assert!(
            report["pending_here"]
                .as_array()
                .expect("pending_here")
                .contains(&serde_json::json!("codex_login_is_driveable"))
        );
    }

    fn parse(line: &str) -> Result<Args, clap::Error> {
        Args::try_parse_from(std::iter::once("pitboard-conformance").chain(line.split_whitespace()))
    }

    #[test]
    fn the_arguments_are_well_formed() {
        Args::command().debug_assert();
    }

    /// The build was once taken as whatever came two places after `--provider`, so a flag
    /// there was read as the file to open.
    #[test]
    fn flags_go_anywhere_and_the_build_is_the_one_bare_argument() {
        for line in [
            "/b --provider codex --json",
            "--provider codex --json /b",
            "--json /b --provider codex",
        ] {
            assert_eq!(
                parse(line).unwrap(),
                Args {
                    build: "/b".into(),
                    provider: ProviderId::Codex,
                    json: true
                },
                "{line}"
            );
        }
        assert_eq!(
            parse("/b").unwrap(),
            Args {
                build: "/b".into(),
                provider: ProviderId::Claude,
                json: false
            }
        );
        assert_eq!(
            parse("--provider desktop /a/app.asar").unwrap(),
            Args {
                build: "/a/app.asar".into(),
                provider: ProviderId::Desktop,
                json: false
            }
        );
    }

    #[test]
    fn what_cannot_be_read_says_why_and_exits_2() {
        for line in ["--provider", "/b --provider nothing", "--json", "/a /b"] {
            let refused = parse(line).unwrap_err();
            assert_eq!(refused.exit_code(), 2, "{line}");
        }
        let unknown = parse("/b --provider nothing").unwrap_err().to_string();
        assert!(unknown.contains("codex"), "{unknown}");
    }
}
