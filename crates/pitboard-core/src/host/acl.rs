// What a Windows access control list says, read apart from the system so every system's
// tests read it, and the `icacls` line that makes a path private on Windows.

use super::Access;

pub(crate) const SYSTEM: &str = "S-1-5-18";
pub(crate) const ADMINISTRATORS: &str = "S-1-5-32-544";
pub(crate) const EVERYONE: &str = "S-1-1-0";
pub(crate) const USERS: &str = "S-1-5-32-545";

#[cfg(any(test, feature = "test-support"))]
const OBJECT_INHERIT: u8 = 0x01;
#[cfg(any(test, feature = "test-support"))]
const CONTAINER_INHERIT: u8 = 0x02;
const INHERIT_ONLY: u8 = 0x08;

// FILE_ALL_ACCESS, what SDDL's `FA` and `icacls`'s `F` grant.
pub(crate) const ALL: u32 = 0x001F_01FF;
const READ_DATA: u32 = 0x0001;
const WRITE_DATA: u32 = 0x0002;
const APPEND_DATA: u32 = 0x0004;
// Either lets its holder grant itself the rest.
const WRITE_DAC: u32 = 0x0004_0000;
const WRITE_OWNER: u32 = 0x0008_0000;
const REACHES_DATA: u32 = READ_DATA | WRITE_DATA | APPEND_DATA | WRITE_DAC | WRITE_OWNER;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Ace {
    pub(crate) allows: bool,
    pub(crate) sid: String,
    pub(crate) mask: u32,
    pub(crate) flags: u8,
}

// A file's generic rights as the file system maps them.
fn specific(mask: u32) -> u32 {
    let mapped = [
        (0x8000_0000, 0x0012_0089),
        (0x4000_0000, 0x0012_0116),
        (0x2000_0000, 0x0012_00A0),
        (0x1000_0000, ALL),
    ];
    mapped
        .into_iter()
        .filter(|(generic, _)| mask & generic != 0)
        .fold(mask & 0x0FFF_FFFF, |rights, (_, file)| rights | file)
}

// A deny ACE is honoured for its own SID alone: whether another account is in a group
// cannot be read here, so it is never taken to cover one. `None` is a NULL DACL.
pub(crate) fn access(dacl: Option<&[Ace]>, you: Option<&str>) -> Access {
    let Some(aces) = dacl else {
        return Access {
            shared: true,
            described: "open to everyone, with no access list".into(),
        };
    };
    let mut denied: Vec<(&str, u32)> = Vec::new();
    let mut reaching: Vec<&str> = Vec::new();
    for ace in aces.iter().filter(|ace| ace.flags & INHERIT_ONLY == 0) {
        let rights = specific(ace.mask) & REACHES_DATA;
        let earlier = denied
            .iter()
            .filter(|(sid, _)| *sid == ace.sid)
            .fold(0, |all, (_, rights)| all | rights);
        if !ace.allows {
            denied.push((ace.sid.as_str(), rights));
        } else if rights & !earlier != 0 && !reaching.contains(&ace.sid.as_str()) {
            reaching.push(&ace.sid);
        }
    }
    let own = |sid: &str| you == Some(sid) || sid == SYSTEM || sid == ADMINISTRATORS;
    let others: Vec<&str> = reaching.iter().copied().filter(|sid| !own(sid)).collect();
    let mut names: Vec<String> = others
        .iter()
        .filter_map(|sid| named(sid))
        .map(str::to_owned)
        .collect();
    match others.iter().filter(|sid| named(sid).is_none()).count() {
        0 => {}
        1 => names.push("another account".into()),
        n => names.push(format!("{n} other accounts")),
    }
    for (sid, name) in [
        (you, "you"),
        (Some(SYSTEM), "SYSTEM"),
        (Some(ADMINISTRATORS), "Administrators"),
    ] {
        if sid.is_some_and(|sid| reaching.contains(&sid)) {
            names.push(name.into());
        }
    }
    Access {
        shared: !others.is_empty(),
        described: format!("open to {}", listed(&names)),
    }
}

// As Windows' Security tab names each.
fn named(sid: &str) -> Option<&'static str> {
    Some(match sid {
        EVERYONE => "Everyone",
        USERS => "Users",
        "S-1-5-11" => "Authenticated Users",
        "S-1-5-32-546" => "Guests",
        "S-1-5-4" => "INTERACTIVE",
        "S-1-5-2" => "NETWORK",
        "S-1-5-7" => "ANONYMOUS LOGON",
        "S-1-15-2-1" => "ALL APPLICATION PACKAGES",
        "S-1-3-0" => "CREATOR OWNER",
        _ if sid.starts_with("S-1-5-21-") && sid.ends_with("-513") => "Domain Users",
        _ => return None,
    })
}

fn listed(names: &[String]) -> String {
    if names.is_empty() {
        "nobody".into()
    } else {
        crate::words::listed(names)
    }
}

