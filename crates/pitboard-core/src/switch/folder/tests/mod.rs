mod enrolling;
mod giving_up;
mod reading;
mod refusing;
mod repairing;
mod signing_in;
mod switching;
mod uninstalling;

use super::super::harness::{
    HOME, NOW, Rig, STRANGER, WORK, desktop_machine, desktop_pair, recover, sessions,
    sign_in_inside,
};
use super::super::{Outcome, Settled, enroll, forget, repair, settle, switch, update_config};
use crate::desktop_testing::{Person, write_stand_in_jar};
use crate::error::Error;
use crate::host::Cookie;
use crate::provider::ProviderId;
use crate::service::{Permit, Warning};
use crate::state::{self, Key, Park, ParkKind, SetAside};
use crate::store::folder::Manifest;
use serde_json::{Map, Value, json};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::mpsc::RecvTimeoutError;

// `work`, in a session from before the one Claude holds.
const WORK_BEFORE: Person = Person {
    uuid: WORK.uuid,
    session: "work-session-before",
};

fn enrol(m: &Rig, label: &str) -> crate::error::Result<(super::Enrolled, Vec<Warning>)> {
    enroll(settled(m), &m.key(label), None)
}

fn use_(m: &Rig, label: &str) -> crate::error::Result<Outcome> {
    switch(settled(m), &m.key(label)).map(|(outcome, _)| outcome)
}

fn forget_(m: &Rig, label: &str) -> crate::error::Result<String> {
    forget(settled(m), &m.key(label)).map(|(email, _)| email)
}

fn confirm_(m: &Rig, label: &str) -> crate::error::Result<Outcome> {
    update_config(settled(m), &m.key(label)).map(|(outcome, _)| outcome)
}

fn code<T: std::fmt::Debug>(result: crate::error::Result<T>) -> &'static str {
    result.expect_err("refused").code()
}

// Claude's settings with `key` set to `value`, or taken out, in place.
fn set(m: &Rig, key: &str, value: Option<Value>) {
    let path = data(m).join("config.json");
    let mut settings: Map<String, Value> =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("settings")).expect("JSON");
    match value {
        Some(value) => {
            settings.insert(key.into(), value);
        }
        None => {
            settings.shift_remove(key);
        }
    }
    std::fs::write(
        &path,
        serde_json::to_string_pretty(&settings).expect("JSON"),
    )
    .expect("set");
}

fn names(m: &Rig, who: &Person) {
    set(m, "lastKnownAccountUuid", Some(json!(who.uuid)));
}

fn session_of(who: Option<&Person>) -> Vec<Cookie> {
    who.map(|who| Cookie {
        host: ".claude.ai".into(),
        name: "sessionKey".into(),
        path: "/".into(),
        stored: [b"v10".as_slice(), who.session.as_bytes()].concat(),
        expires_utc: 0,
    })
    .into_iter()
    .collect()
}

// The jar holding `who`'s session, or none.
fn jar(m: &Rig, who: Option<&Person>) {
    write_stand_in_jar(&data(m).join("Cookies"), 24, &session_of(who));
}

// `work` and `home` enrolled, `work` in use: what two enrolments over one Claude leave.
fn two_accounts(name: &str) -> Rig {
    let m = desktop_machine(name);
    jar(&m, Some(&HOME));
    names(&m, &HOME);
    enrol(&m, "home").expect("`home` enrolled");
    jar(&m, Some(&WORK));
    names(&m, &WORK);
    enrol(&m, "work").expect("`work` in use again");
    m
}

fn items() -> &'static [&'static str] {
    match crate::provider::of(ProviderId::Desktop) {
        crate::provider::LoginKind::Folder(app) => app.items(),
        crate::provider::LoginKind::Document(_) => unreachable!("a folder tool"),
    }
}

// Claude's folder with no jar and none of an account's other items.
fn without_items(m: &Rig) {
    for item in items() {
        let path = data(m).join(item);
        match std::fs::symlink_metadata(&path) {
            Ok(found) if found.is_dir() => std::fs::remove_dir_all(&path).expect("an item"),
            Ok(_) => std::fs::remove_file(&path).expect("an item"),
            Err(_) => {}
        }
    }
}

