//! Check what pitboard believes about a coding tool against a build of that tool.
//!
//! Every load-bearing fact in pitboard was read out of one build and lives in
//! [`pitboard_core::assumptions`] with the literals it is readable by. This reads those
//! literals out of a binary and says which ones are still there.
//!
//! What it is: a cheap, shallow drift alarm. A literal being present does not prove the
//! behaviour around it is unchanged, and this never says it does. A literal disappearing
//! does prove something moved, which is the only thing worth waking somebody for.
//!
//! What it is not: a test of pitboard against a running Claude Code. That needs a real
//! sign-in and a real keychain and cannot run unattended.
//!
//! Measured across six builds while it was written. The probe set holds from 2.1.273
//! through 2.1.278, and correctly goes red on 2.1.124, which predates the credential write
//! lock, two of the five account-scoped keys, and the keychain error classification. So it
//! reports real change rather than noise, and would have reported those the week they
//! landed.
//!
//! ```text
//! pitboard-conformance <path to a binary> [--provider claude|codex] [--json]
//! ```
//!
//! Each tool has its own register, and a build of one tool says nothing about another's
//! facts, so a run checks one tool's build against that tool's register. Claude Code is
//! the default, which is what every run before there was a second tool meant.

use pitboard_core::assumptions::{self, Reading};
use pitboard_core::provider::ProviderId;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (path, provider, as_json) = match parse(&args) {
        Ok(parsed) => parsed,
        Err(problem) => {
            eprintln!("{problem}");
            eprintln!(
                "usage: pitboard-conformance <path to a binary> [--provider {}] [--json]",
                known().join("|")
            );
            return ExitCode::from(2);
        }
    };

    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("cannot read {path}: {e}");
            return ExitCode::from(2);
        }
    };
    let strings = assumptions::printable_runs(&bytes, 6);

    let readings: Vec<(&assumptions::Assumption, Reading)> = assumptions::of(provider)
        .iter()
        .map(|a| (a, assumptions::read_from_build(a, &strings)))
        .collect();
    // Both kinds of drift count. A fact that rested on a keyring backend not existing is
    // as broken by one arriving as a service name is by being renamed.
    let moved: Vec<&assumptions::Assumption> = readings
        .iter()
        .filter(|(_, r)| matches!(r, Reading::Moved(_) | Reading::Appeared(_)))
        .map(|(a, _)| *a)
        .collect();

    if as_json {
        let report = serde_json::json!({
            "build": path,
            "verified_against": assumptions::verified_against(provider),
            "assumptions": readings.iter().map(|(a, r)| serde_json::json!({
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
                "verified_against": a.verified_against,
                "depends": a.depends,
            })).collect::<Vec<_>>(),
            "moved": moved.iter().map(|a| a.name).collect::<Vec<_>>(),
        });
        println!("{report}");
    } else {
        println!(
            "{path}\npitboard's facts about {} were read from {}\n",
            provider.code(),
            assumptions::verified_against(provider)
        );
        for (a, reading) in &readings {
            match reading {
                Reading::Holds => println!("  ok       {}", a.name),
                Reading::NotReadable => {
                    println!(
                        "  no probe {}  (a fact about behaviour, not a name)",
                        a.name
                    );
                }
                Reading::Moved(gone) => {
                    println!("  MOVED    {}", a.name);
                    for needle in gone {
                        println!("             gone: {needle}");
                    }
                    println!("             this breaks: {}", a.depends);
                }
                Reading::Appeared(found) => {
                    println!("  APPEARED {}", a.name);
                    for needle in found {
                        println!("             now present: {needle}");
                    }
                    println!("             this breaks: {}", a.depends);
                }
            }
        }
        println!();
        if moved.is_empty() {
            println!("Everything pitboard can read from a build is still there.");
        } else {
            println!(
                "{} of pitboard's facts can no longer be read from this build. Re-measure \
                 them against it before trusting a switch.",
                moved.len()
            );
        }
    }

    if moved.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// Every tool with a register, as `--provider` takes it.
fn known() -> Vec<&'static str> {
    ProviderId::ALL.iter().map(|p| p.code()).collect()
}

/// The build to read, the tool whose register to read it against, and whether to answer in
/// JSON. Flags go anywhere; the one argument that is not a flag or a flag's value is the
/// build.
fn parse(args: &[String]) -> Result<(String, ProviderId, bool), String> {
    let mut path = None;
    let mut provider = ProviderId::Claude;
    let mut as_json = false;
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "--json" => as_json = true,
            "--provider" => {
                let named = rest.next().ok_or("--provider needs a tool")?;
                provider = ProviderId::parse(named)
                    .ok_or_else(|| format!("--provider takes one of: {}", known().join(", ")))?;
            }
            flag if flag.starts_with("--") => return Err(format!("unknown flag {flag}")),
            build if path.is_none() => path = Some(build.to_string()),
            extra => return Err(format!("one build at a time, not also {extra}")),
        }
    }
    let path = path.ok_or("no build to read")?;
    Ok((path, provider, as_json))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(text: &str) -> Vec<String> {
        text.split_whitespace().map(str::to_owned).collect()
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
                parse(&args(line)).unwrap(),
                ("/b".into(), ProviderId::Codex, true),
                "{line}"
            );
        }
        assert_eq!(
            parse(&args("/b")).unwrap(),
            ("/b".into(), ProviderId::Claude, false)
        );
    }

    #[test]
    fn what_cannot_be_read_says_why() {
        assert!(parse(&args("--provider")).is_err());
        assert!(
            parse(&args("/b --provider nothing"))
                .unwrap_err()
                .contains("codex")
        );
        assert!(parse(&args("--json")).is_err());
        assert!(parse(&args("/a /b")).is_err());
    }
}
