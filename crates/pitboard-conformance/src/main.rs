//! Check what pitboard believes about Claude Code against a Claude Code build.
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
//! pitboard-conformance <path to a claude binary> [--json]
//! ```

use pitboard_core::assumptions::{self, Reading};
use pitboard_core::provider::ProviderId;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let as_json = args.iter().any(|a| a == "--json");
    let Some(path) = args
        .iter()
        .skip_while(|a| *a != "--provider")
        .nth(2)
        .or_else(|| args.iter().find(|a| !a.starts_with("--")))
    else {
        eprintln!("usage: pitboard-conformance <path to a binary> [--provider claude] [--json]");
        return ExitCode::from(2);
    };

    // Which provider's register to check this build against. A build of one tool says
    // nothing about another's facts, so the two are never mixed in one run.
    let provider = match args.iter().position(|a| a == "--provider") {
        Some(at) => match args.get(at + 1).and_then(|name| ProviderId::parse(name)) {
            Some(provider) => provider,
            None => {
                eprintln!("--provider takes one of: claude");
                return ExitCode::from(2);
            }
        },
        None => ProviderId::Claude,
    };

    let bytes = match std::fs::read(path) {
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
            "{path}\npitboard's facts were read from Claude Code {}\n",
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