fn in_use(m: &Rig) -> Option<String> {
    let state = state::load(&m.ctx).expect("state");
    state
        .account_in_use(ProviderId::Desktop)
        .map(|a| a.label.clone())
}

// No mover exists yet to make a folder park.
fn park_by_hand(m: &Rig, key: Option<&Key>, name: &str, who: &Person) -> Park {
    let folder = crate::store::folder::at(&m.ctx, name);
    std::fs::create_dir_all(folder.join("items")).expect("a park");
    write_stand_in_jar(&folder.join("items/Cookies"), 24, &session_of(Some(who)));
    let keys = format!(
        "{{\"oauth:tokenCache\": \"acct:{0}|t\", \"lastKnownAccountUuid\": \"{0}\"}}",
        who.uuid
    );
    std::fs::write(folder.join("keys.json"), &keys).expect("keys");
    let digests = crate::provider::folder::Keys::from_json(keys.as_bytes())
        .expect("keys")
        .digests();
    let manifest = Manifest {
        v: 1,
        tool: "desktop".into(),
        account: Some(format!("desktop_{}", who.uuid)),
        named: Some(who.uuid.into()),
        login: Some(who.login()),
        session_expires_at: None,
        parked_at: NOW,
        machine: m.ctx.this_machine(),
        items: vec!["Cookies".into()],
        keys: digests,
        left_in_folder: Vec::new(),
    };
    std::fs::write(
        folder.join("manifest.json"),
        serde_json::to_vec(&manifest).expect("JSON"),
    )
    .expect("manifest");
    let park = Park {
        service: name.into(),
        kind: ParkKind::Folder,
        parked_at: NOW,
        login: who.login(),
        access_expires_at: None,
        refresh_expires_at: None,
        session_expires_at: None,
    };
    if let Some(key) = key {
        let mut state = state::load(&m.ctx).expect("state");
        state.park(key, park.clone());
        state::save(&m.ctx, Permit::for_a_test(), &state).expect("saved");
    }
    park
}

fn park_name(who: &Person) -> String {
    crate::park::service_name(&format!("desktop_{}", who.uuid), NOW * 1000)
}

#[test]
fn the_desktop_provider_reaches_no_keychain_and_no_service() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    for dir in ["provider/desktop", "switch/folder"] {
        for entry in std::fs::read_dir(root.join(dir)).expect("sources") {
            let path = entry.expect("an entry").path();
            if path
                .file_name()
                .is_some_and(|name| name == "tests" || name == "assumptions.rs")
            {
                continue;
            }
            let source = std::fs::read_to_string(&path).expect("a source");
            for line in source
                .lines()
                .filter(|line| !line.trim_start().starts_with("//"))
            {
                for needle in [
                    "security",
                    "keychain",
                    "vault_",
                    "ureq",
                    "crate::api",
                    "crate::proxy",
                    "names::",
                ] {
                    assert!(
                        !line.contains(needle),
                        "{}: `{needle}` in {line:?}",
                        path.display()
                    );
                }
                if line.contains("crate::store::") || line.contains("use crate::store") {
                    assert!(
                        line.contains("store::fingerprint") || line.contains("store::folder"),
                        "{}: {line:?}",
                        path.display()
                    );
                }
            }
        }
    }
}

// Without printing two indexes and two folders where they differ.
fn used(m: &Rig, label: &str) -> (Outcome, Vec<Warning>) {
    switch(settled(m), &m.key(label)).unwrap_or_else(|e| panic!("use {label}: {e}"))
}

fn park_of(m: &Rig, label: &str) -> Option<Park> {
    state::load(&m.ctx)
        .expect("state")
        .get(&m.key(label))
        .and_then(|a| a.parked.clone())
}

