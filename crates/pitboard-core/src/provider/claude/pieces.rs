// How Claude Code keeps its login in Credential Manager on Windows (`credman_chunking`): in the
// item itself up to 2400 bytes, above that as base64 pieces `#0` to `#n-1` of 2400 characters
// with a manifest `#m` naming how many and how long, and `#p` naming how many pieces a write
// under way may leave. Each is an item of its own, named after the user name.

use crate::permit::Permit;
use crate::store::Error;
use base64::prelude::{BASE64_STANDARD, Engine as _};

pub(crate) const ITEM_BYTES: usize = 2400;
pub(crate) const PIECE: usize = 2400;
pub(crate) const MOST_PIECES: usize = 256;
// What 256 pieces of base64 hold: Claude Code's "max ~460800B".
pub(crate) const MOST_BYTES: usize = MOST_PIECES * PIECE / 4 * 3;
// What one Credential Manager item holds, `CRED_MAX_CREDENTIAL_BLOB_SIZE`, as
// host::windows::credman's an_item_holds_2560_bytes_and_no_more measures.
#[cfg(any(windows, test, feature = "test-support"))]
pub(crate) const BLOB: usize = 5 * 512;

pub(crate) const WHOLE: &str = "";
pub(crate) const MANIFEST: &str = "#m";
pub(crate) const PENDING: &str = "#p";