// What Pitboard makes its own files and folders with: owned by the person, and reached
// by them, SYSTEM and Administrators alone, whatever the folder above allows.
pub(crate) fn private(sid: &str, folder: bool) -> String {
    format!("O:{sid}D:P{}", private_aces(sid, folder))
}

pub(crate) fn private_aces(sid: &str, folder: bool) -> String {
    let inherit = if folder { "OICI" } else { "" };
    [sid, "SY", "BA"]
        .map(|who| format!("(A;{inherit};FA;;;{who})"))
        .concat()
}

// Exactly what `private` gives, in any order.
#[cfg(any(test, feature = "test-support"))]
pub(crate) fn is_private(protected: bool, aces: &[Ace], you: &str, folder: bool) -> bool {
    let flags = if folder {
        OBJECT_INHERIT | CONTAINER_INHERIT
    } else {
        0
    };
    let wanted = [you, SYSTEM, ADMINISTRATORS];
    protected
        && aces.len() == wanted.len()
        && wanted.iter().all(|who| {
            aces.iter()
                .any(|ace| ace.allows && ace.sid == *who && ace.mask == ALL && ace.flags == flags)
        })
}

// `/reset` takes away a grant someone gave another account on the path itself. Inheritance
// goes only beside the grants: alone, it can leave an empty list, which shuts the owner out too.
pub(crate) fn icacls(folder: bool, path: &str, sid: &str) -> [String; 2] {
    let path = quoted(path);
    // A folder's grants pass on to what it holds; on a file they would grant nothing.
    let rights = if folder { "(OI)(CI)F" } else { "F" };
    let grant = |who: &str| format!("\"*{who}:{rights}\"");
    [
        format!("icacls {path} /reset"),
        format!(
            "icacls {path} /inheritance:r /grant:r {} {} {}",
            grant(SYSTEM),
            grant(ADMINISTRATORS),
            grant(sid)
        ),
    ]
}