fn named_in_settings(m: &Rig) -> Option<String> {
    setting(m, "lastKnownAccountUuid").and_then(|name| name.as_str().map(str::to_string))
}

fn setting(m: &Rig, key: &str) -> Option<Value> {
    let mut settings: Map<String, Value> = serde_json::from_str(
        &std::fs::read_to_string(data(m).join("config.json")).expect("settings"),
    )
    .expect("JSON");
    settings.shift_remove(key)
}

fn refused(m: &Rig, label: &str) -> Error {
    switch(settled(m), &m.key(label)).expect_err("refused")
}

fn state_of(m: &Rig) -> state::State {
    state::load(&m.ctx).expect("state")
}

// Every read, which opens no jar, lists no process and records nothing.
fn reads_only(m: &Rig, pitboard: &crate::service::Pitboard, why: &str) {
    let asked = |m: &Rig| {
        let state = std::fs::read(crate::home::dir(&m.ctx).join("state.json")).expect("state");
        (m.mem.jars_asked().len(), m.mem.listings(), state)
    };
    // Claude Code's own record, of nobody, is the first read's to make.
    pitboard.status(true).expect("read");
    let before = asked(m);
    pitboard.status_offline().expect("read");
    pitboard.status(true).expect("read");
    pitboard.doctor();
    pitboard.statusline("{}");
    let after = asked(m);
    assert_eq!(after.0, before.0, "{why}: no jar was opened");
    assert_eq!(after.1, before.1, "{why}: no process was listed");
    assert!(after.2 == before.2, "{why}: nothing was recorded");
}

fn killed(m: &Rig, at: &'static str, label: &str) {
    let died = crate::fault::killing(at, || switch(settled(m), &m.key(label)));
    assert_eq!(died.expect_err("killed"), at);
    as_a_dead_runs(m);
}

fn watched(m: &Rig, label: &str) -> crate::error::Result<super::super::WatchedSignIn> {
    crate::service::Pitboard::new(m.ctx.clone())
        .sign_in_watched(&format!("desktop/{label}"))
        .map_err(|failed| failed.error)
}

const SAYS_WITHIN: std::time::Duration = std::time::Duration::from_secs(10);

// Everything the wait has said so far, once it says `until` or its thread has ended.
fn said_until(said: &super::super::Said, until: &str) -> Vec<String> {
    let mut lines = Vec::new();
    loop {
        match said.within(SAYS_WITHIN) {
            Ok(line) => {
                let done = line.contains(until);
                lines.push(line);
                if done {
                    return lines;
                }
            }
            Err(RecvTimeoutError::Disconnected) => return lines,
            Err(RecvTimeoutError::Timeout) => panic!("waiting for {until:?}, said {lines:?}"),
        }
    }
}

// Every line until the wait's thread has ended.
fn said_to_the_end(said: &super::super::Said) -> Vec<String> {
    let mut lines = Vec::new();
    loop {
        match said.within(SAYS_WITHIN) {
            Ok(line) => lines.push(line),
            Err(RecvTimeoutError::Disconnected) => return lines,
            Err(RecvTimeoutError::Timeout) => panic!("the wait goes on, said {lines:?}"),
        }
    }
}

fn gave_up(m: &Rig) -> crate::error::Result<super::super::GaveUp> {
    super::super::abandon(&m.ctx, Permit::for_a_test())
}

// `work` half parked, then Claude signed in to an account nobody enrolled.
fn mixed(name: &str) -> Rig {
    let m = desktop_pair(name);
    killed(&m, "leave.item_out", "home");
    let claude = m.claude();
    claude.opens();
    claude.signs_in(&STRANGER);
    claude.reports();
    claude.quits();
    recover(&m).expect("swept");
    m
}

// `work`'s leave given up with its items left in Claude's folder.
fn left_by_a_give_up(name: &str) -> Rig {
    let m = desktop_pair(name);
    tokens_beside_no_session(&m);
    let gave = gave_up(&m).expect("given up");
    assert!(!gave.moves[0].left.is_empty(), "{gave:?}");
    m
}

