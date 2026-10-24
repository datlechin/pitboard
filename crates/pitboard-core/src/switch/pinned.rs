use super::harness::{Rig, codex_access, codex_machine, login_of, machine, recover};
use super::*;
use crate::api::scripted::Asked;
use crate::fault;
use crate::host::{OS, Os};
use crate::permit::Permit;
use crate::store::memory::Touch;
use std::cell::RefCell;
use std::collections::BTreeSet;
use std::path::Path;
use std::rc::Rc;

const LIVE: &str = "Claude Code-credentials";
const CLAUDE_FILE: &str = "file .claude/.credentials.json";
const CLAUDE_HERE: &str = "pitboard-park-here-1760000000000";
const CLAUDE_THERE: &str = "pitboard-park-there-1760000000000";
const CODEX_FILE: &str = "file .codex/auth.json";
const AUTH_JSON: &str = "auth.json";
const CODEX_HERE: &str = "pitboard-park-here_user-here-1760000000000";
const CODEX_THERE: &str = "pitboard-park-there_user-there-1760000000000";
// `store::cost`'s look, which a chain Credential Manager took over skips (`credman_migration`).
const PRICED: &str = "contains, to price";

struct Pinned {
    touched: &'static [(&'static str, &'static str, &'static str)],
    asked: Vec<Asked>,
    at_journal: &'static [&'static str],
    made: &'static [&'static str],
    gone: &'static [&'static str],
}

struct Seen {
    touched: Vec<Touch>,
    asked: Vec<Asked>,
    at_journal: Vec<String>,
    made: Vec<String>,
    gone: Vec<String>,
}

fn pin(m: Rig, what: &str, change: fn(&Rig), pinned: Pinned) {
    let tool = m.which.code();
    let seen = watch(&m, change);
    let touched: Vec<Touch> = pinned
        .touched
        .iter()
        .flat_map(|&(store, call, service)| on_this_system(store, call, service))
        .collect();
    assert_eq!(seen.touched, touched, "{tool} {what}: the stores, in order");
    assert_eq!(seen.asked, pinned.asked, "{tool} {what}: the service");
    assert_eq!(
        seen.at_journal, pinned.at_journal,
        "{tool} {what}: what was new when the journal was written"
    );
    assert_eq!(seen.made, pinned.made, "{tool} {what}: what it made");
    assert_eq!(seen.gone, pinned.gone, "{tool} {what}: what it removed");
}

fn watch(m: &Rig, change: fn(&Rig)) -> Seen {
    let root = m.ctx_home();
    let (touched, asked) = (m.mem.trail().len(), m.api.calls());
    let before = listing(&root);
    let journalled = Rc::new(RefCell::new(before.clone()));
    let (seen, at) = (Rc::clone(&journalled), root.clone());
    fault::meanwhile(
        "switch.journal_written",
        move || *seen.borrow_mut() = listing(&at),
        || change(m),
    );
    let after = listing(&root);
    Seen {
        touched: m.mem.trail()[touched..]
            .iter()
            .map(|touch| under(&root, touch))
            .collect(),
        asked: m.api.asked()[asked..].to_vec(),
        at_journal: journalled.borrow().difference(&before).cloned().collect(),
        made: after.difference(&before).cloned().collect(),
        gone: before.difference(&after).cloned().collect(),
    }
}

// Credential Manager reads `#m` before a look or read, and `#p` and `#m` before a write.
fn on_this_system(store: &str, call: &'static str, service: &str) -> Vec<Touch> {
    let touch = |call, service: String| Touch {
        store: store.into(),
        call,
        service,
    };
    let piece = |name: &str| touch("read", format!("{service}{name}"));
    match (OS, store, call) {
        (Os::Windows, "live", PRICED) => Vec::new(),
        (_, _, PRICED) => vec![touch("contains", service.into())],
        (Os::Windows, "live", "contains" | "read") => {
            vec![piece("#m"), touch("read", service.into())]
        }
        (Os::Windows, "live", "write") => {
            vec![piece("#p"), piece("#m"), touch("write", service.into())]
        }
        _ => vec![touch(call, service.into())],
    }
}

