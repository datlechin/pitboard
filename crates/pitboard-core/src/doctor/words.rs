use super::network::ProxyFact;
use crate::host::task::Drift;
use crate::host::{Access, Os};
use crate::words as said;
use std::fmt::Display;

// Each command to run, in backticks, or `None` where Pitboard can say none.
pub(super) fn commands(said: &[String]) -> Option<String> {
    match said {
        [] => None,
        [one] => Some(format!("`{one}`")),
        [all @ .., last] => Some(format!("`{}` and `{last}`", all.join("`, `"))),
    }
}

// The end of a sentence of advice, from `: ` to its full stop.
pub(super) fn running(said: &[String]) -> String {
    commands(said).map_or_else(|| ".".into(), |commands| format!(": {commands}."))
}

pub(super) fn open_to(path: impl Display, access: &Access) -> String {
    format!("{path} is {}", access.described)
}

pub(super) fn changes_nothing_this_way(way_out: &str) -> String {
    format!("Pitboard changes nothing this way: it reads, and renews and writes nothing. {way_out}")
}

pub(super) fn changes_nothing_here(way_out: &str) -> String {
    format!("Pitboard changes nothing here: it reads, and renews and writes nothing. {way_out}")
}

pub(super) fn missing(tool: &str) -> String {
    format!("{tool} is missing")
}

pub(super) const READS_THE_KEYCHAIN_AS_CLAUDE_CODE: &str =
    "Pitboard reads the keychain the same way Claude Code does. Without it, nothing works.";

pub(super) fn not_created_yet(home: impl Display) -> String {
    format!("{home} (not created yet)")
}

pub(super) fn only_you_should_read_the_home(commands: &[String]) -> String {
    format!(
        "Park names contain account identifiers, so only you should read it{}",
        running(commands)
    )
}

pub(super) const NOTHING_READABLE_BY_OTHERS: &str =
    "nothing holding a login is readable by anyone else";

pub(super) fn others_can_read(os: Os, commands: &[String]) -> String {
    let these = match os {
        Os::MacOs | Os::Linux => {
            "These hold usable OAuth tokens in plain text, which is how Claude Code stores them \
             where there is no keychain. Anyone else on this machine can read them"
        }
        Os::Windows => {
            "These hold a login: Claude Code's file in plain text, a sealed park only this \
             account opens. Claude Code's file takes the access list of the folder it is in, and \
             a seal stops reading, not writing. Anyone else on this PC can read or change them"
        }
    };
    format!("{these}{}", running(commands))
}

pub(super) fn enrolled(accounts: usize) -> String {
    match accounts {
        0 => "none enrolled yet".to_string(),
        n => format!("{n} enrolled"),
    }
}

pub(super) const ADOPT_TO_KEEP_THEM: &str = "Run `pitboard adopt` to keep these accounts on this \
     computer. The logins they came with are dropped, because a login belongs to the computer \
     that signed in.";

pub(super) const NO_SWITCH_UNTIL_READABLE: &str =
    "Pitboard will not switch until its account list can be read.";

pub(super) const ABANDON_KEEPS_EVERY_LOGIN: &str = "If it still cannot be finished, `pitboard \
     abandon` gives up on the switch and keeps every login.";

pub(super) const A_SWITCH_DID_NOT_FINISH: &str = "a switch did not finish";

pub(super) const THE_NEXT_CHANGE_FINISHES_IT: &str =
    "The next `pitboard use`, `enroll` or `forget` finishes it before anything else.";

pub(super) fn waiting_to_be_deleted(vault: usize, folders: usize) -> String {
    match (vault, folders) {
        (vault, 0) => format!("{vault} waiting to be deleted"),
        (0, folders) => format!("{folders} folder(s) waiting to be deleted"),
        (vault, folders) => format!("{vault} waiting to be deleted, and {folders} folder(s)"),
    }
}

pub(super) const DELETED_ON_THE_NEXT_CHANGE: &str = "Pitboard deletes them on its next change.";

pub(super) const DELETED_ONCE_THE_KEYCHAIN_OPENS: &str = "Pitboard deletes them on its next \
     change; if they stay, check the keychain is unlocked.";

pub(super) const NO_STABLE_IDENTIFIER: &str = "this machine has no stable identifier";

pub(super) fn never_copy_between_machines(home: impl Display) -> String {
    format!(
        "Pitboard cannot tell this machine from another that also lacks one, so it cannot \
         refuse state copied between them. Never copy {home} between machines."
    )
}

pub(super) const MACHINE_GUID: &str = "Windows' MachineGuid";

pub(super) const A_COPY_KEEPS_THE_MACHINE_GUID: &str = "Pitboard tells this PC apart by it, and \
     a copy of the PC keeps it, such as a virtual machine copied whole. Such a copy takes this \
     PC's accounts for its own, and both renew the same parked logins: on a copy, forget each \
     account and enrol it again with `--sign-in`.";

pub(super) const NOTHING_HELD_BACK: &str = "nothing is being held back";

pub(super) fn not_asked_about(accounts: usize, longest: i64) -> String {
    format!(
        "{accounts} account(s) not being asked about for up to {}",
        said::span(longest)
    )
}

pub(super) fn held_back_by(services: &str) -> String {
    format!(
        "{services} asked for less traffic, or could not be reached. The numbers shown are the \
         last ones measured until then; `pitboard status --fresh` does not override a wait \
         {services} asked for."
    )
}

pub(super) const NONE_OUTSTANDING: &str = "none outstanding";

pub(super) fn could_not_be_read_this_time(names: usize) -> String {
    format!("{names} could not be read this time")
}

pub(super) const UNLOCK_AND_RUN_ANY_COMMAND: &str = "Pitboard wrote these names down before \
     creating a login in them and cannot read them back to find out what is there. Unlock the \
     keychain and run any Pitboard command; it resolves them before doing anything else.";

pub(super) fn not_switched_to_for(seconds: i64) -> String {
    format!(
        "not switched to for {}; Pitboard has kept its login alive that whole time",
        said::span(seconds)
    )
}

pub(super) fn forget_if_not_coming_back(typed: &str) -> String {
    format!(
        "Every `pitboard` renews it, so its refresh token is rotated and kept live on this \
         machine for as long as it stays enrolled. If you are not coming back to it, `pitboard \
         forget {typed}` deletes the login and the record."
    )
}

pub(super) const SEALED_TO_THIS_ACCOUNT: &str = "sealed to this Windows account, not plain text";

pub(super) fn enroll_signing_in(typed: &str) -> String {
    format!("pitboard enroll {typed} --sign-in")
}

pub(super) fn sign_in_again(typed: &str) -> String {
    format!("Run `{}`.", enroll_signing_in(typed))
}

pub(super) fn none_opens(parked: usize) -> String {
    format!("none of the {parked} parked logins opens for this Windows account")
}

pub(super) fn sealed_by_a_lost_key(commands: &[String]) -> String {
    format!(
        "An administrator's reset of your Windows password, or a profile restored on another \
         PC, leaves them sealed with a key this account no longer has. Sign in to each again{}",
        running(commands)
    )
}

pub(super) fn not_open_in_this_logon(commands: &[String]) -> String {
    format!(
        "They do not open in this logon. Run `pitboard doctor` from a desktop session before \
         signing in again{}",
        running(commands)
    )
}

pub(super) const MISSING_FROM_THE_VAULT: &str = "missing from the vault";

pub(super) const SIGNED_IN_PARKED_ON_A_SWITCH: &str = "signed in; parked when you switch away";

pub(super) const NOTHING_PARKED: &str = "nothing parked to switch to";

pub(super) const NOT_READ_THE_KEYCHAIN_IS_LOCKED: &str = "not read: the keychain is locked";

pub(super) const UNLOCK_IT_THEN_DOCTOR: &str = "Unlock it, then run `pitboard doctor` again.";

pub(super) const PARK_DOES_NOT_OPEN: &str = "its parked login does not open in this logon";

pub(super) fn park_unusable(why: &str) -> String {
    format!("its parked login is unusable: {why}")
}

pub(super) const PARKED: &str = "parked";

pub(super) fn parked_with(life: &str) -> String {
    format!("parked, {life}")
}

pub(super) fn park_life(life: &str) -> String {
    format!("its parked login {life}")
}

pub(super) fn park_life_since(life: &str, when: &str) -> String {
    format!("its parked login {life} {when}")
}

pub(super) fn org_pinned(ids: &str, layer: impl Display) -> String {
    format!("`forceLoginOrgUUID` set to {ids} in {layer}")
}

pub(super) fn run_once(program: &str) -> String {
    format!("Run `{program}` once.")
}

pub(super) fn install_then_run_once(program: &str) -> String {
    format!("Install Claude Code, then run `{program}` once.")
}

pub(super) fn config_read(path: impl Display, keys: usize) -> String {
    format!("{path}  ({keys} keys)")
}

pub(super) fn not_installed_and_no_config(path: impl Display) -> String {
    format!("Claude Code is not installed where Pitboard looks, and {path} does not exist")
}

pub(super) fn identity(email: &str, organization: &str) -> String {
    format!("{email}  ·  org {organization}")
}

pub(super) const NO_ACCOUNT_RECORDED: &str = "Claude Code has not recorded a signed-in account";

pub(super) fn written_after_the_first_call(run_once: &str) -> String {
    format!("It writes this after its first successful call. {run_once}")
}

pub(super) fn default_slot(service: &str, account: &str) -> String {
    format!("default  ·  {service}  ·  account {account}")
}

pub(super) fn slot_selected_by(service: &str, account: &str, storage_dir: &str) -> String {
    format!("{service}  ·  account {account}  (selected by {storage_dir})")
}

// The three things taking up the most room, as the end of a size's detail.
pub(super) fn biggest(parts: &[(String, usize)]) -> String {
    let named: Vec<String> = parts
        .iter()
        .take(3)
        .map(|(what, bytes)| format!("{what} {bytes}"))
        .collect();
    if named.is_empty() {
        String::new()
    } else {
        format!(". Mostly: {}", named.join(", "))
    }
}

pub(super) fn size_refused(bytes: usize, limit: usize, biggest: &str) -> String {
    format!("{bytes} of {limit} bytes, and PITBOARD_NO_ARGV refuses that{biggest}")
}

pub(super) const NO_THIRD_WAY: &str = "There is no third way to write a login this size. Sign out \
     of MCP servers you no longer use to make it smaller, or unset PITBOARD_NO_ARGV and accept \
     the argument-line write.";

pub(super) fn size_on_the_argument_line(bytes: usize, limit: usize, biggest: &str) -> String {
    format!("{bytes} of {limit} bytes: written on the argument line{biggest}")
}

pub(super) const ONLY_AS_AN_ARGUMENT: &str = "A login this size can only be written by passing it \
     as an argument, where a process running as you could read it while the call lasts. Signing \
     out of MCP servers you no longer use makes it smaller; PITBOARD_NO_ARGV=1 refuses the switch \
     instead.";

pub(super) fn size(bytes: usize, limit: usize) -> String {
    format!("{bytes} of {limit} bytes")
}

pub(super) const NEITHER_CAN_WRITE_IT: &str = "Neither Pitboard nor Claude Code can write a login \
     this size there. Sign out of MCP servers you no longer use to make it smaller.";

pub(super) fn in_one_item(bytes: usize) -> String {
    format!("{bytes} bytes, in one item")
}

pub(super) fn in_pieces(bytes: usize, pieces: usize, piece: usize) -> String {
    format!("{bytes} bytes, in {pieces} pieces of {piece} characters")
}

pub(super) const NO_ARGV_CHANGES_NOTHING: &str = "set, and it changes nothing on Windows";

pub(super) const UNSET_NO_ARGV: &str = "Claude Code's Credential Manager store takes a login of \
     any size it writes in pieces, never on an argument line. Unset it.";

pub(super) const KEYCHAIN: &str = "keychain";