// Whose each folder set aside as in the way is, oldest first, as the index and its manifest
// file it.
fn in_the_way(m: &Rig) -> Vec<Option<String>> {
    state_of(m)
        .aside
        .iter()
        .filter(|aside| aside.why == SetAside::InTheWay)
        .map(|aside| {
            let manifest = crate::store::folder::manifest(&m.ctx, &aside.name)
                .expect("reads")
                .expect("there");
            assert_eq!(manifest.account, aside.account);
            aside.account.clone()
        })
        .collect()
}

// What the manifest of `label`'s park lists as left in Claude's folder.
fn listed_left(m: &Rig, label: &str) -> Vec<String> {
    let park = park_of(m, label).expect("parked").service;
    crate::store::folder::manifest(&m.ctx, &park)
        .expect("reads")
        .expect("there")
        .left_in_folder
}

// A listing a give-up wrote, which nothing took since.
fn list_left(m: &Rig, label: &str) {
    let park = park_of(m, label).expect("parked").service;
    let mut manifest = crate::store::folder::manifest(&m.ctx, &park)
        .expect("reads")
        .expect("there");
    manifest.left_in_folder = vec!["Local Storage".into()];
    crate::store::folder::write_manifest(&m.ctx, Permit::for_a_test(), &park, &manifest)
        .expect("written");
}

// Claude Code's `here` in use and `there` parked, with Claude Desktop's pair beside them.
fn three_tools(name: &str) -> Rig {
    beside_desktop(super::super::harness::machine(name))
}

// `m`'s own tool's `here` in use and `there` parked, with Claude Desktop's pair beside them.
fn beside_desktop(mut m: Rig) -> Rig {
    m.ctx = m
        .ctx
        .clone()
        .with_awaited_every(std::time::Duration::from_millis(2))
        .with_scratch_claude(m.ctx_home().join("claude"));
    m.which = ProviderId::Desktop;
    m.claude().signed_in_as(&WORK);
    enrol(&m, "work").expect("`work` enrolled");
    sign_in_inside(&m, "home", &HOME).expect("`home` added");
    used(&m, "work");
    m
}

// A switch of `tool` to `there` killed after its record, before anything moved.
fn interrupted(m: &Rig, tool: ProviderId) {
    let (settled, _) = settle(&m.ctx, Permit::for_a_test(), Some(tool)).expect("nothing waits");
    let died = crate::fault::killing("switch.journal_written", || {
        switch(settled, &Key::new(tool, "there"))
    });
    assert_eq!(died.expect_err("killed"), "switch.journal_written");
    assert!(super::super::journal::pending(&m.ctx));
}

// Claude's data folder and Pitboard's parks, each entry with its bytes.
type Claudes = (
    Vec<(PathBuf, Option<Vec<u8>>)>,
    Vec<(PathBuf, Option<Vec<u8>>)>,
);

fn claudes(m: &Rig) -> Claudes {
    (
        tree(&data(m)),
        tree(&crate::store::folder::parks_dir(&m.ctx)),
    )
}

// The switch given up and said, and the move still waiting, said with why.
fn switch_given_up_beside(m: &Rig, gave: &super::super::GaveUp, why: &str) {
    assert!(gave.switch.is_some(), "{gave:?}");
    assert!(!super::super::journal::pending(&m.ctx));
    assert!(gave.moves.is_empty(), "{gave:?}");
    assert_eq!(
        gave.warnings.iter().map(Warning::code).collect::<Vec<_>>(),
        ["move_not_given_up"]
    );
    let said = gave.warnings[0].to_string();
    assert!(
        said.contains("`desktop/work`") && said.contains(why),
        "{said}"
    );
    assert!(
        !said.to_lowercase().contains("nothing has changed"),
        "{said}"
    );
    assert!(state_of(m).moving(ProviderId::Desktop).is_some());
}

