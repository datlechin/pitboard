use super::parks::{ParkFact, judge_dormant, judge_park};
use super::{Check, Facts, fail, ok, read_from, warn, words};
use crate::context::Context;
use crate::host::{Access, Kind, Os};
use crate::provider::ProviderId;
use crate::provider::codex::paths as codex;
use crate::state::State;
use crate::words as said;
use std::path::PathBuf;

// Read without running Codex, and without touching a keychain item it created for itself.
pub(crate) struct CodexFacts {
    pub(crate) home: PathBuf,
    // The home is there once Codex has run here.
    pub(crate) present: bool,
    pub(crate) enrolled: usize,
    pub(crate) backend: codex::Backend,
    // What chose the store, or for `Unknown` what cannot be read.
    pub(crate) backend_setting: String,
    // What would choose the file store, with no full stop; empty for the file.
    pub(crate) backend_remedy: String,
    pub(crate) auth_file: PathBuf,
    pub(crate) auth_access: Option<Access>,
    // `Ok(None)` for no file, and for a store Pitboard does not read.
    pub(crate) login: Result<Option<CodexLogin>, CodexLoginTrouble>,
    pub(crate) program: Option<PathBuf>,
    // `None` where there is no `codex`, and where its path does not say.
    pub(crate) version: Option<String>,
    // `None` where the process list could not be read.
    pub(crate) running: Option<Vec<crate::holder::Holding>>,
    // Codex's `cli|` and `secrets|` items, counted by name (`codex_windows_stores`).
    pub(crate) stored_elsewhere: (usize, usize),
    // Each setting that limits which logins Codex keeps, and where.
    pub(crate) restrictions: Vec<String>,
    pub(crate) restrictions_unreadable: bool,
    // The enrolled accounts whose logins Codex would sign out or leave unused.
    pub(crate) ruled_out: Vec<String>,
}

// As its own ID token says. The fingerprint is a handle on the refresh token, never the token.
pub(crate) struct CodexLogin {
    pub(crate) email: String,
    pub(crate) account_id: String,
    // Empty where the login holds no refresh token.
    pub(crate) fingerprint: String,
}

// A login of no account, as with an API key, is a choice; one nobody can read is a fault.
#[derive(Debug)]
pub(crate) enum CodexLoginTrouble {
    NotAnAccount(String),
    // Unreadable, or not one account's login.
    Unusable(String),
    // `CODEX_HOME` names this, which is no folder, so Codex does not start: Windows' alone.
    NoHome(PathBuf),
}

pub(super) fn codex_facts(ctx: &Context, state: Option<&State>) -> CodexFacts {
    let store = codex::store(ctx);
    let backend = store.backend;
    let auth_file = codex::auth_file(ctx);
    let home = codex::home(ctx);
    // Where the context looks: an app started from Finder passes the login shell's `PATH`.
    let program = crate::provider::program_of(ctx, ProviderId::Codex);
    // A layer Codex cannot read is the store's line to say.
    let restrictions = (backend != codex::Backend::Unknown).then(|| codex::restrictions(ctx));
    let unreadable = restrictions
        .as_ref()
        .is_some_and(|found| found.unreadable());
    let ruled_out = state.filter(|_| !unreadable).map_or_else(Vec::new, |s| {
        s.accounts
            .iter()
            .filter(|a| a.provider() == ProviderId::Codex)
            .filter_map(|a| {
                let workspace = Some(a.owner().organization_uuid).filter(|id| !id.is_empty());
                restrictions
                    .as_ref()?
                    .refuses(workspace.as_deref(), workspace.as_deref())
                    .map(|_| format!("`{}`", s.typed(&a.key())))
            })
            .collect()
    });
    CodexFacts {
        present: home.is_dir(),
        enrolled: state.map_or(0, |s| {
            s.accounts
                .iter()
                .filter(|a| a.provider() == ProviderId::Codex)
                .count()
        }),
        backend,
        backend_setting: match backend {
            codex::Backend::Unknown => store.why_unknown().unwrap_or_default(),
            _ => store.setting(),
        },
        backend_remedy: match backend {
            codex::Backend::File => String::new(),
            codex::Backend::Unknown => store.while_unknown().unwrap_or_default(),
            _ => store.to_use_the_file(
                said::spelling(ctx.os, ProviderId::Codex, program.as_deref()).of(ProviderId::Codex),
            ),
        },
        auth_access: crate::host::fs::access(&auth_file),
        // A keychain item Codex made trusts `codex` alone: reading it would ask for permission.
        login: if backend == codex::Backend::File {
            codex_login(ctx)
        } else {
            Ok(None)
        },
        version: program.as_deref().and_then(codex::installed_version),
        program,
        running: running_codex(ctx),
        stored_elsewhere: if backend == codex::Backend::File {
            stored_elsewhere(ctx)
        } else {
            (0, 0)
        },
        restrictions: restrictions.map_or_else(Vec::new, |found| found.said()),
        restrictions_unreadable: unreadable,
        ruled_out,
        auth_file,
        home,
    }
}