pub(super) fn in_credential_manager(service: &str, account: &str) -> String {
    format!(
        "Credential Manager  ·  {service}/{account}  ·  every logon of yours on this PC reads it"
    )
}

pub(super) const ROAMS_WITH_THE_PROFILE: &str = "Your profile roams, and Credential Manager takes \
     Claude Code's login with it to every PC you sign in to, so a switch here changes the login \
     there too.";

pub(super) fn plaintext_file(path: impl Display) -> String {
    format!("plaintext file  ·  {path}")
}

pub(super) const FELL_BACK_TO_A_FILE: &str =
    "Claude Code fell back to a file, which means a keychain write failed at some point.";

pub(super) const NOT_READ_YET: &str = "a store Pitboard does not read yet";

pub(super) const UNKNOWN_NOT_EMPTY: &str = "Treat this as unknown, never as empty.";

pub(super) const UNKNOWN_AND_SIGNED_OUT: &str = "Treat this as unknown, never as empty. While \
     nothing turns Claude Code's Credential Manager store on, its sessions read only \
     .credentials.json, so a login left only in Credential Manager is signed out to them.";

pub(super) const NOWHERE_ANYBODY_READS: &str = "Pitboard will not write a login where nobody \
     reads it. Check for a Pitboard update; if there is none, this is worth reporting.";

pub(super) const NO_CREDENTIAL_ANYWHERE: &str = "no credential in any backend";

pub(super) const NOTHING_SIGNED_IN: &str = "Nothing is signed in for this slot.";

pub(super) const ASK_WITH_STATUS: &str =
    "Run `pitboard status`, which asks Anthropic whose login Claude Code has stored.";

pub(super) const NEVER_ASKED: &str =
    "Anthropic has not been asked whose login Claude Code has stored";

pub(super) fn anthropic_named(in_use: &str, when: &str) -> String {
    format!("Anthropic named {in_use} {when}")
}

pub(super) fn found_none_stored(when: &str) -> String {
    format!("Pitboard found no login stored {when}")
}

pub(super) fn not_asked_since_an_update(in_use: &str) -> String {
    format!(
        "Anthropic has not been asked whose login Claude Code has stored since Pitboard was \
         updated; {in_use} is the account it last switched to"
    )
}

pub(super) fn config_named_since(config: &str, then: &str) -> String {
    format!("Claude Code's config has named {config} since {then}, so another login may be stored")
}

pub(super) fn config_names_another(in_use: &str, when: &str, config: &str) -> String {
    format!("{in_use}, as Anthropic said {when}, and Claude Code's config names {config}")
}

pub(super) fn status_shows(config: &str, how_to_name: &str) -> String {
    format!("`/status` in Claude Code shows {config}. {how_to_name}")
}

const THE_NEXT_STATUS_ASKS: &str = "the next `pitboard status` asks whose it is";

pub(super) fn renewed_since(in_use: &str, when: &str) -> String {
    format!(
        "{in_use}, as Anthropic said {when} of a login renewed or replaced since; \
         {THE_NEXT_STATUS_ASKS}"
    )
}

pub(super) fn none_stored_since(in_use: &str, when: &str) -> String {
    format!("{in_use}, as Anthropic said {when}; no login is stored since")
}

pub(super) fn as_anthropic_said(in_use: &str, when: &str) -> String {
    format!("{in_use}, as Anthropic said {when}")
}

pub(super) fn one_stored_since(when: &str) -> String {
    format!("no login stored, as Pitboard found {when}; one is since, and {THE_NEXT_STATUS_ASKS}")
}

pub(super) fn none_stored(when: &str) -> String {
    format!("no login stored, as Pitboard found {when}")
}

pub(super) fn signed_out_keeps(keys: &[&str]) -> String {
    format!("signed out; the document keeps only {keys:?}")
}

pub(super) const SHAPE_CHANGED: &str =
    "The credential's shape changed. Do not switch accounts until this is understood.";

pub(super) const NOT_A_JSON_OBJECT: &str = "the stored login is not a JSON object";

pub(super) fn oauth_not_an_object(keys: &[&str]) -> String {
    format!("claudeAiOauth is not an object; the keys are {keys:?}")
}

// The fingerprint of a login with no refresh token.
pub(super) const NO_TOKEN: &str = "none";

pub(super) fn refresh_and_keys(fingerprint: &str, keys: &[&str]) -> String {
    format!("refresh {fingerprint}  ·  keys {keys:?}")
}

pub(super) fn refresh_life_and_keys(fingerprint: &str, life: &str, keys: &[&str]) -> String {
    format!("refresh {fingerprint}  ·  {life}  ·  keys {keys:?}")
}

pub(super) const EXPIRED: &str = "This login has expired: sign in to Claude Code again.";

pub(super) const EXPIRES_SOON: &str = "This login expires soon and will need signing in again.";

pub(super) const NOTHING_STORED: &str = "nothing stored";

pub(super) const CREDENTIAL_MANAGER_OUT_OF_REACH: &str =
    "Credential Manager cannot be reached from this logon";

pub(super) fn from_a_desktop_session(file: impl Display) -> String {
    format!(
        "A network logon, as {}, has no Credential Manager. Run Pitboard from a desktop session. \
         A Claude Code session started from this logon reads only {file}.",
        said::SSH_SIGN_IN
    )
}

pub(super) const KEYCHAIN_LOCKED: &str =
    "the keychain is locked, and cannot ask to be unlocked from here";

// `then` ends the sentence: what a session started here signs in with.
pub(super) fn unlock_the_keychain(then: &str) -> String {
    format!(
        "Unlock it with `security unlock-keychain`, or run Pitboard from a desktop session. Until \
         then a Claude Code session here keeps the login it last read and follows no switch{then}"
    )
}

pub(super) fn then_signs_in_with(file: impl Display) -> String {
    format!(", and one started here signs in with the login in {file}.")
}

pub(super) fn then_cannot_tell(file: impl Display) -> String {
    format!(
        ", and whether one started here signs in with what {file} holds cannot be told, since \
         Pitboard could not read it."
    )
}

pub(super) const THEN_SIGNED_OUT: &str = ", and one started here is signed out.";

pub(super) fn unreadable_to_claude_code_too(why: impl Display) -> String {
    format!("{why}, which Claude Code cannot read either")
}

pub(super) fn log_out_to_delete_it(program: &str) -> String {
    format!(
        "Run `{program} auth logout`, which deletes it, then sign in to Claude Code again. \
         Signing in before that leaves it as it is."
    )
}

pub(super) const DO_NOT_WRITE: &str = "Do not write to the store while this is failing.";

fn while_it_is_there() -> String {
    format!(
        "While it is there, Claude Code sessions already running at a switch {}.",
        said::kept_until_renewed()
    )
}

pub(super) fn file_refresh(path: impl Display, fingerprint: &str) -> String {
    format!("{path}  ·  refresh {fingerprint}")
}

pub(super) fn the_file_signs_sessions_in(os: Os) -> String {
    let cannot = said::out_of_reach(os);
    let (could_not, such_as, as_login) = match os {
        Os::MacOs | Os::Linux => (
            "the keychain could not be read",
            "in a session started over SSH".to_owned(),
            "`/login` over SSH does".to_owned(),
        ),
        Os::Windows => (
            "Credential Manager could not be reached",
            format!("in {}", said::SSH_SIGN_IN),
            format!("`/login` does in {}", said::SSH_SIGN_IN),
        ),
    };
    format!(
        "Claude Code signs in with this wherever it {cannot}, such as {such_as}, and no switch \
         reaches it. A sign-in made where {could_not} leaves one, as {as_login}. {} `pitboard \
         stow` keeps this login for its account, where Pitboard holds no other it can switch to, \
         then deletes the file, which leaves one login for every session.",
        while_it_is_there()
    )
}

pub(super) fn file_without_a_login(path: impl Display) -> String {
    format!("{path}  ·  no login in it")
}

pub(super) fn stow_deletes_it() -> String {
    format!(
        "It holds no Claude Code login. {} `pitboard stow` deletes it, which lets them follow a \
         switch.",
        while_it_is_there()
    )
}

pub(super) fn file_not_read(path: impl Display, why: impl Display) -> String {
    format!("{path}  ·  not read: {why}")
}

pub(super) fn delete_it_yourself(os: Os, command: &str) -> String {
    format!(
        "Pitboard could not read it, so whether it holds a login, which a session that {} signs \
         in with, cannot be told, and `pitboard stow` cannot put it away. {} Deleting it lets \
         them follow a switch, and takes whatever it holds with it: `{command}`.",
        said::out_of_reach(os),
        while_it_is_there()
    )
}

pub(super) const INACTIVE: &str = "inactive";

pub(super) const ON_IN_THE_FALLBACK_STORE: &str =
    "switched on, and this login is in the fallback store";

pub(super) const MAYBE_NOT_THE_SAME_FILE: &str = "Pitboard reads the plaintext file. If Claude \
     Code was given a backend of its own, that is not the same file. Check for an update before \
     switching.";

pub(super) const ON_IN_THE_KEYCHAIN: &str = "switched on; the keychain is still where the login is";

pub(super) fn refused_as_a_session_starts(accounts: &str) -> String {
    format!(
        "Claude Code refuses the logins of {accounts} as a session starts, so Pitboard does not \
         put them in use."
    )
}

pub(super) const THE_STORED_LOGIN: &str = "the stored login, which is what Pitboard moves";

pub(super) const THE_STORED_LOGIN_ONCE_READ: &str =
    "the stored login, which is what Pitboard moves, once a session reads it again";

pub(super) fn something_else(overrides: &str) -> String {
    format!("something else: {overrides}")
}

pub(super) const A_LOGIN_NOTHING_READS: &str = "Claude Code here authenticates with that rather \
     than with the stored login, so switching accounts changes nothing a session would notice. \
     Remove it, or accept that Pitboard is moving a login nothing reads.";

pub(super) fn build_not_found(verified: &str) -> String {
    format!("not found here; Pitboard's facts were read from {verified}")
}

pub(super) fn build_as_read(installed: &str) -> String {
    format!("{installed}, which is what Pitboard's facts were read from")
}

pub(super) fn build_installed(installed: &str, verified: &str) -> String {
    format!("{installed} installed; Pitboard's facts were read from {verified}")
}

pub(super) const NO_DAEMON: &str = "none has run here";

pub(super) fn daemon_version(version: Option<&str>) -> String {
    version.map_or_else(
        || "an unrecorded version".into(),
        |version| format!("Claude Code {version}"),
    )
}

pub(super) fn daemon_running(version: &str, pid: u32) -> String {
    format!("running, {version}, pid {pid}")
}

pub(super) fn daemon_stopped(version: &str) -> String {
    format!("not running; {version} ran here last")
}

pub(super) fn codex_homes_apart(login: impl Display, installer: impl Display) -> String {
    format!("Codex keeps its login in {login}, and its installer, install.ps1, in {installer}")
}

pub(super) const SET_CODEX_HOME: &str = "Set CODEX_HOME, so both use one folder.";

pub(super) fn file_store(setting: &str, auth_file: impl Display) -> String {
    format!("file ({setting})  ·  {auth_file}")
}

// What the file store's line adds: the one layer Pitboard leaves unread.
pub(super) const PROJECTS_NOT_READ: &str = "Pitboard does not read a project's own \
    `.codex/config.toml`, which applies only inside that project: in a trusted project that \
    sets `cli_auth_credentials_store`, Codex keeps its login somewhere else.";

pub(super) fn in_memory_only(setting: &str) -> String {
    format!("in memory only ({setting})")
}

pub(super) fn cannot_tell_the_store(setting: &str) -> String {
    format!("cannot tell: {setting}")
}

pub(super) fn store_set_by(store: &str, setting: &str) -> String {
    format!("{store} ({setting})")
}