// The change settles the move and refuses. Run again, it refuses alone and changes nothing,
// so the first refusal changed nothing past the settle.
fn settled_then_refused(
    m: &Rig,
    code: &str,
    settled: &str,
    change: impl Fn(&crate::service::Pitboard) -> crate::service::Failed,
) {
    let pitboard = crate::service::Pitboard::new(m.ctx.clone());
    let failed = change(&pitboard);
    assert_eq!(failed.error.code(), code, "{}", failed.error);
    // The service gives the refusal back as itself, as every front end matches it.
    assert!(
        !matches!(failed.error, Error::SaidFirst { .. }),
        "{:?}",
        failed.error
    );
    assert!(
        failed.warnings[..failed.said_first]
            .iter()
            .any(|warning| warning.code() == settled),
        "{code}: {:?}",
        failed.warnings
    );
    assert!(state_of(m).moving(ProviderId::Desktop).is_none(), "{code}");
    let before = snapshot(m);
    let again = change(&pitboard);
    assert_eq!(again.error.code(), code, "{}", again.error);
    assert!(
        !again
            .warnings
            .iter()
            .any(|warning| warning.code().starts_with("interrupted_move_")),
        "{code}: {:?}",
        again.warnings
    );
    unchanged(m, &before, code);
}

// Claude ran as whoever its folder held and renewed that account's tokens.
fn renewed(m: &Rig) {
    let path = data(m).join("config.json");
    let written = std::fs::read_to_string(&path).expect("settings");
    let renewed = [WORK, HOME].iter().fold(written.clone(), |held, who| {
        held.replacen(
            &format!("|{}\"", who.session),
            &format!("|{}-renewed\"", who.session),
            1,
        )
    });
    assert_ne!(renewed, written, "tokens to renew");
    std::fs::write(&path, renewed).expect("Claude's own write");
}

fn superseded(m: &Rig) -> PathBuf {
    state_of(m)
        .aside
        .iter()
        .find(|aside| aside.why == SetAside::Superseded)
        .map(|aside| crate::store::folder::at(&m.ctx, &aside.name))
        .expect("kept aside")
}

#[test]
fn no_folder_name_reaches_the_vault() {
    let m = desktop_pair("no-vault");
    used(&m, "home");
    killed(&m, "leave.jar_out", "work");
    gave_up(&m).expect("given up");
    repair(settled(&m)).expect("repaired");
    forget_(&m, "home").expect("forgotten");
    let asked: Vec<_> = m
        .mem
        .trail()
        .into_iter()
        .filter(|touch| touch.store == "vault")
        .filter(|touch| {
            touch.service.starts_with("pitboard-park-desktop_")
                || touch
                    .service
                    .starts_with(crate::store::folder::ASIDE_PREFIX)
        })
        .collect();
    assert!(asked.is_empty(), "{asked:?}");
    assert!(
        crate::store::folder::list(&m.ctx)
            .expect("parks")
            .is_some_and(|names| !names.is_empty()),
        "folders there were, all the while"
    );
}

// Until the process list has been asked `n` times in all: a wait that stops asking fails.
fn listed(m: &Rig, n: usize) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while m.mem.listings() < n {
        assert!(
            std::time::Instant::now() < deadline,
            "the wait stopped asking the process list"
        );
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}

// A refusal and what it is refused over: set up, then asked.
struct Refusal {
    code: &'static str,
    set_up: fn(&Rig),
    asked: fn(&Rig) -> Error,
}

fn tokens_beside_no_session(m: &Rig) {
    killed(m, "leave.jar_out", "home");
    let park = state_of(m)
        .moving(ProviderId::Desktop)
        .expect("recorded")
        .park
        .clone();
    let kept =
        std::fs::read_to_string(crate::store::folder::keys_file(&m.ctx, &park)).expect("keys");
    let tokens: Map<String, Value> = serde_json::from_str(&kept).expect("JSON");
    set(
        m,
        "oauth:tokenCache",
        tokens.get("oauth:tokenCache").cloned(),
    );
}