pub(crate) fn piece(k: usize) -> String {
    format!("#{k}")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Manifest {
    pub(crate) n: usize,
    pub(crate) l: usize,
}

impl Manifest {
    // As Claude Code takes one: integers with 0 < n <= 256 and 0 < l <= n * 2400.
    pub(crate) fn read(text: &str) -> Option<Manifest> {
        let value: serde_json::Value = serde_json::from_str(text).ok()?;
        let whole = |key: &str| {
            let number = value.get(key)?.as_f64()?;
            let fits =
                number.fract() == 0.0 && (1.0..=(MOST_PIECES * PIECE) as f64).contains(&number);
            fits.then_some(number as usize)
        };
        let (n, l) = (whole("n")?, whole("l")?);
        (n <= MOST_PIECES && l <= n * PIECE).then_some(Manifest { n, l })
    }

    // As `JSON.stringify` gives it.
    pub(crate) fn text(self) -> String {
        format!(r#"{{"n":{},"l":{}}}"#, self.n, self.l)
    }
}

// How many pieces a login of `bytes` takes, none for the item alone; None past 256.
pub(crate) fn pieces_for(bytes: usize) -> Option<usize> {
    if bytes <= ITEM_BYTES {
        return Some(0);
    }
    let pieces = (bytes.div_ceil(3) * 4).div_ceil(PIECE);
    (pieces <= MOST_PIECES).then_some(pieces)
}

// One item's change, in the order Claude Code makes them. `Forget` is a delete whose failure it
// ignores.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Step {
    Set(String, String),
    Delete(String),
    Forget(String),
}

// Claude Code's write over what `old` names and the `left` pieces a stopped write's `#p` names,
// None past 256 pieces. One step is Pitboard's own: where pieces are replaced in place, the
// manifest first names a length no base64 has, so Claude Code's read fails and keeps the login
// it holds rather than join old pieces and new.
pub(crate) fn steps(old: Option<Manifest>, left: usize, contents: &str) -> Option<Vec<Step>> {
    let set = |name: &str, value: String| Step::Set(name.to_owned(), value);
    let old_n = old.map_or(0, |old| old.n).max(left);
    let pieces = pieces_for(contents.len())?;
    if pieces == 0 {
        let mut steps = vec![set(WHOLE, contents.to_owned())];
        if old.is_some() {
            steps.push(set(PENDING, old_n.to_string()));
            steps.push(Step::Delete(MANIFEST.to_owned()));
            steps.extend((0..old_n).map(|k| Step::Forget(piece(k))));
            steps.push(Step::Forget(PENDING.to_owned()));
        }
        return Some(steps);
    }
    let encoded = BASE64_STANDARD.encode(contents.as_bytes());
    let mut steps = vec![set(PENDING, old_n.max(pieces).to_string())];
    if old.is_some() {
        let n = old_n.max(pieces);
        steps.push(set(MANIFEST, Manifest { n, l: 1 }.text()));
    }
    steps.extend(
        encoded
            .as_bytes()
            .chunks(PIECE)
            .enumerate()
            .map(|(k, part)| set(&piece(k), String::from_utf8_lossy(part).into_owned())),
    );
    steps.push(set(
        MANIFEST,
        Manifest {
            n: pieces,
            l: encoded.len(),
        }
        .text(),
    ));
    steps.push(Step::Forget(WHOLE.to_owned()));
    steps.extend((pieces..old_n).map(|k| Step::Forget(piece(k))));
    steps.push(Step::Forget(PENDING.to_owned()));
    Some(steps)
}

// One item each, by its name after the user name's: `WHOLE`, `#<k>`, `MANIFEST`, `PENDING`.
pub(crate) trait Items {
    fn get(&self, name: &str) -> Result<Option<String>, Error>;
    fn set(&self, permit: Permit, name: &str, value: &str) -> Result<(), Error>;
    fn delete(&self, permit: Permit, name: &str) -> Result<(), Error>;
    // How a hint names `claude` here, which a read nothing can join says to run.
    fn spelling(&self) -> crate::words::Spelling;
}

// Claude Code takes an item holding nothing for one that is not there.
fn found(items: &dyn Items, name: &str) -> Result<Option<String>, Error> {
    Ok(items.get(name)?.filter(|held| !held.is_empty()))
}

// As Claude Code reads it before a write or a delete: one that is no manifest is none.
fn manifest(items: &dyn Items) -> Result<Option<Manifest>, Error> {
    Ok(found(items, MANIFEST)?.and_then(|text| Manifest::read(&text)))
}

// Whether Claude Code's read finds a login here, or Pitboard's finds one it refuses.
pub(crate) fn holds(items: &dyn Items) -> Result<bool, Error> {
    Ok(found(items, MANIFEST)?.is_some() || found(items, WHOLE)?.is_some())
}

// Claude Code's read, stricter where it would follow what no write of its makes, or join the
// pieces of a write it did not see end.
pub(crate) fn read(items: &dyn Items, target: &str) -> Result<Option<String>, Error> {
    let Some(text) = found(items, MANIFEST)? else {
        return found(items, WHOLE);
    };
    let Some(manifest) = Manifest::read(&text) else {
        return Err(broken(
            items,
            format!("`{target}` has a manifest `#m` Claude Code does not write"),
        ));
    };
    if found(items, PENDING)?.is_some() {
        return Err(Error::Unfinished(format!(
            "`{target}` has pieces a write left unfinished, as its `#p` says"
        )));
    }
    joined(items, target, manifest).map(Some)
}

fn broken(items: &dyn Items, why: String) -> Error {
    Error::Broken {
        why,
        spelling: items.spelling(),
    }
}

// The login the pieces `manifest` names hold, whatever `#p` says.
fn joined(items: &dyn Items, target: &str, manifest: Manifest) -> Result<String, Error> {
    let unreadable = |why: String| Err(broken(items, format!("`{target}` {why}")));
    let mut joined = String::with_capacity(manifest.l);
    for k in 0..manifest.n {
        let Some(part) = items.get(&piece(k))? else {
            return unreadable(format!(
                "lacks piece #{k} of the {} its manifest names",
                manifest.n
            ));
        };
        joined.push_str(&part);
    }
    if joined.len() != manifest.l {
        return unreadable(format!(
            "has {} characters in its pieces, where its manifest names {}",
            joined.len(),
            manifest.l
        ));
    }
    BASE64_STANDARD
        .decode(&joined)
        .ok()
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .ok_or_else(|| {
            broken(
                items,
                format!("`{target}` has pieces that are not base64 of text"),
            )
        })
}

// Each step is a durable change a test can stop a write after.
pub(crate) fn write(
    items: &dyn Items,
    permit: Permit,
    contents: &str,
    target: &str,
) -> Result<(), Error> {
    let left = found(items, PENDING)?
        .and_then(|text| text.trim().parse::<usize>().ok())
        .map_or(0, |n| n.min(MOST_PIECES));
    let steps = steps(manifest(items)?, left, contents).ok_or_else(|| {
        Error::Write(format!(
            "this login is {}",
            crate::words::past_credential_manager(contents.len(), MOST_BYTES)
        ))
    })?;
    let in_pieces = matches!(steps.first(), Some(Step::Set(name, _)) if name == PENDING);
    for step in steps {
        match step {
            Step::Set(name, value) => items.set(permit, &name, &value)?,
            Step::Delete(name) => items.delete(permit, &name)?,
            Step::Forget(name) => {
                let _ = items.delete(permit, &name);
            }
        }
        crate::fault::point("credman.step");
    }
    // The item read itself back; pieces are read back whole.
    if !in_pieces {
        return Ok(());
    }
    // Credential Manager can still show `#p` just after its delete (runs 39137503718, 39909919492).
    let ours = || {
        matches!(manifest(items), Ok(Some(named))
            if joined(items, target, named).is_ok_and(|held| held == contents))
    };
    let mut deleted = Ok(());
    let mut again = 0;
    let back = loop {
        let back = read(items, target);
        if again == 5 || !matches!(back, Err(Error::Unfinished(_))) || !ours() {
            break back;
        }
        again += 1;
        deleted = items.delete(permit, PENDING);
        std::thread::sleep(std::time::Duration::from_millis(20));
    };
    match back {
        Ok(Some(back)) if back == contents => Ok(()),
        Ok(Some(_)) => Err(Error::NotDurable(format!(
            "`{target}` holds different bytes"
        ))),
        Ok(None) => Err(Error::NotDurable(format!("`{target}` reads back empty"))),
        Err(Error::Unfinished(why)) => Err(Error::Unfinished(match deleted {
            Ok(()) => why,
            Err(failed) => format!("{why}; deleting it failed: {failed}"),
        })),
        Err(other) => Err(other),
    }
}

// Claude Code's delete: the item, the pieces its manifest names, then as many as `#p` says may be
// left, or where it says none, each piece found from `#0` on.
pub(crate) fn delete(items: &dyn Items, permit: Permit) -> Result<(), Error> {
    let named = manifest(items)?;
    let named_n = named.map_or(0, |named| named.n);
    items.delete(permit, WHOLE)?;
    if named.is_some() {
        for k in 0..named_n {
            items.delete(permit, &piece(k))?;
        }
        items.delete(permit, MANIFEST)?;
    }
    let pending = found(items, PENDING)
        .ok()
        .flatten()
        .and_then(|text| text.trim().parse::<usize>().ok())
        .map_or(0, |n| n.min(MOST_PIECES));
    let mut left = named_n.max(pending);
    if left == 0 {
        while left < MOST_PIECES && matches!(items.get(&piece(left)), Ok(Some(_))) {
            left += 1;
        }
    }
    let _ = items.delete(permit, MANIFEST);
    let _ = items.delete(permit, PENDING);
    for k in named_n..left {
        let _ = items.delete(permit, &piece(k));
    }
    Ok(())
}

// Claude Code's own read, as it is in 2.1.301: a manifest it does not take is none, and `#p` is
// never read. `Err` is a read that throws, after which a session keeps what it holds.
#[cfg(test)]
pub(crate) fn claude_code_reads(items: &dyn Items) -> Result<Option<String>, ()> {
    let held = |name: &str| items.get(name).map_err(drop);
    let manifest = held(MANIFEST)?
        .filter(|m| !m.is_empty())
        .and_then(|m| Manifest::read(&m));
    let Some(manifest) = manifest else {
        return Ok(held(WHOLE)?.filter(|w| !w.is_empty()));
    };
    let parts = (0..manifest.n)
        .map(|k| held(&piece(k)))
        .collect::<Result<Vec<_>, ()>>()?;
    let joined: String = parts.iter().flatten().cloned().collect();
    if joined.len() != manifest.l || parts.iter().any(Option::is_none) {
        return Err(());
    }
    let bytes = BASE64_STANDARD.decode(&joined).map_err(drop)?;
    String::from_utf8(bytes).map(Some).map_err(drop)
}

// A login of `bytes`, all of it its access token.
#[cfg(test)]
pub(crate) fn login(bytes: usize) -> String {
    let mut text = String::from("{\"claudeAiOauth\":{\"accessToken\":\"");
    let fill = bytes - text.len() - 4;
    text.extend(std::iter::repeat_n('a', fill));
    text.push_str("\"}}\n");
    assert_eq!(text.len(), bytes);
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};
    use std::collections::BTreeMap;

    #[derive(Default)]
    struct Kept(RefCell<BTreeMap<String, String>>);

    impl Items for Kept {
        fn get(&self, name: &str) -> Result<Option<String>, Error> {
            Ok(self.0.borrow().get(name).cloned())
        }

        fn set(&self, _: Permit, name: &str, value: &str) -> Result<(), Error> {
            self.0.borrow_mut().insert(name.into(), value.into());
            Ok(())
        }

        fn delete(&self, _: Permit, name: &str) -> Result<(), Error> {
            self.0.borrow_mut().remove(name);
            Ok(())
        }

        fn spelling(&self) -> crate::words::Spelling {
            crate::words::Spelling::Bare
        }
    }

    impl Kept {
        fn names(&self) -> Vec<String> {
            self.0.borrow().keys().cloned().collect()
        }
    }

    #[test]
    fn a_login_takes_the_item_to_2400_bytes_and_pieces_of_2400_characters_to_256() {
        assert_eq!(pieces_for(0), Some(0));
        assert_eq!(pieces_for(2399), Some(0));
        assert_eq!(pieces_for(2400), Some(0));
        assert_eq!(pieces_for(2401), Some(2), "3204 characters");
        assert_eq!(pieces_for(3600), Some(2));
        assert_eq!(
            pieces_for(5400),
            Some(3),
            "7200 characters, three whole pieces"
        );
        assert_eq!(pieces_for(5401), Some(4));
        assert_eq!(pieces_for(MOST_BYTES), Some(256));
        assert_eq!(pieces_for(MOST_BYTES + 1), None);
        assert_eq!(MOST_BYTES, 460_800);
    }

    #[test]
    fn a_manifest_is_one_claude_code_takes() {
        assert_eq!(
            Manifest::read(r#"{"n":2,"l":3204}"#),
            Some(Manifest { n: 2, l: 3204 })
        );
        assert_eq!(
            Manifest::read(r#"{"l":3204,"n":2.0,"v":1}"#),
            Some(Manifest { n: 2, l: 3204 })
        );
        assert_eq!(Manifest { n: 2, l: 3204 }.text(), r#"{"n":2,"l":3204}"#);
        for bad in [
            r#"{"n":0,"l":1}"#,
            r#"{"n":257,"l":1}"#,
            r#"{"n":1,"l":2401}"#,
            r#"{"n":1,"l":0}"#,
            r#"{"n":1.5,"l":3}"#,
            r#"{"n":"1","l":3}"#,
            r#"[1,3]"#,
            "",
        ] {
            assert_eq!(Manifest::read(bad), None, "{bad}");
        }
    }

    #[test]
    fn each_size_reads_back_and_shrinking_leaves_the_item_alone() {
        let items = Kept::default();
        let target = "Claude Code-credentials-00000000/claude-code-user";
        for bytes in [2399, 2400, 2401, 9000, MOST_BYTES, 2400, 100] {
            let text = login(bytes);
            write(&items, Permit::for_a_test(), &text, target).expect("written");
            assert_eq!(
                read(&items, target).expect("read").as_deref(),
                Some(text.as_str())
            );
            assert_eq!(claude_code_reads(&items), Ok(Some(text.clone())), "{bytes}");
            let pieces = pieces_for(bytes).expect("fits");
            let mut named: Vec<String> = (0..pieces).map(piece).collect();
            if pieces == 0 {
                named.push(WHOLE.into());
            } else {
                named.push(MANIFEST.into());
            }
            named.sort();
            assert_eq!(items.names(), named, "{bytes}: nothing else is left");
        }
        assert!(matches!(
            write(&items, Permit::for_a_test(), &login(MOST_BYTES + 1), target),
            Err(Error::Write(_))
        ));
        assert_eq!(items.names(), [WHOLE], "a login too large writes nothing");
    }

    #[test]
    fn a_write_runs_in_claude_codes_order() {
        let small = |steps: Vec<Step>| -> Vec<String> {
            steps
                .into_iter()
                .map(|step| match step {
                    Step::Set(name, _) => format!("set {name}"),
                    Step::Delete(name) => format!("delete {name}"),
                    Step::Forget(name) => format!("forget {name}"),
                })
                .collect()
        };
        assert_eq!(small(steps(None, 0, "{}").unwrap()), ["set "]);
        assert_eq!(
            small(steps(Some(Manifest { n: 2, l: 3204 }), 0, "{}").unwrap()),
            [
                "set ",
                "set #p",
                "delete #m",
                "forget #0",
                "forget #1",
                "forget #p"
            ]
        );
        assert_eq!(
            small(steps(None, 0, &login(2401)).unwrap()),
            [
                "set #p",
                "set #0",
                "set #1",
                "set #m",
                "forget ",
                "forget #p"
            ]
        );
        assert_eq!(
            small(steps(Some(Manifest { n: 3, l: 7200 }), 0, &login(2401)).unwrap()),
            [
                "set #p",
                "set #m",
                "set #0",
                "set #1",
                "set #m",
                "forget ",
                "forget #2",
                "forget #p"
            ]
        );
        let shrinking = steps(Some(Manifest { n: 3, l: 7200 }), 0, &login(2401)).unwrap();
        assert_eq!(shrinking[0], Step::Set(PENDING.into(), "3".into()));
        assert_eq!(
            shrinking[1],
            Step::Set(MANIFEST.into(), r#"{"n":3,"l":1}"#.into())
        );
        assert_eq!(
            small(steps(None, 4, &login(2401)).unwrap()),
            [
                "set #p",
                "set #0",
                "set #1",
                "set #m",
                "forget ",
                "forget #2",
                "forget #3",
                "forget #p"
            ]
        );
    }

    // From every shape Claude Code leaves to every other, the same lengths too: wherever a write
    // stops, Claude Code reads the login it had, the one written, or fails and keeps its own.
    #[test]
    fn a_write_stopped_after_any_step_leaves_no_login_half_written() {
        let shapes = [
            login(300),
            login(2401),
            login(2401).replace('a', "b"),
            login(9000),
        ];
        for before in &shapes {
            for after in &shapes {
                let steps_taken = {
                    let items = Kept::default();
                    write(&items, Permit::for_a_test(), before, "t").unwrap();
                    crate::fault::counting("credman.step", || {
                        write(&items, Permit::for_a_test(), after, "t")
                    })
                    .1
                };
                for stop in 1..=steps_taken {
                    let items = Kept::default();
                    write(&items, Permit::for_a_test(), before, "t").unwrap();
                    let died = crate::fault::killing_nth("credman.step", stop, || {
                        write(&items, Permit::for_a_test(), after, "t")
                    });
                    if stop < steps_taken {
                        assert_eq!(died.unwrap_err(), "credman.step");
                    }
                    match claude_code_reads(&items) {
                        Ok(Some(read)) => assert!(
                            read == *before || read == *after,
                            "{} to {} stopped at {stop}: a login half written",
                            before.len(),
                            after.len()
                        ),
                        Ok(None) => panic!(
                            "{} to {} stopped at {stop}: signed out",
                            before.len(),
                            after.len()
                        ),
                        Err(()) => {}
                    }
                    match read(&items, "t") {
                        Ok(Some(read)) => assert!(read == *before || read == *after),
                        Ok(None) => panic!("Pitboard reads no login"),
                        Err(Error::Unfinished(_)) => {
                            write(&items, Permit::for_a_test(), after, "t")
                                .expect("a whole write finishes it");
                            assert_eq!(read(&items, "t").unwrap().as_ref(), Some(after));
                            assert_eq!(claude_code_reads(&items), Ok(Some(after.clone())));
                        }
                        Err(other) => panic!("{other:?}"),
                    }
                }
            }
        }
    }

    // A stopped write that was to end with fewer pieces, or that began with none, leaves pieces
    // past the count the next write reads; that write takes every one of them away.
    #[test]
    fn a_write_after_a_stopped_one_leaves_no_piece_behind() {
        for (before, after) in [(login(9000), login(2401)), (login(100), login(9000))] {
            let steps = {
                let items = Kept::default();
                write(&items, Permit::for_a_test(), &before, "t").unwrap();
                crate::fault::counting("credman.step", || {
                    write(&items, Permit::for_a_test(), &after, "t")
                })
                .1
            };
            for stop in 1..steps {
                let items = Kept::default();
                write(&items, Permit::for_a_test(), &before, "t").unwrap();
                let _ = crate::fault::killing_nth("credman.step", stop, || {
                    write(&items, Permit::for_a_test(), &after, "t")
                });
                let next = login(2401);
                write(&items, Permit::for_a_test(), &next, "t").unwrap();
                assert_eq!(read(&items, "t").unwrap(), Some(next));
                assert_eq!(
                    items.names(),
                    ["#0", "#1", MANIFEST],
                    "{} to {} stopped at {stop}",
                    before.len(),
                    after.len()
                );
            }
        }
    }

    // Credential Manager beside another reader: a delete that fails, a read that lags.
    #[derive(Default)]
    struct Lagging {
        kept: Kept,
        failing_deletes: Cell<usize>,
        stale_reads: Cell<usize>,
        deleted: RefCell<Option<String>>,
    }

    impl Items for Lagging {
        fn get(&self, name: &str) -> Result<Option<String>, Error> {
            let stale = self.deleted.borrow().clone();
            if name == PENDING && self.stale_reads.get() > 0 && stale.is_some() {
                self.stale_reads.set(self.stale_reads.get() - 1);
                return Ok(stale);
            }
            self.kept.get(name)
        }

        fn set(&self, permit: Permit, name: &str, value: &str) -> Result<(), Error> {
            self.kept.set(permit, name, value)
        }

        fn delete(&self, permit: Permit, name: &str) -> Result<(), Error> {
            if name == PENDING && self.failing_deletes.get() > 0 {
                self.failing_deletes.set(self.failing_deletes.get() - 1);
                return Err(Error::Write("`t#p`: Access is denied. (os error 5)".into()));
            }
            if name == PENDING
                && let Some(held) = self.kept.get(name)?
            {
                *self.deleted.borrow_mut() = Some(held);
            }
            self.kept.delete(permit, name)
        }

        fn spelling(&self) -> crate::words::Spelling {
            crate::words::Spelling::Bare
        }
    }

    #[test]
    fn a_write_in_pieces_deletes_its_marker_again_where_credential_manager_still_shows_it() {
        let text = login(9000);
        let pieces = ["#0", "#1", "#2", "#3", "#4", MANIFEST];
        let stale = Lagging::default();
        stale.stale_reads.set(1);
        write(&stale, Permit::for_a_test(), &text, "t").expect("a read that lagged");
        assert_eq!(read(&stale, "t").unwrap().as_ref(), Some(&text));
        assert_eq!(stale.kept.names(), pieces);

        let refused = Lagging::default();
        refused.failing_deletes.set(1);
        write(&refused, Permit::for_a_test(), &text, "t").expect("a delete refused once");
        assert_eq!(refused.kept.names(), pieces);

        let kept = Lagging::default();
        kept.failing_deletes.set(usize::MAX);
        let Err(Error::Unfinished(why)) = write(&kept, Permit::for_a_test(), &text, "t") else {
            panic!("a `#p` no delete takes away");
        };
        assert_eq!(
            why,
            "`t` has pieces a write left unfinished, as its `#p` says; deleting it failed: \
             writing the credential failed: `t#p`: Access is denied. (os error 5)"
        );
    }

    #[test]
    fn a_marker_beside_pieces_reads_unfinished_and_a_bad_manifest_unreadable() {
        let items = Kept::default();
        write(&items, Permit::for_a_test(), &login(9000), "t").unwrap();
        items.set(Permit::for_a_test(), PENDING, "4").unwrap();
        let Err(Error::Unfinished(why)) = read(&items, "t") else {
            panic!("a leftover #p beside pieces");
        };
        assert_eq!(
            why,
            "`t` has pieces a write left unfinished, as its `#p` says"
        );
        items.delete(Permit::for_a_test(), PENDING).unwrap();
        items
            .set(Permit::for_a_test(), MANIFEST, r#"{"n":9,"l":1}"#)
            .unwrap();
        assert!(matches!(read(&items, "t"), Err(Error::Broken { .. })));
        items
            .set(Permit::for_a_test(), MANIFEST, "not a manifest")
            .unwrap();
        let Err(Error::Broken { why, .. }) = read(&items, "t") else {
            panic!("a bad #m");
        };
        assert_eq!(why, "`t` has a manifest `#m` Claude Code does not write");
        assert!(holds(&items).unwrap());

        // A small write never clears a `#p` it finds no manifest beside, nor does Claude Code's.
        let items = Kept::default();
        items.set(Permit::for_a_test(), PENDING, "2").unwrap();
        write(&items, Permit::for_a_test(), "{}", "t").unwrap();
        assert_eq!(read(&items, "t").unwrap().as_deref(), Some("{}"));
        assert_eq!(items.names(), [WHOLE, PENDING]);
        items.set(Permit::for_a_test(), WHOLE, "").unwrap();
        assert!(!holds(&items).unwrap(), "an item holding nothing is none");
        assert_eq!(read(&items, "t").unwrap(), None);
    }

    #[test]
    fn a_delete_takes_every_piece_a_manifest_or_a_marker_names_or_it_finds() {
        for (shape, extra) in [
            (login(9000), None),
            (login(100), Some(("#p", "3"))),
            (login(100), None),
        ] {
            let items = Kept::default();
            write(&items, Permit::for_a_test(), &shape, "t").unwrap();
            for k in 0..3 {
                items
                    .set(Permit::for_a_test(), &piece(k + 20), "left")
                    .unwrap();
            }
            if let Some((name, value)) = extra {
                items.set(Permit::for_a_test(), name, value).unwrap();
                for k in 0..3 {
                    items.set(Permit::for_a_test(), &piece(k), "left").unwrap();
                }
            }
            delete(&items, Permit::for_a_test()).unwrap();
            let left: Vec<String> = items.names();
            assert!(
                left.iter().all(|name| name.starts_with("#2")),
                "{left:?}: only pieces past a gap stay, as Claude Code leaves them"
            );
        }
        let items = Kept::default();
        for k in 0..5 {
            items.set(Permit::for_a_test(), &piece(k), "left").unwrap();
        }
        delete(&items, Permit::for_a_test()).unwrap();
        assert!(
            items.names().is_empty(),
            "pieces from #0 on, found one by one"
        );
    }
}