pub(super) fn no_store_known(remedy: &str) -> String {
    format!(
        "{remedy}. Pitboard parks and switches no Codex login while it cannot tell where Codex \
         keeps it."
    )
}

pub(super) fn nothing_at_rest(remedy: &str) -> String {
    format!(
        "Codex keeps nothing at rest, so there is no login Pitboard can park or switch. \
         {remedy}."
    )
}

pub(super) fn no_account_to_switch_to(file_store_only: &str, remedy: &str) -> String {
    format!("{file_store_only}, so no enrolled Codex account can be switched to. {remedy}.")
}

pub(super) fn switched_only_in_the_file(store: &str) -> String {
    format!("{store}; Pitboard switches Codex accounts only in the file store")
}

pub(super) const RESTRICTIONS_UNREADABLE: &str = "Codex does not start until that is put right, \
     and Pitboard switches no Codex account while which logins Codex keeps cannot be told.";

pub(super) fn codex_rules_out(accounts: &str) -> String {
    format!(
        "Codex signs out of or does not use the logins of {accounts}, so Pitboard does not put \
         them in use."
    )
}

pub(super) fn codex_file_absent(path: impl Display) -> String {
    format!("{path} is absent: nothing is signed in to Codex")
}

pub(super) fn use_or_sign_in(program: &str) -> String {
    format!(
        "Put one of your enrolled accounts in use with `pitboard use codex/<label>`, or sign in \
         with `{program} login`."
    )
}

pub(super) const NO_CODEX_FILE: &str = "absent; nothing is signed in to Codex";

pub(super) fn codex_file_open(os: Os, commands: &[String]) -> String {
    let file = match os {
        // Codex 0.162.1's file store sets its mode under `cfg(unix)` alone.
        Os::MacOs | Os::Linux => {
            "It holds a usable login in plain text, and Codex sets 0600 only when it creates the \
             file, never on a later write"
        }
        Os::Windows => {
            "It holds a usable login in plain text, and Codex gives it no access list of its own, \
             so it takes its folder's"
        }
    };
    format!("{file}{}", running(commands))
}

pub(super) fn codex_file(path: impl Display, access: &Access) -> String {
    format!("{path}  ·  {}", access.described)
}

pub(super) fn codex_login(email: &str, account: &str, fingerprint: &str) -> String {
    format!("{email}  ·  account {account}  ·  refresh {fingerprint}")
}

pub(super) fn chatgpt_sign_ins_only(program: &str) -> String {
    format!(
        "Pitboard parks and switches Codex's ChatGPT sign-ins, so `pitboard use codex/<label>` \
         refuses while Codex is signed in this way rather than replace a login it has nowhere \
         to park. `{program} login` signs in with a ChatGPT account."
    )
}

pub(super) fn cannot_be_used(why: &str) -> String {
    format!("it cannot be used: {why}")
}

pub(super) fn sign_in_writes_a_fresh_one(program: &str) -> String {
    format!(
        "Pitboard will not park or switch a Codex login it cannot read as one account's. \
         Signing in with `{program}` again writes a fresh one."
    )
}

pub(super) fn items(count: usize, prefix: &str) -> String {
    match count {
        1 => format!("1 {prefix} item"),
        many => format!("{many} {prefix} items"),
    }
}

pub(super) const CODEX_KEEPS_IT_THERE_TOO: &str = "Codex keeps its login there where a setting \
     Pitboard does not read, such as an enterprise's cloud configuration in a session of \
     `codex`, chooses `keyring` or `auto`. A `secrets|` item also keeps the key of Codex's MCP \
     logins. Pitboard switches only auth.json.";

pub(super) const COULD_NOT_TELL: &str = "could not tell";

pub(super) const NONE_RUNNING: &str = "none";

pub(super) fn each_keeps_its_account(running: &str) -> String {
    format!("{running}; each keeps using the account it started with until it is started again")
}

pub(super) fn build_path_says_nothing(program: impl Display, verified: &str) -> String {
    format!(
        "{program}, whose path does not say which version it is; Pitboard's facts were read \
         from {verified}"
    )
}

pub(super) const FOLDERS_SET_ASIDE_FOR_IT: &str = "folders set aside for Claude Desktop";

pub(super) fn nothing_here_changes(holds: &str) -> String {
    format!("This index holds {holds}. Nothing here changes them.")
}

pub(super) fn run_with_home_unset(own: impl Display) -> String {
    format!("Run Pitboard with `HOME` unset, or set to {own}.")
}

pub(super) fn the_folder(folder: impl Display, is: impl Display) -> String {
    format!("{folder} {is}")
}

pub(super) const NO_MOVE_THROUGH_A_LINK: &str =
    "Pitboard moves no Claude Desktop login through a link.";

pub(super) const NO_MOVE_FOR_A_THIRD_PARTY: &str =
    "Pitboard moves no Claude Desktop login for a third-party deployment.";

pub(super) const LET_THE_RESET_FINISH: &str =
    "Open Claude to let the reset finish, then quit it with Command-Q.";

pub(super) fn does_not_exist(folder: impl Display) -> String {
    format!("{folder} does not exist")
}

pub(super) const ENROLL_THEN_SIGN_IN_INSIDE: &str = "Run `pitboard enroll desktop/<label> \
     --sign-in`, then open Claude and sign in: Pitboard waits for it to quit.";

pub(super) const SETTINGS_WITH_A_WRITE: &str =
    "config.json read, with a settings write Claude left, which Pitboard reads as Claude will";

pub(super) const SETTINGS_WITH_A_STALE_WRITE: &str =
    "config.json read; a stale settings write beside it, which Claude drops when it opens";

pub(super) const SETTINGS_READ: &str = "config.json read";

pub(super) const NO_SETTINGS_YET: &str = "no config.json yet";

pub(super) const EVERY_PART_MOVED: &str = "each part of the account's data is one Pitboard moves";

pub(super) fn holds_a_part_unread(part: &str) -> String {
    format!("holds `{part}`, a part of the account's data this Pitboard has not read")
}

pub(super) const NO_MOVE_WHILE_IT_IS_THERE: &str = "Pitboard starts no move of a Claude Desktop \
     login while it is there. Update Pitboard, or report it at \
     https://github.com/datlechin/pitboard/issues if no release reads it yet.";

pub(super) fn holds_parts_unread(parts: &[String]) -> String {
    format!(
        "holds {}, parts of the account's data this Pitboard has not read",
        said::listed(parts.iter().map(|part| format!("`{part}`")))
    )
}

pub(super) const NO_MOVE_WHILE_THEY_ARE_THERE: &str = "Pitboard starts no move of a Claude \
     Desktop login while they are there. Update Pitboard, or report them at \
     https://github.com/datlechin/pitboard/issues if no release reads them yet.";

pub(super) fn move_interrupted(typed: &str, when: &str) -> String {
    format!("a move of `{typed}`'s login was interrupted {when}")
}

pub(super) const KEEP_CLAUDE_CLOSED: &str = "Keep Claude closed: the next change to a Claude \
     Desktop account settles it first. If it cannot, `pitboard abandon` keeps every folder and \
     moves on.";

pub(super) fn signed_out_for_a_sign_in(since: &str, put_back: &str) -> String {
    format!("Claude is signed out for a sign-in since {since}, and `{put_back}` is parked")
}

pub(super) fn finish_the_sign_in(typed: &str, put_back: &str) -> String {
    format!(
        "Sign in inside Claude, quit it with Command-Q, then run `pitboard enroll {typed}`. To \
         go back, run `pitboard use {put_back}`."
    )
}

pub(super) fn on_another_volume(parks: impl Display, data: Option<impl Display>) -> String {
    let data = data.map_or_else(|| "Claude's data".into(), |data| data.to_string());
    format!("{parks} is on another volume than {data}")
}

pub(super) const KEEP_ONE_VOLUME: &str = "Keep Pitboard's folder on the volume your home is on.";

pub(super) fn private_folder(parks: impl Display) -> String {
    format!("private  ·  {parks}")
}

pub(super) fn reached_by_others(parks: impl Display) -> String {
    format!("{parks} can be reached by others")
}

pub(super) fn run_them(commands: &[String]) -> String {
    self::commands(commands).map_or_else(String::new, |commands| format!("Run {commands}."))
}

pub(super) fn is_a_link(parks: impl Display) -> String {
    format!("{parks} is a link")
}

pub(super) const NO_PARK_THROUGH_A_LINK: &str =
    "Pitboard parks no Claude Desktop login through a link.";

pub(super) fn not_yours(parks: impl Display) -> String {
    format!("{parks} is not yours")
}

pub(super) const NO_PARK_IN_ANOTHERS: &str =
    "Pitboard parks no Claude Desktop login in a folder another user owns.";

pub(super) fn not_a_folder(parks: impl Display) -> String {
    format!("{parks} is not a folder")
}

pub(super) const NO_PARK_THERE: &str = "Pitboard parks no Claude Desktop login there.";

pub(super) fn from_another_mac(typed: &str) -> String {
    format!("`{typed}`'s login from another Mac")
}

pub(super) fn from_before_signing_in_again(typed: &str, when: &str) -> String {
    format!("`{typed}`'s login from before a sign-in again, set aside {when}")
}

pub(super) fn earlier_login(typed: &str, when: &str) -> String {
    format!("`{typed}`'s earlier login, set aside {when}")
}

pub(super) fn nobodys_login(when: &str) -> String {
    format!("a login no account here holds, set aside {when}")
}

pub(super) fn parts_of(typed: &str, when: &str) -> String {
    format!("parts of `{typed}`'s login, set aside {when}")
}

pub(super) fn parts_of_nobodys(when: &str) -> String {
    format!("parts of a login no account here holds, set aside {when}")
}

pub(super) const MADE_WHILE_SIGNED_OUT: &str = "what Claude made while signed out";

pub(super) fn set_aside(count: usize, parks: impl Display, holding: &[String]) -> String {
    match holding {
        [] => format!("{count} in {parks}, none holding a login"),
        held => format!(
            "{count} in {parks}, {} holding a login: {}",
            held.len(),
            said::listed(held)
        ),
    }
}

// After what holds a login; `only` where the rest is all Claude made while signed out.
pub(super) fn the_rest_hold(none_held: bool, only: bool, rest: &[String]) -> String {
    let whose = if none_held {
        "they hold"
    } else {
        "the rest hold"
    };
    let only = if only { "only " } else { "" };
    format!("; {whose} {only}{}", said::listed(rest))
}

pub(super) const REPAIR_GIVES_ONE_BACK: &str = "`pitboard repair` gives one back to its account \
     where it can. `pitboard forget` of an account deletes those set aside for it, and `pitboard \
     uninstall` deletes them all.";

pub(super) const ENROLL_UNDER_ITS_NAME: &str = "Open Claude, check which account it shows, quit \
     it with Command-Q, then run `pitboard enroll desktop/<label>` under that account's name.";

pub(super) const NOT_READ_WHO_IS_IN: &str = "Pitboard has not read who is signed in to Claude";

pub(super) const THE_NEXT_CHANGE_READS_IT: &str =
    "The next change to a Claude Desktop account reads it, with Claude closed.";

pub(super) fn settings_unreadable_since(when: &str) -> String {
    format!("Claude's settings could not be read, so this is as Pitboard last read it {when}")
}

pub(super) const THE_SETTINGS_CHECK_SAYS_WHY: &str = "`Claude settings` says why.";

pub(super) fn looks_signed_out_since(who: &str, when: &str) -> String {
    format!(
        "{who}, as Pitboard last read it {when}; both token caches are emptied since, so Claude \
         looks signed out"
    )
}

pub(super) const THE_NEXT_CHANGE_READS_WHO: &str = "The next change to a Claude Desktop account \
     reads who is signed in, with Claude closed.";

pub(super) fn in_use_and_parked(typed: &str) -> String {
    format!("`{typed}` is recorded in use and also holds a parked login")
}