// Every place a move of an app's login can be killed, by the name the code gives it.
pub(super) const FOLDER_POINTS: [&str; 30] = [
    "leave.recorded",
    "leave.keys_kept",
    "leave.keys_out",
    "leave.jar_out",
    "leave.item_out",
    "leave.verified",
    "leave.done",
    "arrive.recorded",
    "arrive.aside_kept",
    "arrive.keys_cleared",
    "arrive.item_aside",
    "arrive.item_in",
    "arrive.jar_in",
    "arrive.keys_in",
    "arrive.verified",
    "arrive.done",
    "settings.temp_written",
    "settle.keys_back",
    "settle.aside_named",
    "settle.aside_kept",
    "settle.keys_cleared",
    "settle.item_back",
    "settle.item_aside",
    "settle.saved",
    "give_up.aside_named",
    "give_up.item_aside",
    "give_up.done",
    "start.aside_kept",
    "start.keys_cleared",
    "start.saved",
];

// Claude's data folder on a machine `desktop_machine` made.
pub(super) fn data(m: &Rig) -> PathBuf {
    m.ctx_home().join("claude/data")
}

pub(super) fn settled(m: &Rig) -> Settled {
    settle(&m.ctx, Permit::for_a_test(), Some(ProviderId::Desktop))
        .expect("nothing of a document to recover")
        .0
}

// Every entry under `root`, by its path from there, with each file's bytes.
pub(super) fn tree(root: &std::path::Path) -> Vec<(PathBuf, Option<Vec<u8>>)> {
    fn walk(
        root: &std::path::Path,
        dir: &std::path::Path,
        out: &mut Vec<(PathBuf, Option<Vec<u8>>)>,
    ) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let within = path
                .strip_prefix(root)
                .expect("under the root")
                .to_path_buf();
            let kind = std::fs::symlink_metadata(&path).expect("metadata");
            if kind.is_symlink() {
                let target = std::fs::read_link(&path).expect("a link");
                out.push((within, Some(target.to_string_lossy().as_bytes().to_vec())));
            } else if kind.is_dir() {
                out.push((within, None));
                walk(root, &path, out);
            } else {
                out.push((within, Some(std::fs::read(&path).expect("a file"))));
            }
        }
    }
    let mut found = Vec::new();
    walk(root, root, &mut found);
    found.sort();
    found
}

pub(super) type Stat = (PathBuf, Option<u64>, u64, Option<std::time::SystemTime>);

pub(super) type Snapshot = (
    Vec<u8>,
    Vec<u8>,
    Vec<(PathBuf, Option<Vec<u8>>)>,
    Vec<String>,
    Vec<Stat>,
);

// Claude's folder and each entry under it, links not followed: inode, size and when changed.
pub(super) fn stats(root: &std::path::Path) -> Vec<Stat> {
    let stat = |within: PathBuf| {
        let found = std::fs::symlink_metadata(root.join(&within)).ok();
        let inode = crate::host::fs::testing::inode(&root.join(&within)).ok();
        let size = found.as_ref().map_or(0, std::fs::Metadata::len);
        (
            within,
            inode,
            size,
            found.and_then(|kind| kind.modified().ok()),
        )
    };
    let mut found = vec![stat(PathBuf::new())];
    found.extend(tree(root).into_iter().map(|(within, _)| stat(within)));
    found
}

// What a refusal must leave as it was: the index, `pending`, Claude's folder and `parks`.
pub(super) fn snapshot(m: &Rig) -> Snapshot {
    let home = crate::home::dir(&m.ctx);
    (
        std::fs::read(home.join("state.json")).unwrap_or_default(),
        std::fs::read(home.join("pending")).unwrap_or_default(),
        tree(&data(m)),
        crate::store::folder::list(&m.ctx)
            .expect("parks")
            .unwrap_or_default(),
        stats(&data(m)),
    )
}

pub(super) fn unchanged(m: &Rig, before: &Snapshot, why: &str) {
    let now = snapshot(m);
    assert!(now.0 == before.0, "{why}: state.json changed");
    assert!(now.1 == before.1, "{why}: `pending` changed");
    assert!(now.2 == before.2, "{why}: Claude's folder changed");
    assert!(now.3 == before.3, "{why}: `parks` changed");
    assert!(
        now.4 == before.4,
        "{why}: an entry of Claude's folder was touched"
    );
}