// By name alone: a setting Pitboard does not read can still keep a session's login there.
pub(super) fn stored_elsewhere(ctx: &Context) -> (usize, usize) {
    use crate::provider::names::{Family, family};
    let count = |prefix: &str, of: Family| {
        ctx.host().credential_names(prefix).map_or(0, |names| {
            names.iter().filter(|name| family(name) == Some(of)).count()
        })
    };
    (
        count("cli|", Family::CodexCli),
        count("secrets|", Family::CodexSecrets),
    )
}

pub(super) fn codex_login(ctx: &Context) -> Result<Option<CodexLogin>, CodexLoginTrouble> {
    let tool = crate::provider::codex();
    let unusable = |e: crate::provider::ProviderError| CodexLoginTrouble::Unusable(e.to_string());
    let Some(credential) = tool.read_live(ctx).map_err(|e| match e {
        crate::provider::ProviderError::CodexHomeMissing { path } => {
            CodexLoginTrouble::NoHome(path)
        }
        other => unusable(other),
    })?
    else {
        return Ok(None);
    };
    // Whether it is one account's login, as a park asks.
    tool.slice(&credential.raw).map_err(|e| match e {
        crate::provider::ProviderError::Unsupported { reason, .. } => {
            CodexLoginTrouble::NotAnAccount(reason)
        }
        other => unusable(other),
    })?;
    let found = tool.identify(ctx, &credential).map_err(unusable)?;
    Ok(Some(CodexLogin {
        email: found.email,
        account_id: found.account_uuid,
        fingerprint: tool.fingerprint(&credential.raw),
    }))
}

// Asked as a switch asks, so the two cannot disagree.
pub(super) fn running_codex(ctx: &Context) -> Option<Vec<crate::holder::Holding>> {
    let crate::provider::Adoption::RestartRequired { program, holders } =
        crate::provider::codex().adoption(None)
    else {
        return Some(Vec::new());
    };
    crate::holder::find(ctx, ProviderId::Codex, program, holders)
}

// Codex's home is in the known folder Profile, install.ps1's under `USERPROFILE`.
pub(super) fn judge_codex_installer_home(facts: &Facts) -> Option<Check> {
    let windows = facts.windows.as_ref().filter(|w| !w.codex_home_set)?;
    let codex = |folder: &std::path::Path| folder.join(".codex");
    let profile = windows.profile.as_deref()?;
    (!Os::Windows.same_path(&codex(profile), &codex(&windows.home))).then(|| {
        warn(
            "codex_installer_home",
            "Codex's home",
            words::codex_homes_apart(codex(profile).display(), codex(&windows.home).display()),
            words::SET_CODEX_HOME,
        )
    })
}