pub(super) fn check_which_account_claude_shows(typed: &str) -> String {
    format!(
        "Open Claude, check which account it shows, and quit it with Command-Q. If it shows \
         `{typed}`, run `pitboard enroll {typed}`: its older parked login is set aside, and \
         nothing is deleted. If it shows another account, run `pitboard enroll desktop/<label>` \
         under that account's name."
    )
}

pub(super) fn as_last_read(who: &str, when: &str) -> String {
    format!("{who}, as Pitboard last read it {when}")
}

pub(super) fn not_looked_at(parks: impl Display, why: &str) -> String {
    format!("not read: Pitboard could not look at it in {parks} ({why})")
}

pub(super) fn make_it_readable(parks: impl Display) -> String {
    format!(
        "Make {parks}, and each folder it is in, readable to you, then run `pitboard doctor` \
         again."
    )
}

pub(super) fn missing_from(parks: impl Display) -> String {
    format!("its parked login is missing from {parks}")
}

pub(super) fn repair_then_sign_in(typed: &str) -> String {
    format!(
        "Run `pitboard repair`. If it gives nothing back, run `{}`.",
        enroll_signing_in(typed)
    )
}

pub(super) fn if_claude_asks(typed: &str) -> String {
    format!(
        "If Claude asks you to sign in once you use it, run `{}`.",
        enroll_signing_in(typed)
    )
}

pub(super) fn claude_build(build: &str) -> String {
    use crate::provider::desktop::assumptions::{
        READ_FROM_CLAUDE_3, READ_FROM_THE_ARCHIVE, READ_FROM_THE_NEWER_BUNDLE, VERIFIED_AGAINST,
    };
    format!(
        "Claude {build}; Pitboard's facts were read from {VERIFIED_AGAINST}, \
         {READ_FROM_THE_ARCHIVE}, {READ_FROM_THE_NEWER_BUNDLE} and {READ_FROM_CLAUDE_3}, and it \
         checks the folder before each move, not the build"
    )
}

pub(super) const NOT_IN_APPLICATIONS: &str = "not found in /Applications";

pub(super) fn local_app_data_in(local: impl Display) -> String {
    format!("Windows keeps this account's local app data in {local}")
}

pub(super) fn kept_in_one_folder(home: impl Display) -> String {
    format!(
        "Pitboard keeps its folder in {home} whatever Windows says, so a run of the renewal \
         schedule and one by hand use the same folder."
    )
}

pub(super) const THE_PROFILE_ROAMS: &str = "this Windows profile roams";

pub(super) const LOGINS_GO_WITH_IT: &str = "Claude Code's and Codex's logins in it go to every \
     computer this account signs in to, as their Credential Manager items do. Pitboard's own \
     folder, in AppData\\Local, stays on this one.";

pub(super) fn wsl_holds(distributions: &[String]) -> String {
    format!("WSL holds {}", said::listed(distributions))
}

pub(super) fn installs_of_their_own(codex_home: impl Display) -> String {
    format!(
        "Claude Code and Codex installed there are installs of their own, which Pitboard on \
         Windows does not switch. Claude Code there keeps its own login. Where Codex there uses \
         {codex_home}, Codex's home here, a switch here changes its login too. WSL 2 runs them \
         in a virtual machine, so Windows' list of processes does not show them running."
    )
}

pub(super) fn copies_on_path(first: impl Display, rest: &[String]) -> String {
    format!("{first} on PATH, then {}", said::listed(rest))
}

pub(super) const UNINSTALL_THE_OTHERS: &str = "A shell runs the first, so updating another \
     changes nothing there. Uninstall the copies you do not use.";

pub(super) fn the_way(variable: Option<&str>) -> String {
    match variable {
        Some(variable) => format!("through the one {variable} names"),
        None => "out directly".to_string(),
    }
}

pub(super) const CODEX_GOES_THROUGH_IT: &str = "Codex sends its requests through it";

pub(super) fn codex_fails_through(proxy: &str) -> String {
    format!(
        "Codex reads the setting as one address, so it sends its requests to a proxy named \
         {proxy}, and they fail"
    )
}

pub(super) fn codex_goes(way: &str) -> String {
    format!(
        "Codex reads the setting as one address, which a proxy for each protocol is not, so its \
         requests go {way}"
    )
}

pub(super) const THE_OTHERS_GO_DIRECTLY: &str =
    "Claude Code and Pitboard follow only the proxy variables, so theirs go out directly";

pub(super) fn the_others_go(claude: &str, pitboard: &str) -> String {
    format!("Claude Code's go {claude}, and Pitboard's {pitboard}")
}

pub(super) fn set_https_proxy(codex: &str, others: &str) -> String {
    format!("{codex}. {others}: set HTTPS_PROXY where this network needs the proxy.")
}

pub(super) fn system_proxy(shown: &str) -> String {
    format!("Windows' proxy settings name {shown}, and HTTPS_PROXY is not set")
}

pub(super) fn inside_wsl(distribution: Option<&str>) -> String {
    match distribution {
        Some(name) => format!("inside WSL, in {name}"),
        None => "inside WSL".to_string(),
    }
}

pub(super) const WINDOWS_KEEPS_ITS_LOGINS: &str = "Pitboard here switches what is installed in \
     this Linux. Claude Code and Codex installed on Windows keep their own logins, which \
     Pitboard for Windows switches.";

// `home` is Codex's home on a Windows drive, by its Windows path where one could be told.
pub(super) fn codex_home_on_windows(home: impl Display, on_windows: Option<&str>) -> String {
    let (there, uses) = match on_windows {
        Some(path) => (format!("{path} on Windows"), path),
        None => ("on a Windows drive".to_string(), "that folder"),
    };
    format!(
        "Pitboard here switches what is installed in this Linux. Claude Code installed on \
         Windows keeps its own login, which Pitboard for Windows switches. Codex's home, {home}, \
         is {there}, where Codex here starts no shared background server. Where Codex on \
         Windows uses {uses}, a switch here changes its login too."
    )
}

pub(super) fn in_git_bash(bash: impl Display) -> String {
    format!("runs the status line in Git Bash, {bash}")
}

pub(super) fn in_powershell_7(found: impl Display) -> String {
    format!("runs the status line in PowerShell 7, {found}, finding no Git Bash")
}

pub(super) fn in_windows_powershell(found: impl Display) -> String {
    format!("runs the status line in Windows PowerShell, {found}, finding no Git Bash")
}

pub(super) const NO_SHELL: &str = "finds neither Git Bash nor PowerShell to run the status line in";

pub(super) fn passes_over(named: impl Display) -> String {
    format!("; it passes over {named}, which CLAUDE_CODE_GIT_BASH_PATH names")
}

pub(super) fn pitboard_there(runs: &str, found: impl Display, passed_over: &str) -> String {
    format!("Claude Code {runs}; `pitboard` there is {found}{passed_over}")
}

pub(super) fn no_pitboard_there(runs: &str, passed_over: &str) -> String {
    format!("Claude Code {runs}; no `pitboard` is on its PATH{passed_over}")
}

pub(super) const ADD_PITBOARD_TO_PATH: &str =
    "Add the folder pitboard.exe is in to your PATH, then start Claude Code again.";

pub(super) fn passed_over(variables: &[&str]) -> String {
    match variables {
        [one] => format!("{one} holds no proxy's address Pitboard can read, so it is passed over"),
        many => format!(
            "{} hold no proxy's address Pitboard can read, so they are passed over",
            said::listed(many)
        ),
    }
}

pub(super) const SET_IT_RIGHT: &str =
    "Set it to a proxy's address, such as http://proxy.example.com:8080, or unset it.";

pub(super) const SET_EACH_RIGHT: &str =
    "Set each to a proxy's address, such as http://proxy.example.com:8080, or unset them.";

pub(super) const DIRECT: &str = "direct: no variable names a proxy";

pub(super) fn direct_passing_over(passed: &str) -> String {
    format!("direct: {passed}")
}

pub(super) fn through(address: &str, variable: &str) -> String {
    format!("through {address}, from {variable}")
}

pub(super) fn socks(fails: bool, variable: &str, address: &str) -> String {
    format!(
        "{}: {}, {address}",
        if fails { "requests fail" } else { "direct" },
        crate::proxy::refused(variable)
    )
}

pub(super) fn http_proxies_only(each: &str, them: &str) -> String {
    format!(
        "Pitboard goes through HTTP and HTTPS proxies only. For its requests to go out, set \
         {each} to an HTTP proxy's address, such as http://proxy.example.com:8080, or unset \
         {them}."
    )
}

pub(super) fn each_of(variables: &[String]) -> String {
    format!("each of {}", said::listed(variables))
}

pub(super) fn exempts_none(exempting: &str) -> String {
    format!("{exempting} exempts none of the hosts Pitboard calls")
}

pub(super) fn exempts_every_host(exempting: &str) -> String {
    format!("{exempting} exempts every host Pitboard calls, which it reaches directly")
}

pub(super) fn exempts(exempting: &str, hosts: &[String]) -> String {
    format!(
        "{exempting} exempts {}, which Pitboard reaches directly",
        said::listed(hosts)
    )
}

pub(super) fn does_not_say_which_pitboard(location: impl Display) -> String {
    format!("{location} does not say which Pitboard it runs")
}

pub(super) fn runs_what_is_gone(program: impl Display) -> String {
    format!("runs {program}, which is not there any more")
}

pub(super) fn runs_the_app(program: impl Display) -> String {
    format!("runs {program}, which is the app itself and not a command line")
}

pub(super) fn runs_daily(program: impl Display) -> String {
    format!("daily  ·  runs {program}")
}

pub(super) fn drifted_in_task_scheduler(runs: &str, drifted: &[String]) -> String {
    format!("{runs}, but in Task Scheduler it {}", said::listed(drifted))
}

// As Task Scheduler's own settings word each.
pub(super) fn drift(drift: &Drift) -> String {
    match drift {
        Drift::OnAcPowerOnly => "starts only if the computer is on AC power",
        Drift::StopsOnBattery => "stops if the computer switches to battery power",
        Drift::SkipsMissedStarts => {
            "does not run as soon as possible after a scheduled start is missed"
        }
        Drift::Disabled => "is disabled",
    }
    .to_string()
}

// A program's exit code, or one of Task Scheduler's own results.
pub(super) fn ended(result: i32) -> String {
    if (0..=255).contains(&result) {
        format!("with exit code {result}")
    } else {
        format!("with 0x{:08X}", result as u32)
    }
}

pub(super) fn last_run_ended(runs: &str, ago: Option<i64>, result: i32) -> String {
    format!(
        "{runs}, and its last run{} ended {}",
        ago.map(|ago| format!(", {} ago,", said::span(ago)))
            .unwrap_or_default(),
        ended(result)
    )
}

pub(super) const RENEW_TO_SEE_WHY: &str = "Run `pitboard renew` to see what it says.";

pub(super) fn last_ran(runs: &str, ago: i64) -> String {
    format!("{runs}  ·  last ran {} ago", said::span(ago))
}

pub(super) fn not_run_yet(runs: &str) -> String {
    format!("{runs}  ·  not run yet")
}

pub(super) const AGAIN_IN_THE_APP_OR_A_TERMINAL: &str = "Turn daily renewal off and on again: in \
     the app's Settings, or with `pitboard schedule uninstall` and then `pitboard schedule \
     install`.";

pub(super) const AGAIN_IN_A_TERMINAL: &str = "Turn daily renewal off and on again with \
     `pitboard schedule uninstall` and then `pitboard schedule install`.";

pub(super) const INSTALL_THE_SERVICE_AGAIN: &str =
    "Install it again with `pitboard watch install`.";

pub(super) fn runs_from_sign_in(at: Option<u8>, program: impl Display) -> String {
    match at {
        Some(at) => format!("from sign-in  ·  switches at {at}%  ·  runs {program}"),
        None => format!("from sign-in  ·  runs {program}"),
    }
}