// As a pin writes a path, with `/` between its parts on every system.
fn slashed(path: &Path) -> String {
    path.components()
        .map(|part| part.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

fn under(root: &Path, touch: &Touch) -> Touch {
    let store = match touch.store.strip_prefix("file ") {
        Some(path) => format!(
            "file {}",
            slashed(
                Path::new(path)
                    .strip_prefix(root)
                    .expect("a file under the machine's root")
            )
        ),
        None => touch.store.clone(),
    };
    Touch {
        store,
        ..touch.clone()
    }
}

fn listing(root: &Path) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    let mut left = vec![root.to_path_buf()];
    while let Some(dir) = left.pop() {
        for entry in std::fs::read_dir(&dir).expect("a directory") {
            let path = entry.expect("an entry").path();
            let name = slashed(path.strip_prefix(root).expect("under the root"));
            if std::fs::symlink_metadata(&path)
                .expect("still there")
                .is_dir()
            {
                found.insert(format!("{name}/"));
                left.push(path);
            } else {
                found.insert(name);
            }
        }
    }
    found
}

fn settled(m: &Rig) -> Settled {
    settle(&m.ctx, Permit::for_a_test(), None)
        .expect("nothing to recover")
        .0
}

fn switched(m: &Rig) {
    switch(settled(m), &m.key("there")).expect("switched");
}

fn forgotten(m: &Rig) {
    forget(settled(m), &m.key("there")).expect("forgotten");
}

fn enrolled(m: &Rig) {
    enroll(settled(m), &m.key("third"), None).expect("enrolled");
}

fn recovered(m: &Rig) {
    let settled = settled(m);
    let died = fault::killing("switch.park_recorded", || switch(settled, &m.key("there")));
    assert_eq!(died.unwrap_err(), "switch.park_recorded");
    recover(m).expect("recovered");
}

fn signed_in_as_third(m: Rig) -> Rig {
    m.sign_in(&login_of(&m, "third", "third-refresh"));
    m
}

#[test]
fn a_document_switch_touches_its_stores_in_the_order_it_always_did() {
    pin(
        machine("pinned-switch"),
        "switch",
        switched,
        Pinned {
            touched: &[
                ("live", "contains", LIVE),
                ("live", "read", LIVE),
                ("live", "contains", LIVE),
                ("live", "read", LIVE),
                ("vault", "read", CLAUDE_THERE),
                ("live", "contains", LIVE),
                ("live", "read", LIVE),
                ("live", PRICED, LIVE),
                ("vault", "read", CLAUDE_HERE),
                ("vault", "write", CLAUDE_HERE),
                ("live", "contains", LIVE),
                ("live", "read", LIVE),
                ("live", "contains", LIVE),
                ("live", "write", LIVE),
                ("live", "contains", LIVE),
                ("live", "read", LIVE),
                ("vault", "delete", CLAUDE_THERE),
                ("live", "contains", LIVE),
                (CLAUDE_FILE, "contains", LIVE),
            ],
            asked: vec![Asked::Identity("access-there-refresh".into())],
            at_journal: &[
                ".claude.lock/",
                ".claude/",
                ".claude/.oauth_refresh.lock/",
                ".claude/.storage-write.lock/",
                ".pitboard/journal.json",
                ".pitboard/state.lock",
            ],
            made: &[
                ".claude/",
                ".pitboard/audit.log",
                ".pitboard/backups/",
                ".pitboard/backups/claude.json.1760000000",
                ".pitboard/state.lock",
            ],
            gone: &[],
        },
    );
    pin(
        machine("pinned-forget"),
        "forget",
        forgotten,
        Pinned {
            touched: &[
                ("live", "contains", LIVE),
                ("live", "read", LIVE),
                ("live", "contains", LIVE),
                ("live", "read", LIVE),
                ("vault", "delete", CLAUDE_THERE),
            ],
            asked: vec![],
            at_journal: &[],
            made: &[".pitboard/state.lock"],
            gone: &[".pitboard/pending"],
        },
    );
    pin(
        signed_in_as_third(machine("pinned-enroll")),
        "enroll",
        enrolled,
        Pinned {
            touched: &[
                ("live", "contains", LIVE),
                ("live", "read", LIVE),
                ("live", "contains", LIVE),
                ("live", "read", LIVE),
            ],
            // Which login, then the name of an account Pitboard does not know yet.
            asked: vec![
                Asked::Identity("access-third-refresh".into()),
                Asked::Owner("access-third-refresh".into()),
            ],
            at_journal: &[],
            made: &[".pitboard/audit.log", ".pitboard/state.lock"],
            gone: &[".pitboard/pending"],
        },
    );
    pin(
        machine("pinned-recover"),
        "recover",
        recovered,
        Pinned {
            touched: &[
                ("live", "contains", LIVE),
                ("live", "read", LIVE),
                ("live", "contains", LIVE),
                ("live", "read", LIVE),
                ("vault", "read", CLAUDE_THERE),
                ("live", "contains", LIVE),
                ("live", "read", LIVE),
                ("live", PRICED, LIVE),
                ("vault", "read", CLAUDE_HERE),
                ("vault", "write", CLAUDE_HERE),
                ("live", "contains", LIVE),
                ("live", "read", LIVE),
                ("vault", "read", CLAUDE_HERE),
                ("live", "contains", LIVE),
                ("live", "read", LIVE),
                ("vault", "delete", CLAUDE_HERE),
            ],
            asked: vec![Asked::Identity("access-there-refresh".into())],
            at_journal: &[
                ".claude.lock/",
                ".claude/",
                ".claude/.oauth_refresh.lock/",
                ".claude/.storage-write.lock/",
                ".pitboard/journal.json",
                ".pitboard/state.lock",
            ],
            made: &[".claude/", ".pitboard/state.lock"],
            gone: &[".pitboard/pending"],
        },
    );

    pin(
        codex_machine("pinned-switch"),
        "switch",
        switched,
        Pinned {
            touched: &[
                (CODEX_FILE, "contains", AUTH_JSON),
                (CODEX_FILE, "read", AUTH_JSON),
                (CODEX_FILE, "contains", AUTH_JSON),
                (CODEX_FILE, "read", AUTH_JSON),
                ("vault", "read", CODEX_THERE),
                (CODEX_FILE, "contains", AUTH_JSON),
                (CODEX_FILE, "read", AUTH_JSON),
                (CODEX_FILE, PRICED, AUTH_JSON),
                ("vault", "read", CODEX_HERE),
                ("vault", "write", CODEX_HERE),
                ("vault", "read", CODEX_HERE),
                (CODEX_FILE, "contains", AUTH_JSON),
                (CODEX_FILE, "read", AUTH_JSON),
                (CODEX_FILE, "contains", AUTH_JSON),
                (CODEX_FILE, "write", AUTH_JSON),
                (CODEX_FILE, "contains", AUTH_JSON),
                (CODEX_FILE, "read", AUTH_JSON),
                ("vault", "delete", CODEX_THERE),
            ],
            asked: vec![Asked::Usage(codex_access("there-refresh"))],
            at_journal: &[".pitboard/journal.json", ".pitboard/state.lock"],
            made: &[".pitboard/state.lock"],
            gone: &[],
        },
    );
    pin(
        codex_machine("pinned-forget"),
        "forget",
        forgotten,
        Pinned {
            touched: &[
                (CODEX_FILE, "contains", AUTH_JSON),
                (CODEX_FILE, "read", AUTH_JSON),
                (CODEX_FILE, "contains", AUTH_JSON),
                (CODEX_FILE, "read", AUTH_JSON),
                ("vault", "delete", CODEX_THERE),
            ],
            asked: vec![],
            at_journal: &[],
            made: &[".pitboard/state.lock"],
            gone: &[".pitboard/pending"],
        },
    );
    pin(
        signed_in_as_third(codex_machine("pinned-enroll")),
        "enroll",
        enrolled,
        Pinned {
            touched: &[
                (CODEX_FILE, "contains", AUTH_JSON),
                (CODEX_FILE, "read", AUTH_JSON),
                (CODEX_FILE, "contains", AUTH_JSON),
                (CODEX_FILE, "read", AUTH_JSON),
            ],
            asked: vec![],
            at_journal: &[],
            made: &[".pitboard/audit.log", ".pitboard/state.lock"],
            gone: &[".pitboard/pending"],
        },
    );
    pin(
        codex_machine("pinned-recover"),
        "recover",
        recovered,
        Pinned {
            touched: &[
                (CODEX_FILE, "contains", AUTH_JSON),
                (CODEX_FILE, "read", AUTH_JSON),
                (CODEX_FILE, "contains", AUTH_JSON),
                (CODEX_FILE, "read", AUTH_JSON),
                ("vault", "read", CODEX_THERE),
                (CODEX_FILE, "contains", AUTH_JSON),
                (CODEX_FILE, "read", AUTH_JSON),
                (CODEX_FILE, PRICED, AUTH_JSON),
                ("vault", "read", CODEX_HERE),
                ("vault", "write", CODEX_HERE),
                (CODEX_FILE, "contains", AUTH_JSON),
                (CODEX_FILE, "read", AUTH_JSON),
                ("vault", "read", CODEX_HERE),
                (CODEX_FILE, "contains", AUTH_JSON),
                (CODEX_FILE, "read", AUTH_JSON),
                ("vault", "delete", CODEX_HERE),
            ],
            asked: vec![Asked::Usage(codex_access("there-refresh"))],
            at_journal: &[".pitboard/journal.json", ".pitboard/state.lock"],
            made: &[".pitboard/state.lock"],
            gone: &[".pitboard/pending"],
        },
    );
}