// A fault fails where Codex accounts are enrolled; a choice fails only where it strands one.
pub(super) fn judge_codex(
    os: Os,
    sid: Option<&str>,
    facts: &CodexFacts,
    parks: &[&ParkFact],
    now: i64,
    moment: &dyn Fn(i64, i64) -> String,
) -> Vec<Check> {
    let mut checks = Vec::new();
    let enrolled = facts.enrolled > 0 || !parks.is_empty();
    let program =
        said::spelling(os, ProviderId::Codex, facts.program.as_deref()).of(ProviderId::Codex);
    let broken = |code, name: &str, detail: String, advice: String| {
        if enrolled {
            fail(code, name, detail, advice)
        } else {
            warn(code, name, detail, advice)
        }
    };
    let file = facts.backend == codex::Backend::File;
    let setting = &facts.backend_setting;
    let remedy = &facts.backend_remedy;
    let described = match facts.backend {
        codex::Backend::Ephemeral => words::in_memory_only(setting),
        codex::Backend::Unknown => words::cannot_tell_the_store(setting),
        other => words::store_set_by(other.word(), setting),
    };
    checks.push(match facts.backend {
        codex::Backend::File => Check {
            advice: words::PROJECTS_NOT_READ.into(),
            ..ok(
                "codex_backend",
                "Codex login store",
                words::file_store(setting, facts.auth_file.display()),
            )
        },
        // Not a choice: a layer Codex reads stops it from starting.
        codex::Backend::Unknown => broken(
            "codex_backend",
            "Codex login store",
            described,
            words::no_store_known(remedy),
        ),
        // A choice, and one that leaves nothing Pitboard can park or switch.
        other if enrolled => fail(
            "codex_backend",
            "Codex login store",
            described,
            match other {
                codex::Backend::Ephemeral => words::nothing_at_rest(remedy),
                _ => words::no_account_to_switch_to(
                    crate::provider::codex::layers::file_store_only(os),
                    remedy,
                ),
            },
        ),
        _ => ok(
            "codex_backend",
            "Codex login store",
            words::switched_only_in_the_file(&described),
        ),
    });

    if !facts.restrictions.is_empty() {
        let name = "Codex login restrictions";
        let restrictions = facts.restrictions.join("; ");
        checks.push(if facts.restrictions_unreadable {
            broken(
                "codex_login_restrictions",
                name,
                restrictions,
                words::RESTRICTIONS_UNREADABLE.into(),
            )
        } else if facts.ruled_out.is_empty() {
            ok("codex_login_restrictions", name, restrictions)
        } else {
            warn(
                "codex_login_restrictions",
                name,
                restrictions,
                words::codex_rules_out(&said::listed(facts.ruled_out.clone())),
            )
        });
    }

    // A keychain store deletes the file on purpose.
    if file {
        let auth_file = facts.auth_file.display();
        checks.push(match &facts.auth_access {
            None if enrolled => warn(
                "codex_auth_file",
                "Codex login file",
                words::codex_file_absent(auth_file),
                words::use_or_sign_in(program),
            ),
            None => ok("codex_auth_file", "Codex login file", words::NO_CODEX_FILE),
            Some(access) if access.shared => warn(
                "codex_auth_file",
                "Codex login file",
                words::open_to(&auth_file, access),
                words::codex_file_open(
                    os,
                    &os.make_private_commands(Kind::File, &[&auth_file.to_string()], sid),
                ),
            ),
            Some(access) => ok(
                "codex_auth_file",
                "Codex login file",
                words::codex_file(auth_file, access),
            ),
        });
        match &facts.login {
            Ok(Some(login)) => checks.push(ok(
                "codex_login",
                "Codex login",
                words::codex_login(
                    &login.email,
                    &login.account_id,
                    if login.fingerprint.is_empty() {
                        words::NO_TOKEN
                    } else {
                        &login.fingerprint
                    },
                ),
            )),
            // No file, which the check above has already said.
            Ok(None) => {}
            // Signed in on purpose some way Pitboard does not switch: nothing is broken.
            Err(CodexLoginTrouble::NotAnAccount(why)) if enrolled => checks.push(warn(
                "codex_login",
                "Codex login",
                why.clone(),
                words::chatgpt_sign_ins_only(program),
            )),
            Err(CodexLoginTrouble::NotAnAccount(why)) => {
                checks.push(ok("codex_login", "Codex login", why.clone()));
            }
            Err(CodexLoginTrouble::Unusable(why)) => checks.push(broken(
                "codex_login",
                "Codex login",
                words::cannot_be_used(why),
                words::sign_in_writes_a_fresh_one(program),
            )),
            Err(CodexLoginTrouble::NoHome(path)) => checks.push(broken(
                "codex_login",
                "Codex login",
                crate::error::codex_home_no_folder(path),
                format!("{}.", crate::error::CODEX_HOME_REMEDY),
            )),
        }
    }

    let (cli, secrets) = facts.stored_elsewhere;
    if file && cli + secrets > 0 {
        let items = [(cli, "`cli|`"), (secrets, "`secrets|`")]
            .into_iter()
            .filter(|&(count, _)| count > 0)
            .map(|(count, prefix)| words::items(count, prefix));
        checks.push(Check {
            advice: words::CODEX_KEEPS_IT_THERE_TOO.into(),
            ..ok(
                "codex_credential_manager",
                "Codex in Credential Manager",
                said::listed(items),
            )
        });
    }

    checks.extend(parks.iter().map(|p| judge_park(p, now, moment)));
    checks.extend(parks.iter().filter_map(|p| judge_dormant(p, now)));

    checks.push(judge_codex_version(os, facts));
    checks.push(judge_codex_running(facts.running.as_deref()));
    checks
}

// Never warned about: running it is the point of having it.
pub(super) fn judge_codex_running(running: Option<&[crate::holder::Holding]>) -> Check {
    let (detail, advice) = match running {
        None => (words::COULD_NOT_TELL.to_string(), String::new()),
        Some([]) => (words::NONE_RUNNING.to_string(), String::new()),
        Some(holding) => (
            words::each_keeps_its_account(&crate::holder::described_with_pids(holding)),
            crate::holder::remedies(holding, "to take a switch"),
        ),
    };
    Check {
        advice,
        ..ok("codex_running", "running Codex", detail)
    }
}

pub(super) fn judge_codex_version(os: Os, facts: &CodexFacts) -> Check {
    let verified = read_from(ProviderId::Codex, os);
    ok(
        "codex_version",
        "Codex build",
        match (facts.program.as_deref(), facts.version.as_deref()) {
            (None, _) => words::build_not_found(verified),
            // Behind a version manager's shim, say: found is not missing.
            (Some(program), None) => words::build_path_says_nothing(program.display(), verified),
            (Some(_), Some(installed)) if installed == verified => words::build_as_read(installed),
            (Some(_), Some(installed)) => words::build_installed(installed, verified),
        },
    )
}