pub(super) const WATCH_TO_SEE_WHY: &str = "Run `pitboard watch` to see what it says.";

// A proxy as the schedule's line names it, with the hosts Pitboard calls that go around it.
pub(super) fn proxy_named(proxy: &ProxyFact) -> String {
    let named = format!("{}, from {}", proxy.address, proxy.variable);
    match proxy.exempting {
        Some(exempting) if !proxy.exempt.is_empty() => format!(
            "{named}, {exempting} exempting {}",
            said::listed(&proxy.exempt)
        ),
        _ => named,
    }
}

pub(super) fn this_side(in_the_app: bool) -> &'static str {
    if in_the_app { "the app" } else { "this run" }
}

pub(super) const INSTALLED_WITH_NO_PROXY: &str = "the schedule was installed with no proxy";

pub(super) fn installed_with(proxy: &str) -> String {
    format!("the schedule was installed with {proxy}")
}

pub(super) fn goes_directly(here: &str) -> String {
    format!("{here} goes directly")
}

pub(super) fn is_given(here: &str, proxy: &str) -> String {
    format!("{here} is given {proxy}")
}

pub(super) fn goes_through(here: &str, proxy: &str) -> String {
    format!("{here} goes through {proxy}")
}

pub(super) fn another_user_or_password(here: &str, proxy: &str) -> String {
    format!(
        "the schedule was installed with {here}'s proxy, {proxy}, but with another user name or \
         password"
    )
}

pub(super) fn installed_and_here(installed: &str, here: &str) -> String {
    format!("{installed}; {here}")
}

pub(super) const THE_APPS_OWN_ENVIRONMENT: &str = "The app runs with the environment the system \
     started it with, not a terminal's. Turning daily renewal off and on in Settings gives the \
     schedule the app's proxy variables; `pitboard schedule install` in a terminal gives it that \
     terminal's.";

pub(super) fn socks_would_fail(variable: &str) -> String {
    format!(
        "The schedule's runs are given the proxy variables it was installed with. Installed \
         again from here, they would be given this run's SOCKS proxy, which Pitboard does not \
         use yet, and their requests would fail. Set {variable} to an HTTP proxy's address, or \
         unset it, then install it again with `pitboard schedule install`, which takes the proxy \
         variables set where it runs."
    )
}

pub(super) const INSTALL_AGAIN_FROM_HERE: &str = "The schedule's runs are given the proxy \
     variables it was installed with. To give them this run's, install it again from here with \
     `pitboard schedule install`, which takes the proxy variables set where it runs.";

pub(super) fn not_a_full_path(variable: &str, path: &std::path::Path) -> String {
    if path.as_os_str().is_empty() {
        format!("{variable} is empty")
    } else {
        format!(
            "{variable} is `{}`, which is not a full path",
            path.display()
        )
    }
}