// A kill is a panic here, so a temporary it left is this process's own, which a sweep
// leaves alone: renamed to a dead process's, as the one a killed run leaves is.
pub(super) fn as_a_dead_runs(m: &Rig) {
    let data = data(m);
    for entry in std::fs::read_dir(&data).into_iter().flatten().flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if crate::atomic::temp_owner(&name) == Some(std::process::id()) {
            let dead = name.replace(
                &format!(".{}.", std::process::id()),
                &format!(".{}.", i32::MAX - 1),
            );
            std::fs::rename(entry.path(), data.join(dead)).expect("renamed");
        }
    }
}

// Every folder that holds `who`'s session: Claude's, or one under `parks`.
pub(super) fn whereabouts(m: &Rig, who: &Person) -> Vec<String> {
    sessions(m)
        .into_iter()
        .filter(|(_, login)| *login == who.login())
        .map(|(place, _)| place)
        .collect()
}

// A resting state: no move recorded, no temporary of Pitboard's in Claude's folder, no
// `pending` line for a folder the index names, and Claude's folder holding nobody, or one
// account whole under its own name.
pub(super) fn rested(m: &Rig, after: &str) {
    let crate::provider::LoginKind::Folder(app) = crate::provider::of(m.which) else {
        panic!("{after}: a folder tool");
    };
    // The next command's sweep, which drops each `pending` line the index has taken up.
    recover(m).unwrap_or_else(|e| panic!("{after}: {e}"));
    let state = state::load(&m.ctx).expect("state");
    assert!(
        state.moving(m.which).is_none(),
        "{after}: a move still waits"
    );
    // One session, one place: Claude's folder or one folder under `parks`.
    let mut seen = HashSet::new();
    for (place, login) in sessions(m) {
        assert!(
            seen.insert(login),
            "{after}: the session in {place} is also somewhere else"
        );
    }
    let data = data(m);
    for entry in std::fs::read_dir(&data).into_iter().flatten().flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        assert!(
            crate::atomic::temp_owner(&name).is_none(),
            "{after}: {name} was left in Claude's folder"
        );
    }
    let pending =
        std::fs::read_to_string(crate::home::dir(&m.ctx).join("pending")).unwrap_or_default();
    for line in pending.lines() {
        let name = line.strip_prefix("folder ").unwrap_or(line);
        assert!(!state.names(name), "{after}: `pending` still lists {name}");
    }
    if !data.exists() {
        return;
    }
    let settings = app.settings(&data).expect("settings");
    let jar = data.join(app.items()[0]);
    if !jar.exists() {
        let left: Vec<String> = app
            .items()
            .iter()
            .filter(|item| data.join(item).exists())
            .map(|item| (*item).to_string())
            .collect();
        let listed = crate::store::folder::list(&m.ctx)
            .ok()
            .flatten()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|name| crate::store::folder::manifest(&m.ctx, &name).ok().flatten())
            .any(|manifest| !left.is_empty() && manifest.left_in_folder == left);
        assert!(
            left.is_empty() && settings.account_keys(app).is_empty() || listed,
            "{after}: Claude's folder has no jar and holds {left:?}"
        );
        return;
    }
    let Some(session) = app.session(&m.ctx, &jar).expect("the jar reads") else {
        return;
    };
    let named = settings.named(app);
    for account in state.accounts.iter().filter(|a| a.provider() == m.which) {
        let holds_it = state
            .in_use(m.which)
            .is_some_and(|record| record.login == session.login)
            && state
                .account_in_use(m.which)
                .is_some_and(|a| a.id == account.id);
        if holds_it {
            assert_eq!(
                named.as_deref(),
                Some(account.account_uuid.as_str()),
                "{after}: {}'s session beside another name",
                account.label
            );
        }
    }
}
