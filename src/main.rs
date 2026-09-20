use pitboard::{doctor, state, status, switch};

const USAGE: &str = "\
pitboard — park and restore your own Claude Code logins

  pitboard status          what is signed in, and how much of it is left  (default)
  pitboard enroll <label>  remember the account signed in now, so it can be parked
  pitboard use <label>     sign in as an enrolled account
  pitboard forget <label>  drop an account and its parked credentials
  pitboard doctor          check that pitboard's model of Claude Code still holds

Options
  --json                   machine-readable output
";

/// 0 success · 1 failure · 2 wrong usage · 3 an assumption about Claude Code broke.
fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let json = args.iter().any(|a| a == "--json");
    let positional: Vec<&str> = args
        .iter()
        .filter(|a| !a.starts_with('-'))
        .map(String::as_str)
        .collect();

    match positional.split_first() {
        None | Some((&"status", _)) => cmd_status(json),
        Some((&"doctor", _)) => cmd_doctor(json),
        Some((&"enroll", rest)) => match rest.first() {
            Some(label) => cmd_enroll(label),
            None => misuse("enroll needs a label, for example `pitboard enroll personal`"),
        },
        Some((&"use", rest)) => match rest.first() {
            Some(label) => cmd_use(label),
            None => misuse("use needs a label, for example `pitboard use work`"),
        },
        Some((&"forget", rest)) => match rest.first() {
            Some(label) => cmd_forget(label),
            None => misuse("forget needs a label"),
        },
        Some((&"version", _)) => {
            println!("{} {}", env!("CARGO_BIN_NAME"), env!("CARGO_PKG_VERSION"));
            std::process::ExitCode::SUCCESS
        }
        Some((&"help", _)) => {
            print!("{USAGE}");
            std::process::ExitCode::SUCCESS
        }
        Some((other, _)) => misuse(&format!("unknown command `{other}`")),
    }
}

fn misuse(message: &str) -> std::process::ExitCode {
    eprintln!("{}: {message}\n", env!("CARGO_BIN_NAME"));
    eprint!("{USAGE}");
    std::process::ExitCode::from(2)
}

fn failed(message: impl std::fmt::Display) -> std::process::ExitCode {
    eprintln!("{}: {message}", env!("CARGO_BIN_NAME"));
    std::process::ExitCode::FAILURE
}

fn cmd_status(json: bool) -> std::process::ExitCode {
    let report = status::gather();
    let accounts = state::load()
        .map(|s| (s.accounts, s.active))
        .unwrap_or_default();
    if json {
        println!(
            "{}",
            status::render_json(&report, &accounts.0, accounts.1.as_deref())
        );
    } else {
        print!(
            "\n{}",
            status::render_human(&report, &accounts.0, accounts.1.as_deref())
        );
    }
    std::process::ExitCode::SUCCESS
}

fn cmd_doctor(json: bool) -> std::process::ExitCode {
    let checks = doctor::run();
    if json {
        println!("{}", doctor::render_json(&checks));
    } else {
        print!("\n{}", doctor::render_human(&checks));
    }
    if checks.iter().any(|c| c.level == doctor::Level::Fail) {
        std::process::ExitCode::from(3)
    } else {
        std::process::ExitCode::SUCCESS
    }
}

fn cmd_enroll(label: &str) -> std::process::ExitCode {
    match switch::enroll_current(label) {
        Ok(account) => {
            println!("enrolled {} as `{label}`", account.email);
            std::process::ExitCode::SUCCESS
        }
        Err(e) => failed(e),
    }
}

fn cmd_use(label: &str) -> std::process::ExitCode {
    match switch::switch(label) {
        Ok(outcome) => {
            println!(
                "signed in as `{}`, parked `{}` at {}",
                outcome.to, outcome.from, outcome.parked.service
            );
            println!(
                "a Claude Code session already running picks this up within {} seconds",
                switch::ADOPTION_CEILING_SECONDS
            );
            if let Some(warning) = outcome.config_warning {
                eprintln!(
                    "note: the credential moved but the config did not ({warning}); \
                     Claude Code corrects this on its next call"
                );
            }
            for service in outcome.stuck_generations {
                eprintln!("note: {service} could not be removed from the keychain");
            }
            std::process::ExitCode::SUCCESS
        }
        Err(e) => failed(e),
    }
}

fn cmd_forget(label: &str) -> std::process::ExitCode {
    let mut state = match state::load() {
        Ok(s) => s,
        Err(e) => return failed(e),
    };
    let Some(index) = state.accounts.iter().position(|a| a.label == label) else {
        return failed(format!("no account is enrolled as `{label}`"));
    };
    if state.active.as_deref() == Some(label) {
        return failed(format!(
            "`{label}` is signed in; switch to another account first"
        ));
    }
    let account = state.accounts.remove(index);
    let mut kept = 0;
    for generation in &account.generations {
        if pitboard::store::delete(&generation.service).is_err() {
            kept += 1;
        }
    }
    match state::save(&state) {
        Ok(()) => {
            println!("forgot `{label}` ({})", account.email);
            if kept > 0 {
                eprintln!(
                    "note: {kept} parked credential(s) could not be removed from the keychain"
                );
            }
            std::process::ExitCode::SUCCESS
        }
        Err(e) => failed(e),
    }
}