pub(super) const SET_A_FULL_PATH: &str = "Set it to a full path, or unset it. Pitboard checks \
     nothing else until then: everything it reads is under these folders.";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::Kind;

    const SID: &str = "S-1-5-21-1004336348-1177238915-682003330-1001";

    // On Windows two `icacls` a path, and none where the person's SID could not be read.
    #[test]
    fn advice_names_a_command_only_where_there_is_one() {
        assert_eq!(
            running(&Os::MacOs.make_private_commands(Kind::File, &["/a"], Some(SID))),
            ": `chmod 600 /a`."
        );
        assert_eq!(
            running(&Os::Linux.make_private_commands(Kind::Directory, &["/a", "/b"], None)),
            ": `chmod 700 /a /b`."
        );
        let windows =
            Os::Windows.make_private_commands(Kind::File, &[r"C:\a", r"C:\b c\"], Some(SID));
        assert_eq!(
            windows,
            [r"C:\a", r"C:\b c"]
                .map(|path| crate::host::acl::icacls(false, path, SID))
                .concat()
        );
        assert_eq!(
            running(&windows),
            format!(
                ": `{}`, `{}`, `{}` and `{}`.",
                windows[0], windows[1], windows[2], windows[3]
            )
        );
        assert!(
            Os::Windows
                .make_private_commands(Kind::File, &[r"C:\a"], None)
                .is_empty()
        );
        // No one `icacls` line serves a file and a folder alike.
        assert!(
            Os::Windows
                .make_private_commands(Kind::Any, &[r"C:\a"], Some(SID))
                .is_empty()
        );
        assert_eq!(running(&[]), ".");
    }

    #[test]
    fn the_machines_checks_say_what_they_find() {
        assert_eq!(
            open_to(
                "/home/x/.pitboard",
                &Access {
                    shared: true,
                    described: "mode 755".into()
                }
            ),
            "/home/x/.pitboard is mode 755"
        );
        assert_eq!(
            changes_nothing_this_way("Run it from a terminal that is not elevated."),
            "Pitboard changes nothing this way: it reads, and renews and writes nothing. Run it \
             from a terminal that is not elevated."
        );
        assert_eq!(
            changes_nothing_here("Update Windows to use it here."),
            "Pitboard changes nothing here: it reads, and renews and writes nothing. Update \
             Windows to use it here."
        );
        assert_eq!(missing("/usr/bin/security"), "/usr/bin/security is missing");
        assert_eq!(
            not_created_yet("/home/x/.pitboard"),
            "/home/x/.pitboard (not created yet)"
        );
        assert_eq!(
            only_you_should_read_the_home(&["chmod 700 /home/x/.pitboard".into()]),
            "Park names contain account identifiers, so only you should read it: `chmod 700 \
             /home/x/.pitboard`."
        );
        assert_eq!(
            others_can_read(
                Os::MacOs,
                &["chmod go-rwx /home/x/.claude/.credentials.json".into()]
            ),
            "These hold usable OAuth tokens in plain text, which is how Claude Code stores them \
             where there is no keychain. Anyone else on this machine can read them: `chmod go-rwx \
             /home/x/.claude/.credentials.json`."
        );
        assert_eq!(
            others_can_read(Os::Windows, &[]),
            "These hold a login: Claude Code's file in plain text, a sealed park only this account \
             opens. Claude Code's file takes the access list of the folder it is in, and a seal \
             stops reading, not writing. Anyone else on this PC can read or change them."
        );
        assert_eq!(enrolled(0), "none enrolled yet");
        assert_eq!(enrolled(1), "1 enrolled");
        assert_eq!(enrolled(3), "3 enrolled");
        assert_eq!(waiting_to_be_deleted(2, 0), "2 waiting to be deleted");
        assert_eq!(
            waiting_to_be_deleted(0, 1),
            "1 folder(s) waiting to be deleted"
        );
        assert_eq!(
            waiting_to_be_deleted(2, 1),
            "2 waiting to be deleted, and 1 folder(s)"
        );
        assert_eq!(
            never_copy_between_machines("/home/x/.pitboard"),
            "Pitboard cannot tell this machine from another that also lacks one, so it cannot \
             refuse state copied between them. Never copy /home/x/.pitboard between machines."
        );
        assert_eq!(
            not_asked_about(2, 600),
            "2 account(s) not being asked about for up to 10m"
        );
        assert_eq!(
            held_back_by("Anthropic and OpenAI"),
            "Anthropic and OpenAI asked for less traffic, or could not be reached. The numbers \
             shown are the last ones measured until then; `pitboard status --fresh` does not \
             override a wait Anthropic and OpenAI asked for."
        );
        assert_eq!(
            could_not_be_read_this_time(2),
            "2 could not be read this time"
        );
        assert_eq!(
            not_a_full_path("HOME", std::path::Path::new("")),
            "HOME is empty"
        );
        assert_eq!(
            not_a_full_path("CODEX_HOME", std::path::Path::new("codex")),
            "CODEX_HOME is `codex`, which is not a full path"
        );
    }

    #[test]
    fn a_parked_logins_checks_say_its_life_and_the_way_back() {
        assert_eq!(
            not_switched_to_for(31 * 86_400),
            "not switched to for 31d 0h; Pitboard has kept its login alive that whole time"
        );
        assert_eq!(
            forget_if_not_coming_back("codex/work"),
            "Every `pitboard` renews it, so its refresh token is rotated and kept live on this \
             machine for as long as it stays enrolled. If you are not coming back to it, `pitboard \
             forget codex/work` deletes the login and the record."
        );
        assert_eq!(
            enroll_signing_in("codex/work"),
            "pitboard enroll codex/work --sign-in"
        );
        assert_eq!(
            sign_in_again("work"),
            "Run `pitboard enroll work --sign-in`."
        );
        assert_eq!(
            none_opens(2),
            "none of the 2 parked logins opens for this Windows account"
        );
        assert_eq!(
            sealed_by_a_lost_key(&[
                "pitboard enroll work --sign-in".into(),
                "pitboard enroll codex/home --sign-in".into()
            ]),
            "An administrator's reset of your Windows password, or a profile restored on another \
             PC, leaves them sealed with a key this account no longer has. Sign in to each again: \
             `pitboard enroll work --sign-in` and `pitboard enroll codex/home --sign-in`."
        );
        assert_eq!(
            not_open_in_this_logon(&["pitboard enroll work --sign-in".into()]),
            "They do not open in this logon. Run `pitboard doctor` from a desktop session before \
             signing in again: `pitboard enroll work --sign-in`."
        );
        assert_eq!(
            park_unusable(MISSING_FROM_THE_VAULT),
            "its parked login is unusable: missing from the vault"
        );
        assert_eq!(parked_with("good for 20d 0h"), "parked, good for 20d 0h");
        assert_eq!(
            park_life("expires in 1d 0h"),
            "its parked login expires in 1d 0h"
        );
        assert_eq!(
            park_life_since("expired", "at 09:00"),
            "its parked login expired at 09:00"
        );
    }

    #[test]
    fn claude_codes_checks_say_what_its_files_and_store_hold() {
        assert_eq!(
            org_pinned("`org-here`", "/etc/claude-code/managed-settings.json"),
            "`forceLoginOrgUUID` set to `org-here` in /etc/claude-code/managed-settings.json"
        );
        assert_eq!(run_once("claude.cmd"), "Run `claude.cmd` once.");
        assert_eq!(
            install_then_run_once("claude"),
            "Install Claude Code, then run `claude` once."
        );
        assert_eq!(
            config_read("/home/x/.claude.json", 12),
            "/home/x/.claude.json  (12 keys)"
        );
        assert_eq!(
            not_installed_and_no_config("/home/x/.claude.json"),
            "Claude Code is not installed where Pitboard looks, and /home/x/.claude.json does not \
             exist"
        );
        assert_eq!(identity("a@b.c", "org"), "a@b.c  ·  org org");
        assert_eq!(
            written_after_the_first_call("Run `claude` once."),
            "It writes this after its first successful call. Run `claude` once."
        );
        assert_eq!(
            default_slot("Claude Code-credentials", "dana"),
            "default  ·  Claude Code-credentials  ·  account dana"
        );
        assert_eq!(
            slot_selected_by("Claude Code-credentials-0123abcd", "dana", "/home/x/work"),
            "Claude Code-credentials-0123abcd  ·  account dana  (selected by /home/x/work)"
        );
        assert_eq!(
            biggest(&[
                ("mcpOAuth one".into(), 300),
                ("mcpOAuth two".into(), 200),
                ("mcpOAuth three".into(), 100),
                ("claudeAiOauth".into(), 50)
            ]),
            ". Mostly: mcpOAuth one 300, mcpOAuth two 200, mcpOAuth three 100"
        );
        assert_eq!(biggest(&[]), "");
        assert_eq!(
            size_refused(8503, 4032, ". Mostly: mcpOAuth one 300"),
            "8503 of 4032 bytes, and PITBOARD_NO_ARGV refuses that. Mostly: mcpOAuth one 300"
        );
        assert_eq!(
            size_on_the_argument_line(8503, 4032, ""),
            "8503 of 4032 bytes: written on the argument line"
        );
        assert_eq!(size(900, 4032), "900 of 4032 bytes");
        assert_eq!(in_one_item(900), "900 bytes, in one item");
        assert_eq!(
            in_pieces(2401, 2, 2400),
            "2401 bytes, in 2 pieces of 2400 characters"
        );
        assert_eq!(
            in_credential_manager("Claude Code-credentials", "claude-code-user"),
            "Credential Manager  ·  Claude Code-credentials/claude-code-user  ·  every logon of \
             yours on this PC reads it"
        );
        assert_eq!(
            plaintext_file("/home/x/.claude/.credentials.json"),
            "plaintext file  ·  /home/x/.claude/.credentials.json"
        );
    }

    #[test]
    fn whose_login_claude_code_has_stored_is_said_with_when() {
        assert_eq!(
            anthropic_named("`here`", "at 09:00"),
            "Anthropic named `here` at 09:00"
        );
        assert_eq!(
            found_none_stored("at 09:00"),
            "Pitboard found no login stored at 09:00"
        );
        assert_eq!(
            not_asked_since_an_update("`here`"),
            "Anthropic has not been asked whose login Claude Code has stored since Pitboard was \
             updated; `here` is the account it last switched to"
        );
        assert_eq!(
            config_named_since("`there`", "Anthropic named `here` at 09:00"),
            "Claude Code's config has named `there` since Anthropic named `here` at 09:00, so \
             another login may be stored"
        );
        assert_eq!(
            config_names_another("`here`", "at 09:00", "`there`"),
            "`here`, as Anthropic said at 09:00, and Claude Code's config names `there`"
        );
        assert_eq!(
            status_shows(
                "`there`",
                "`pitboard use here` writes `here` into the config."
            ),
            "`/status` in Claude Code shows `there`. `pitboard use here` writes `here` into the \
             config."
        );
        assert_eq!(
            renewed_since("`here`", "at 09:00"),
            "`here`, as Anthropic said at 09:00 of a login renewed or replaced since; the next \
             `pitboard status` asks whose it is"
        );
        assert_eq!(
            none_stored_since("`here`", "at 09:00"),
            "`here`, as Anthropic said at 09:00; no login is stored since"
        );
        assert_eq!(
            as_anthropic_said("`here`", "at 09:00"),
            "`here`, as Anthropic said at 09:00"
        );
        assert_eq!(
            one_stored_since("at 09:00"),
            "no login stored, as Pitboard found at 09:00; one is since, and the next `pitboard \
             status` asks whose it is"
        );
        assert_eq!(
            none_stored("at 09:00"),
            "no login stored, as Pitboard found at 09:00"
        );
    }

    #[test]
    fn claude_codes_login_and_the_file_behind_it_are_said_with_what_reads_them() {
        assert_eq!(
            signed_out_keeps(&["mcpOAuth"]),
            "signed out; the document keeps only [\"mcpOAuth\"]"
        );
        assert_eq!(
            oauth_not_an_object(&["claudeAiOauth"]),
            "claudeAiOauth is not an object; the keys are [\"claudeAiOauth\"]"
        );
        assert_eq!(
            refresh_and_keys("0123456789abcdef", &["claudeAiOauth"]),
            "refresh 0123456789abcdef  ·  keys [\"claudeAiOauth\"]"
        );
        assert_eq!(
            refresh_life_and_keys("0123456789abcdef", "good for 20d 0h", &["claudeAiOauth"]),
            "refresh 0123456789abcdef  ·  good for 20d 0h  ·  keys [\"claudeAiOauth\"]"
        );
        assert_eq!(
            from_a_desktop_session(r"C:\Users\dana\.claude\.credentials.json"),
            "A network logon, as a sign-in over SSH with a key, has no Credential Manager. Run \
             Pitboard from a desktop session. A Claude Code session started from this logon reads \
             only C:\\Users\\dana\\.claude\\.credentials.json."
        );
        assert_eq!(
            unlock_the_keychain(THEN_SIGNED_OUT),
            "Unlock it with `security unlock-keychain`, or run Pitboard from a desktop session. \
             Until then a Claude Code session here keeps the login it last read and follows no \
             switch, and one started here is signed out."
        );
        assert_eq!(
            unlock_the_keychain(&then_signs_in_with("/home/x/.claude/.credentials.json")),
            "Unlock it with `security unlock-keychain`, or run Pitboard from a desktop session. \
             Until then a Claude Code session here keeps the login it last read and follows no \
             switch, and one started here signs in with the login in \
             /home/x/.claude/.credentials.json."
        );
        assert_eq!(
            unlock_the_keychain(&then_cannot_tell("/home/x/.claude/.credentials.json")),
            "Unlock it with `security unlock-keychain`, or run Pitboard from a desktop session. \
             Until then a Claude Code session here keeps the login it last read and follows no \
             switch, and whether one started here signs in with what \
             /home/x/.claude/.credentials.json holds cannot be told, since Pitboard could not read \
             it."
        );
        assert_eq!(
            unreadable_to_claude_code_too(
                "`Claude Code-credentials/dana` lacks piece #2 of the 3 its manifest names"
            ),
            "`Claude Code-credentials/dana` lacks piece #2 of the 3 its manifest names, which \
             Claude Code cannot read either"
        );
        assert_eq!(
            log_out_to_delete_it("claude.cmd"),
            "Run `claude.cmd auth logout`, which deletes it, then sign in to Claude Code again. \
             Signing in before that leaves it as it is."
        );
        assert_eq!(
            file_refresh("/home/x/.claude/.credentials.json", "0123456789abcdef"),
            "/home/x/.claude/.credentials.json  ·  refresh 0123456789abcdef"
        );
        assert_eq!(
            the_file_signs_sessions_in(Os::MacOs),
            "Claude Code signs in with this wherever it cannot read the keychain, such as in a \
             session started over SSH, and no switch reaches it. A sign-in made where the keychain \
             could not be read leaves one, as `/login` over SSH does. While it is there, Claude \
             Code sessions already running at a switch keep the account they are on until their \
             login is next renewed, or until they are started again. `pitboard stow` keeps this \
             login for its account, where Pitboard holds no other it can switch to, then deletes \
             the file, which leaves one login for every session."
        );
        assert_eq!(
            the_file_signs_sessions_in(Os::Windows),
            "Claude Code signs in with this wherever it cannot reach Credential Manager, such as \
             in a sign-in over SSH with a key, and no switch reaches it. A sign-in made where \
             Credential Manager could not be reached leaves one, as `/login` does in a sign-in \
             over SSH with a key. While it is there, Claude Code sessions already running at a \
             switch keep the account they are on until their login is next renewed, or until they \
             are started again. `pitboard stow` keeps this login for its account, where Pitboard \
             holds no other it can switch to, then deletes the file, which leaves one login for \
             every session."
        );
        assert_eq!(
            file_without_a_login("/home/x/.claude/.credentials.json"),
            "/home/x/.claude/.credentials.json  ·  no login in it"
        );
        assert_eq!(
            stow_deletes_it(),
            "It holds no Claude Code login. While it is there, Claude Code sessions already \
             running at a switch keep the account they are on until their login is next renewed, \
             or until they are started again. `pitboard stow` deletes it, which lets them follow a \
             switch."
        );
        assert_eq!(
            file_not_read("/home/x/.claude/.credentials.json", "Permission denied"),
            "/home/x/.claude/.credentials.json  ·  not read: Permission denied"
        );
        assert_eq!(
            delete_it_yourself(Os::MacOs, "rm '/home/x/.claude/.credentials.json'"),
            "Pitboard could not read it, so whether it holds a login, which a session that cannot \
             read the keychain signs in with, cannot be told, and `pitboard stow` cannot put it \
             away. While it is there, Claude Code sessions already running at a switch keep the \
             account they are on until their login is next renewed, or until they are started \
             again. Deleting it lets them follow a switch, and takes whatever it holds with it: \
             `rm '/home/x/.claude/.credentials.json'`."
        );
        assert_eq!(
            delete_it_yourself(
                Os::Windows,
                r"Remove-Item -LiteralPath 'C:\Users\dana\.claude\.credentials.json'"
            ),
            "Pitboard could not read it, so whether it holds a login, which a session that cannot \
             reach Credential Manager signs in with, cannot be told, and `pitboard stow` cannot \
             put it away. While it is there, Claude Code sessions already running at a switch keep \
             the account they are on until their login is next renewed, or until they are started \
             again. Deleting it lets them follow a switch, and takes whatever it holds with it: \
             `Remove-Item -LiteralPath 'C:\\Users\\dana\\.claude\\.credentials.json'`."
        );
        assert_eq!(
            refused_as_a_session_starts("`there`"),
            "Claude Code refuses the logins of `there` as a session starts, so Pitboard does not \
             put them in use."
        );
        assert_eq!(
            something_else("ANTHROPIC_API_KEY in this environment"),
            "something else: ANTHROPIC_API_KEY in this environment"
        );
        assert_eq!(
            build_not_found("2.1.309"),
            "not found here; Pitboard's facts were read from 2.1.309"
        );
        assert_eq!(
            build_as_read("2.1.309"),
            "2.1.309, which is what Pitboard's facts were read from"
        );
        assert_eq!(
            build_installed("2.1.310", "2.1.309"),
            "2.1.310 installed; Pitboard's facts were read from 2.1.309"
        );
        assert_eq!(daemon_version(Some("2.1.278")), "Claude Code 2.1.278");
        assert_eq!(daemon_version(None), "an unrecorded version");
        assert_eq!(
            daemon_running("Claude Code 2.1.278", 4321),
            "running, Claude Code 2.1.278, pid 4321"
        );
        assert_eq!(
            daemon_stopped("an unrecorded version"),
            "not running; an unrecorded version ran here last"
        );
    }

    #[test]
    fn codexs_checks_say_where_it_keeps_its_login() {
        assert_eq!(
            codex_homes_apart(r"C:\Users\x\.codex", r"D:\home\.codex"),
            "Codex keeps its login in C:\\Users\\x\\.codex, and its installer, install.ps1, in \
             D:\\home\\.codex"
        );
        assert_eq!(
            file_store("Codex's default", "/home/x/.codex/auth.json"),
            "file (Codex's default)  ·  /home/x/.codex/auth.json"
        );
        assert_eq!(
            in_memory_only("`cli_auth_credentials_store = \"ephemeral\"` in config.toml"),
            "in memory only (`cli_auth_credentials_store = \"ephemeral\"` in config.toml)"
        );
        assert_eq!(
            cannot_tell_the_store("/etc/codex/config.toml is not TOML Codex can read, at line 3"),
            "cannot tell: /etc/codex/config.toml is not TOML Codex can read, at line 3"
        );
        assert_eq!(
            store_set_by("keyring", "Codex's default"),
            "keyring (Codex's default)"
        );
        assert_eq!(
            no_store_known("Codex 0.160.0 does not start until that is put right"),
            "Codex 0.160.0 does not start until that is put right. Pitboard parks and switches no \
             Codex login while it cannot tell where Codex keeps it."
        );
        assert_eq!(
            nothing_at_rest("To use the file store, set it"),
            "Codex keeps nothing at rest, so there is no login Pitboard can park or switch. To use \
             the file store, set it."
        );
        assert_eq!(
            no_account_to_switch_to(
                "Pitboard handles Codex's file store, auth.json",
                "To use the file store, set it"
            ),
            "Pitboard handles Codex's file store, auth.json, so no enrolled Codex account can be \
             switched to. To use the file store, set it."
        );
        assert_eq!(
            switched_only_in_the_file("keyring (Codex's default)"),
            "keyring (Codex's default); Pitboard switches Codex accounts only in the file store"
        );
        assert_eq!(
            codex_rules_out("`codex/there`"),
            "Codex signs out of or does not use the logins of `codex/there`, so Pitboard does not \
             put them in use."
        );
        assert_eq!(
            codex_file_absent("/home/x/.codex/auth.json"),
            "/home/x/.codex/auth.json is absent: nothing is signed in to Codex"
        );
        assert_eq!(
            use_or_sign_in("codex.cmd"),
            "Put one of your enrolled accounts in use with `pitboard use codex/<label>`, or sign \
             in with `codex.cmd login`."
        );
        assert_eq!(
            codex_file_open(Os::Linux, &["chmod 600 /home/x/.codex/auth.json".into()]),
            "It holds a usable login in plain text, and Codex sets 0600 only when it creates the \
             file, never on a later write: `chmod 600 /home/x/.codex/auth.json`."
        );
        assert_eq!(
            codex_file_open(Os::Windows, &[]),
            "It holds a usable login in plain text, and Codex gives it no access list of its own, \
             so it takes its folder's."
        );
        assert_eq!(
            codex_file(
                "/home/x/.codex/auth.json",
                &Access {
                    shared: false,
                    described: "mode 600".into()
                }
            ),
            "/home/x/.codex/auth.json  ·  mode 600"
        );
        assert_eq!(
            codex_login("w@example.com", "work-account", "0123456789abcdef"),
            "w@example.com  ·  account work-account  ·  refresh 0123456789abcdef"
        );
        assert_eq!(
            chatgpt_sign_ins_only("codex"),
            "Pitboard parks and switches Codex's ChatGPT sign-ins, so `pitboard use codex/<label>` \
             refuses while Codex is signed in this way rather than replace a login it has nowhere \
             to park. `codex login` signs in with a ChatGPT account."
        );
        assert_eq!(
            cannot_be_used("its id token is not readable"),
            "it cannot be used: its id token is not readable"
        );
        assert_eq!(
            sign_in_writes_a_fresh_one("codex"),
            "Pitboard will not park or switch a Codex login it cannot read as one account's. \
             Signing in with `codex` again writes a fresh one."
        );
        assert_eq!(items(1, "`cli|`"), "1 `cli|` item");
        assert_eq!(items(2, "`secrets|`"), "2 `secrets|` items");
        assert_eq!(
            each_keeps_its_account("2 `codex` sessions (pid 4321, 99)"),
            "2 `codex` sessions (pid 4321, 99); each keeps using the account it started with until \
             it is started again"
        );
        assert_eq!(
            build_path_says_nothing("/home/x/.volta/bin/codex", "0.164.0"),
            "/home/x/.volta/bin/codex, whose path does not say which version it is; Pitboard's \
             facts were read from 0.164.0"
        );
    }

    #[test]
    fn claude_desktops_checks_say_what_the_index_and_the_folder_hold() {
        assert_eq!(
            nothing_here_changes("`desktop/work` and `desktop/home`"),
            "This index holds `desktop/work` and `desktop/home`. Nothing here changes them."
        );
        assert_eq!(
            run_with_home_unset("/Users/dana"),
            "Run Pitboard with `HOME` unset, or set to /Users/dana."
        );
        assert_eq!(
            the_folder(
                "/Users/dana/Library/Application Support/Claude",
                "is a link"
            ),
            "/Users/dana/Library/Application Support/Claude is a link"
        );
        assert_eq!(
            does_not_exist("/Users/dana/Library/Application Support/Claude"),
            "/Users/dana/Library/Application Support/Claude does not exist"
        );
        assert_eq!(
            holds_a_part_unread("Cookies-wal"),
            "holds `Cookies-wal`, a part of the account's data this Pitboard has not read"
        );
        assert_eq!(
            holds_parts_unread(&["CdmStorage.db-wal".into(), "Cookies-wal".into()]),
            "holds `CdmStorage.db-wal` and `Cookies-wal`, parts of the account's data this \
             Pitboard has not read"
        );
        assert_eq!(
            move_interrupted("desktop/work", "at 09:00"),
            "a move of `desktop/work`'s login was interrupted at 09:00"
        );
        assert_eq!(
            signed_out_for_a_sign_in("at 09:00", "desktop/home"),
            "Claude is signed out for a sign-in since at 09:00, and `desktop/home` is parked"
        );
        assert_eq!(
            finish_the_sign_in("desktop/new", "desktop/home"),
            "Sign in inside Claude, quit it with Command-Q, then run `pitboard enroll \
             desktop/new`. To go back, run `pitboard use desktop/home`."
        );
        assert_eq!(
            on_another_volume(
                "/Users/dana/.pitboard/parks",
                Some("/Users/dana/Library/Application Support/Claude")
            ),
            "/Users/dana/.pitboard/parks is on another volume than /Users/dana/Library/Application \
             Support/Claude"
        );
        assert_eq!(
            on_another_volume("/Users/dana/.pitboard/parks", None::<&str>),
            "/Users/dana/.pitboard/parks is on another volume than Claude's data"
        );
        assert_eq!(
            private_folder("/Users/dana/.pitboard/parks"),
            "private  ·  /Users/dana/.pitboard/parks"
        );
        assert_eq!(
            reached_by_others("/Users/dana/.pitboard/parks"),
            "/Users/dana/.pitboard/parks can be reached by others"
        );
        assert_eq!(
            run_them(&["chmod 700 /Users/dana/.pitboard/parks".into()]),
            "Run `chmod 700 /Users/dana/.pitboard/parks`."
        );
        assert_eq!(run_them(&[]), "");
        assert_eq!(
            is_a_link("/Users/dana/.pitboard/parks"),
            "/Users/dana/.pitboard/parks is a link"
        );
        assert_eq!(
            not_yours("/Users/dana/.pitboard/parks"),
            "/Users/dana/.pitboard/parks is not yours"
        );
        assert_eq!(
            not_a_folder("/Users/dana/.pitboard/parks"),
            "/Users/dana/.pitboard/parks is not a folder"
        );
        assert_eq!(
            settings_unreadable_since("at 09:00"),
            "Claude's settings could not be read, so this is as Pitboard last read it at 09:00"
        );
        assert_eq!(
            looks_signed_out_since("`desktop/work`", "at 09:00"),
            "`desktop/work`, as Pitboard last read it at 09:00; both token caches are emptied \
             since, so Claude looks signed out"
        );
        assert_eq!(
            in_use_and_parked("desktop/work"),
            "`desktop/work` is recorded in use and also holds a parked login"
        );
        assert_eq!(
            check_which_account_claude_shows("desktop/work"),
            "Open Claude, check which account it shows, and quit it with Command-Q. If it shows \
             `desktop/work`, run `pitboard enroll desktop/work`: its older parked login is set \
             aside, and nothing is deleted. If it shows another account, run `pitboard enroll \
             desktop/<label>` under that account's name."
        );
        assert_eq!(
            as_last_read("`desktop/work`", "at 09:00"),
            "`desktop/work`, as Pitboard last read it at 09:00"
        );
        assert_eq!(
            not_looked_at(
                "/Users/dana/.pitboard/parks",
                "Permission denied (os error 13)"
            ),
            "not read: Pitboard could not look at it in /Users/dana/.pitboard/parks (Permission \
             denied (os error 13))"
        );
        assert_eq!(
            make_it_readable("/Users/dana/.pitboard/parks"),
            "Make /Users/dana/.pitboard/parks, and each folder it is in, readable to you, then run \
             `pitboard doctor` again."
        );
        assert_eq!(
            missing_from("/Users/dana/.pitboard/parks"),
            "its parked login is missing from /Users/dana/.pitboard/parks"
        );
        assert_eq!(
            repair_then_sign_in("desktop/home"),
            "Run `pitboard repair`. If it gives nothing back, run `pitboard enroll desktop/home \
             --sign-in`."
        );
        assert_eq!(
            if_claude_asks("desktop/home"),
            "If Claude asks you to sign in once you use it, run `pitboard enroll desktop/home \
             --sign-in`."
        );
        assert_eq!(
            claude_build("2.31226.1"),
            "Claude 2.31226.1; Pitboard's facts were read from 2.19675.0, 2.7032.0, 2.42304.0 and \
             3.6556.2, and it checks the folder before each move, not the build"
        );
    }

    #[test]
    fn folders_set_aside_are_said_by_whose_they_are() {
        assert_eq!(
            from_another_mac("desktop/lost"),
            "`desktop/lost`'s login from another Mac"
        );
        assert_eq!(
            from_before_signing_in_again("desktop/home", "at 09:00"),
            "`desktop/home`'s login from before a sign-in again, set aside at 09:00"
        );
        assert_eq!(
            earlier_login("desktop/work", "at 09:00"),
            "`desktop/work`'s earlier login, set aside at 09:00"
        );
        assert_eq!(
            nobodys_login("at 09:00"),
            "a login no account here holds, set aside at 09:00"
        );
        assert_eq!(
            parts_of("desktop/home", "at 09:00"),
            "parts of `desktop/home`'s login, set aside at 09:00"
        );
        assert_eq!(
            parts_of_nobodys("at 09:00"),
            "parts of a login no account here holds, set aside at 09:00"
        );
        assert_eq!(
            set_aside(1, "/Users/dana/.pitboard/parks", &[]),
            "1 in /Users/dana/.pitboard/parks, none holding a login"
        );
        assert_eq!(
            set_aside(
                3,
                "/Users/dana/.pitboard/parks",
                &[
                    "`desktop/lost`'s login from another Mac".into(),
                    "a login no account here holds, set aside at 09:00".into()
                ]
            ),
            "3 in /Users/dana/.pitboard/parks, 2 holding a login: `desktop/lost`'s login from \
             another Mac and a login no account here holds, set aside at 09:00"
        );
        assert_eq!(
            the_rest_hold(true, true, &[MADE_WHILE_SIGNED_OUT.into()]),
            "; they hold only what Claude made while signed out"
        );
        assert_eq!(
            the_rest_hold(
                true,
                false,
                &[
                    "parts of `desktop/home`'s login, set aside at 09:00".into(),
                    MADE_WHILE_SIGNED_OUT.into()
                ]
            ),
            "; they hold parts of `desktop/home`'s login, set aside at 09:00 and what Claude made \
             while signed out"
        );
        assert_eq!(
            the_rest_hold(false, false, &[MADE_WHILE_SIGNED_OUT.into()]),
            "; the rest hold what Claude made while signed out"
        );
    }

    #[test]
    fn windows_and_wsl_are_said_in_their_own_words() {
        assert_eq!(
            local_app_data_in(r"D:\Local"),
            "Windows keeps this account's local app data in D:\\Local"
        );
        assert_eq!(
            kept_in_one_folder(r"C:\Users\x\AppData\Local\Pitboard"),
            "Pitboard keeps its folder in C:\\Users\\x\\AppData\\Local\\Pitboard whatever Windows \
             says, so a run of the renewal schedule and one by hand use the same folder."
        );
        assert_eq!(
            wsl_holds(&["Ubuntu".into(), "Debian".into()]),
            "WSL holds Ubuntu and Debian"
        );
        assert_eq!(
            installs_of_their_own(r"C:\Users\x\.codex"),
            "Claude Code and Codex installed there are installs of their own, which Pitboard on \
             Windows does not switch. Claude Code there keeps its own login. Where Codex there \
             uses C:\\Users\\x\\.codex, Codex's home here, a switch here changes its login too. \
             WSL 2 runs them in a virtual machine, so Windows' list of processes does not show \
             them running."
        );
        assert_eq!(
            copies_on_path(
                r"C:\a\pitboard.exe",
                &[r"C:\b\pitboard.exe".into(), r"C:\c\pitboard.exe".into()]
            ),
            "C:\\a\\pitboard.exe on PATH, then C:\\b\\pitboard.exe and C:\\c\\pitboard.exe"
        );
        assert_eq!(
            the_way(Some("HTTP_PROXY")),
            "through the one HTTP_PROXY names"
        );
        assert_eq!(the_way(None), "out directly");
        assert_eq!(
            codex_fails_through("https=secure.example.com:443"),
            "Codex reads the setting as one address, so it sends its requests to a proxy named \
             https=secure.example.com:443, and they fail"
        );
        assert_eq!(
            codex_goes("out directly"),
            "Codex reads the setting as one address, which a proxy for each protocol is not, so \
             its requests go out directly"
        );
        assert_eq!(
            the_others_go("out directly", "through the one ALL_PROXY names"),
            "Claude Code's go out directly, and Pitboard's through the one ALL_PROXY names"
        );
        assert_eq!(
            set_https_proxy(CODEX_GOES_THROUGH_IT, THE_OTHERS_GO_DIRECTLY),
            "Codex sends its requests through it. Claude Code and Pitboard follow only the proxy \
             variables, so theirs go out directly: set HTTPS_PROXY where this network needs the \
             proxy."
        );
        assert_eq!(
            system_proxy("proxy.example.com:8080"),
            "Windows' proxy settings name proxy.example.com:8080, and HTTPS_PROXY is not set"
        );
        assert_eq!(inside_wsl(Some("Ubuntu")), "inside WSL, in Ubuntu");
        assert_eq!(inside_wsl(None), "inside WSL");
        assert_eq!(
            codex_home_on_windows("/mnt/c/Users/x/.codex", Some(r"C:\Users\x\.codex")),
            "Pitboard here switches what is installed in this Linux. Claude Code installed on \
             Windows keeps its own login, which Pitboard for Windows switches. Codex's home, \
             /mnt/c/Users/x/.codex, is C:\\Users\\x\\.codex on Windows, where Codex here starts no \
             shared background server. Where Codex on Windows uses C:\\Users\\x\\.codex, a switch \
             here changes its login too."
        );
        assert_eq!(
            codex_home_on_windows("/media/c/codex", None),
            "Pitboard here switches what is installed in this Linux. Claude Code installed on \
             Windows keeps its own login, which Pitboard for Windows switches. Codex's home, \
             /media/c/codex, is on a Windows drive, where Codex here starts no shared background \
             server. Where Codex on Windows uses that folder, a switch here changes its login too."
        );
        assert_eq!(
            in_git_bash(r"C:\Program Files\Git\bin\bash.exe"),
            "runs the status line in Git Bash, C:\\Program Files\\Git\\bin\\bash.exe"
        );
        assert_eq!(
            in_powershell_7(r"C:\Program Files\PowerShell\7\pwsh.exe"),
            "runs the status line in PowerShell 7, C:\\Program Files\\PowerShell\\7\\pwsh.exe, \
             finding no Git Bash"
        );
        assert_eq!(
            in_windows_powershell(r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe"),
            "runs the status line in Windows PowerShell, \
             C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe, finding no Git Bash"
        );
        assert_eq!(
            passes_over(r"D:\tools\git.exe"),
            "; it passes over D:\\tools\\git.exe, which CLAUDE_CODE_GIT_BASH_PATH names"
        );
        assert_eq!(
            pitboard_there(
                "runs the status line in Git Bash, C:\\bash.exe",
                r"C:\Users\x\.cargo\bin\pitboard.exe",
                ""
            ),
            "Claude Code runs the status line in Git Bash, C:\\bash.exe; `pitboard` there is \
             C:\\Users\\x\\.cargo\\bin\\pitboard.exe"
        );
        assert_eq!(
            no_pitboard_there(
                NO_SHELL,
                "; it passes over D:\\tools\\git.exe, which CLAUDE_CODE_GIT_BASH_PATH names"
            ),
            "Claude Code finds neither Git Bash nor PowerShell to run the status line in; no \
             `pitboard` is on its PATH; it passes over D:\\tools\\git.exe, which \
             CLAUDE_CODE_GIT_BASH_PATH names"
        );
    }

    #[test]
    fn the_network_is_said_with_the_variable_that_names_it() {
        assert_eq!(
            passed_over(&["HTTPS_PROXY"]),
            "HTTPS_PROXY holds no proxy's address Pitboard can read, so it is passed over"
        );
        assert_eq!(
            passed_over(&["ALL_PROXY", "HTTPS_PROXY"]),
            "ALL_PROXY and HTTPS_PROXY hold no proxy's address Pitboard can read, so they are \
             passed over"
        );
        assert_eq!(
            direct_passing_over(
                "HTTPS_PROXY holds no proxy's address Pitboard can read, so it is passed over"
            ),
            "direct: HTTPS_PROXY holds no proxy's address Pitboard can read, so it is passed over"
        );
        assert_eq!(
            through("http://***:***@proxy.example.com:3128", "HTTPS_PROXY"),
            "through http://***:***@proxy.example.com:3128, from HTTPS_PROXY"
        );
        assert_eq!(
            socks(true, "all_proxy", "socks5://127.0.0.1:7890"),
            "requests fail: Pitboard does not use SOCKS proxies yet, and all_proxy names one, \
             socks5://127.0.0.1:7890"
        );
        assert_eq!(
            socks(false, "HTTPS_PROXY", "socks4://10.0.0.1:1080"),
            "direct: Pitboard does not use SOCKS proxies yet, and HTTPS_PROXY names one, \
             socks4://10.0.0.1:1080"
        );
        assert_eq!(
            http_proxies_only("all_proxy", "it"),
            "Pitboard goes through HTTP and HTTPS proxies only. For its requests to go out, set \
             all_proxy to an HTTP proxy's address, such as http://proxy.example.com:8080, or unset \
             it."
        );
        assert_eq!(
            each_of(&["ALL_PROXY".into(), "HTTPS_PROXY".into()]),
            "each of ALL_PROXY and HTTPS_PROXY"
        );
        assert_eq!(
            exempts_none("NO_PROXY"),
            "NO_PROXY exempts none of the hosts Pitboard calls"
        );
        assert_eq!(
            exempts_every_host("no_proxy"),
            "no_proxy exempts every host Pitboard calls, which it reaches directly"
        );
        assert_eq!(
            exempts(
                "NO_PROXY",
                &["api.anthropic.com".into(), "platform.claude.com".into()]
            ),
            "NO_PROXY exempts api.anthropic.com and platform.claude.com, which Pitboard reaches \
             directly"
        );
    }

    #[test]
    fn the_schedule_and_the_watch_service_are_said_with_what_they_run() {
        assert_eq!(
            does_not_say_which_pitboard("/home/x/.config/systemd/user/pitboard-renew.timer"),
            "/home/x/.config/systemd/user/pitboard-renew.timer does not say which Pitboard it runs"
        );
        assert_eq!(
            runs_what_is_gone("/opt/homebrew/bin/pitboard"),
            "runs /opt/homebrew/bin/pitboard, which is not there any more"
        );
        assert_eq!(
            runs_the_app("/Applications/Pitboard.app/Contents/MacOS/Pitboard"),
            "runs /Applications/Pitboard.app/Contents/MacOS/Pitboard, which is the app itself and \
             not a command line"
        );
        assert_eq!(
            runs_daily("/opt/homebrew/bin/pitboard"),
            "daily  ·  runs /opt/homebrew/bin/pitboard"
        );
        assert_eq!(
            drifted_in_task_scheduler(
                "daily  ·  runs C:\\pitboard.exe",
                &[drift(&Drift::OnAcPowerOnly), drift(&Drift::Disabled)]
            ),
            "daily  ·  runs C:\\pitboard.exe, but in Task Scheduler it starts only if the computer \
             is on AC power and is disabled"
        );
        assert_eq!(
            drift(&Drift::StopsOnBattery),
            "stops if the computer switches to battery power"
        );
        assert_eq!(
            drift(&Drift::SkipsMissedStarts),
            "does not run as soon as possible after a scheduled start is missed"
        );
        assert_eq!(ended(1), "with exit code 1");
        assert_eq!(ended(0x8007_10E0_u32 as i32), "with 0x800710E0");
        assert_eq!(
            last_run_ended("daily  ·  runs C:\\pitboard.exe", Some(3_660), 1),
            "daily  ·  runs C:\\pitboard.exe, and its last run, 1h 01m ago, ended with exit code 1"
        );
        assert_eq!(
            last_run_ended("from sign-in  ·  runs C:\\pitboard.exe", None, 1),
            "from sign-in  ·  runs C:\\pitboard.exe, and its last run ended with exit code 1"
        );
        assert_eq!(
            last_ran("daily  ·  runs C:\\pitboard.exe", 3_660),
            "daily  ·  runs C:\\pitboard.exe  ·  last ran 1h 01m ago"
        );
        assert_eq!(
            not_run_yet("daily  ·  runs C:\\pitboard.exe"),
            "daily  ·  runs C:\\pitboard.exe  ·  not run yet"
        );
        assert_eq!(
            runs_from_sign_in(Some(90), "/opt/homebrew/bin/pitboard"),
            "from sign-in  ·  switches at 90%  ·  runs /opt/homebrew/bin/pitboard"
        );
        assert_eq!(
            runs_from_sign_in(None, "/opt/homebrew/bin/pitboard"),
            "from sign-in  ·  runs /opt/homebrew/bin/pitboard"
        );
    }

    #[test]
    fn the_schedules_proxy_is_said_beside_this_runs() {
        assert_eq!(
            proxy_named(&ProxyFact {
                variable: "HTTPS_PROXY",
                address: "http://***:***@proxy.example.com:3128".into(),
                server: "proxy.example.com:3128".into(),
                on_loopback: false,
                socks: false,
                fails: false,
                exempting: Some("NO_PROXY"),
                exempt: vec!["chatgpt.com".into()]
            }),
            "http://***:***@proxy.example.com:3128, from HTTPS_PROXY, NO_PROXY exempting \
             chatgpt.com"
        );
        assert_eq!(
            proxy_named(&ProxyFact {
                variable: "ALL_PROXY",
                address: "http://127.0.0.1:3128".into(),
                server: "127.0.0.1:3128".into(),
                on_loopback: true,
                socks: false,
                fails: false,
                exempting: None,
                exempt: Vec::new()
            }),
            "http://127.0.0.1:3128, from ALL_PROXY"
        );
        assert_eq!(this_side(true), "the app");
        assert_eq!(this_side(false), "this run");
        assert_eq!(
            installed_with("http://***:***@proxy.example.com:3128, from HTTPS_PROXY"),
            "the schedule was installed with http://***:***@proxy.example.com:3128, from \
             HTTPS_PROXY"
        );
        assert_eq!(goes_directly("this run"), "this run goes directly");
        assert_eq!(
            is_given("this run", "socks5h://127.0.0.1:1080, from ALL_PROXY"),
            "this run is given socks5h://127.0.0.1:1080, from ALL_PROXY"
        );
        assert_eq!(
            goes_through("the app", "http://127.0.0.1:3128, from ALL_PROXY"),
            "the app goes through http://127.0.0.1:3128, from ALL_PROXY"
        );
        assert_eq!(
            another_user_or_password(
                "this run",
                "http://***:***@proxy.example.com:3128, from HTTPS_PROXY"
            ),
            "the schedule was installed with this run's proxy, \
             http://***:***@proxy.example.com:3128, from HTTPS_PROXY, but with another user name \
             or password"
        );
        assert_eq!(
            installed_and_here(INSTALLED_WITH_NO_PROXY, "this run goes directly"),
            "the schedule was installed with no proxy; this run goes directly"
        );
        assert_eq!(
            socks_would_fail("ALL_PROXY"),
            "The schedule's runs are given the proxy variables it was installed with. Installed \
             again from here, they would be given this run's SOCKS proxy, which Pitboard does not \
             use yet, and their requests would fail. Set ALL_PROXY to an HTTP proxy's address, or \
             unset it, then install it again with `pitboard schedule install`, which takes the \
             proxy variables set where it runs."
        );
    }
}