// To the C runtime that splits icacls's arguments `\"` is a quote, so a trailing separator
// goes, and a drive's root keeps its own, doubled.
fn quoted(path: &str) -> String {
    let path = path.trim_end_matches(['\\', '/']);
    if path.is_empty() || path.ends_with(':') {
        format!("\"{path}\\\\\"")
    } else {
        format!("\"{path}\"")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const YOU: &str = "S-1-5-21-1004336348-1177238915-682003330-1001";
    const ANOTHER: &str = "S-1-5-21-1004336348-1177238915-682003330-1002";

    fn allow(sid: &str, mask: u32) -> Ace {
        Ace {
            allows: true,
            sid: sid.into(),
            mask,
            flags: 0,
        }
    }

    fn deny(sid: &str, mask: u32) -> Ace {
        Ace {
            allows: false,
            ..allow(sid, mask)
        }
    }

    fn private_to_you() -> Vec<Ace> {
        vec![
            allow(YOU, ALL),
            allow(SYSTEM, ALL),
            allow(ADMINISTRATORS, ALL),
        ]
    }

    // RF-C1: what a profile's `AppData\Local` and a folder made in it give.
    #[test]
    fn the_person_system_and_administrators_are_not_others() {
        let found = access(Some(&private_to_you()), Some(YOU));
        assert!(!found.shared);
        assert_eq!(found.described, "open to you, SYSTEM and Administrators");
        let mut profile = private_to_you();
        // RF-C1: the profile's own app container ACE, traverse and synchronize alone.
        profile.push(allow(
            "S-1-15-3-1024-1065365936-1281604716-3511738428",
            0x0010_0020,
        ));
        assert!(!access(Some(&profile), Some(YOU)).shared);
    }

    #[test]
    fn anybody_else_who_reads_or_changes_the_data_shares_it() {
        for (sid, name) in [
            (EVERYONE, "Everyone"),
            (USERS, "Users"),
            ("S-1-5-11", "Authenticated Users"),
            (ANOTHER, "another account"),
        ] {
            for mask in [
                0x0012_0089,
                0x0000_0002,
                0x8000_0000,
                0x1000_0000,
                0x0004_0000,
            ] {
                let mut aces = private_to_you();
                aces.push(allow(sid, mask));
                let found = access(Some(&aces), Some(YOU));
                assert!(found.shared, "{sid} {mask:#x}");
                assert_eq!(
                    found.described,
                    format!("open to {name}, you, SYSTEM and Administrators"),
                    "{sid} {mask:#x}"
                );
            }
        }
        let mut aces = private_to_you();
        aces.extend([allow(ANOTHER, ALL), allow("S-1-5-21-9-9-9-1003", ALL)]);
        assert_eq!(
            access(Some(&aces), Some(YOU)).described,
            "open to 2 other accounts, you, SYSTEM and Administrators"
        );
    }

    #[test]
    fn a_null_list_is_open_to_everyone_and_an_empty_one_to_nobody() {
        let open = access(None, Some(YOU));
        assert!(open.shared);
        assert_eq!(open.described, "open to everyone, with no access list");
        let closed = access(Some(&[]), Some(YOU));
        assert!(!closed.shared);
        assert_eq!(closed.described, "open to nobody");
    }

    #[test]
    fn what_reaches_no_data_or_only_what_is_below_shares_nothing() {
        let mut aces = private_to_you();
        aces.push(Ace {
            flags: OBJECT_INHERIT | CONTAINER_INHERIT | 0x08,
            ..allow(EVERYONE, ALL)
        });
        aces.push(allow(USERS, 0x0012_00A0));
        aces.push(allow(USERS, 0x2000_0000));
        assert!(!access(Some(&aces), Some(YOU)).shared);
    }

    #[test]
    fn a_deny_holds_for_its_own_account_alone() {
        let mut aces = vec![deny(USERS, 0x0012_0089)];
        aces.extend(private_to_you());
        aces.push(allow(USERS, 0x0012_0089));
        assert!(!access(Some(&aces), Some(YOU)).shared, "Users denied first");

        let mut aces = vec![deny(EVERYONE, 0x0012_0089)];
        aces.extend(private_to_you());
        aces.push(allow(USERS, 0x0012_0089));
        assert!(access(Some(&aces), Some(YOU)).shared, "Everyone denied");

        let mut aces = vec![deny(USERS, 0x0000_0002)];
        aces.push(allow(USERS, 0x0012_0089));
        assert!(
            access(Some(&aces), Some(YOU)).shared,
            "denied writing alone"
        );

        let mut aces = private_to_you();
        aces.extend([allow(USERS, 0x0012_0089), deny(USERS, 0x0012_0089)]);
        assert!(access(Some(&aces), Some(YOU)).shared, "denied after");
    }

    #[test]
    fn a_person_whose_sid_is_unread_is_another_account() {
        let found = access(Some(&private_to_you()), None);
        assert!(found.shared);
        assert_eq!(
            found.described,
            "open to another account, SYSTEM and Administrators"
        );
    }

    #[test]
    fn private_is_exactly_the_three_full_aces_protected() {
        let file = private_to_you();
        assert!(is_private(true, &file, YOU, false));
        assert!(!is_private(false, &file, YOU, false), "inherited");
        assert!(!is_private(true, &file, YOU, true), "a folder's pass on");
        assert!(!is_private(true, &file, ANOTHER, false), "someone else's");
        let mut more = file.clone();
        more.push(allow(USERS, 0x0012_0089));
        assert!(!is_private(true, &more, YOU, false));
        let mut less = file.clone();
        less[0].mask = 0x0012_0089;
        assert!(!is_private(true, &less, YOU, false));
        let folder: Vec<Ace> = file
            .into_iter()
            .rev()
            .map(|ace| Ace {
                flags: OBJECT_INHERIT | CONTAINER_INHERIT,
                ..ace
            })
            .collect();
        assert!(is_private(true, &folder, YOU, true));
    }

    #[test]
    fn pitboard_makes_its_own_files_and_folders_owned_by_and_private_to_the_person() {
        assert_eq!(
            private(YOU, false),
            format!("O:{YOU}D:P(A;;FA;;;{YOU})(A;;FA;;;SY)(A;;FA;;;BA)")
        );
        assert_eq!(
            private(YOU, true),
            format!("O:{YOU}D:P(A;OICI;FA;;;{YOU})(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)")
        );
    }

    #[test]
    fn the_icacls_lines_reset_the_path_then_take_inheritance_away_only_with_the_grants() {
        let path = r"C:\Users\Đạt Ng\.codex\auth.json";
        assert_eq!(
            icacls(false, path, YOU),
            [
                format!(r#"icacls "{path}" /reset"#),
                format!(
                    r#"icacls "{path}" /inheritance:r /grant:r "*S-1-5-18:F" "*S-1-5-32-544:F" "*{YOU}:F""#
                )
            ]
        );
        assert_eq!(
            icacls(true, r"C:\x y", YOU),
            [
                r#"icacls "C:\x y" /reset"#.to_owned(),
                format!(
                    r#"icacls "C:\x y" /inheritance:r /grant:r "*S-1-5-18:(OI)(CI)F" "*S-1-5-32-544:(OI)(CI)F" "*{YOU}:(OI)(CI)F""#
                )
            ]
        );
    }

    #[test]
    fn a_trailing_separator_never_escapes_the_closing_quote() {
        for (given, said) in [
            (r"C:\b c\", r#""C:\b c""#),
            (r"C:\b c\\", r#""C:\b c""#),
            ("C:/b c/", r#""C:/b c""#),
            (r"\\server\share\pb\", r#""\\server\share\pb""#),
            (r"C:\", r#""C:\\""#),
            ("C:", r#""C:\\""#),
        ] {
            let [reset, grant] = icacls(true, given, YOU);
            assert_eq!(reset, format!("icacls {said} /reset"), "{given}");
            assert!(
                grant.starts_with(&format!("icacls {said} /inheritance:r ")),
                "{given}"
            );
        }
    }
}
