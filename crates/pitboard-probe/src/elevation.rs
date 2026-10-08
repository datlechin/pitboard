//! Naming a process token's elevation, so block A1 and the runner facts classify a token the
//! one way Pitboard's Windows face does, in `pitboard-core`'s `host/token.rs`.
//!
//! The naming is pure; the Windows reader hands it what it could read, each part `None` when
//! that read failed.

/// Windows's `TOKEN_ELEVATION_TYPE`: Default 1 (not split: a standard user, or UAC off),
/// Full 2 (an elevated token), Limited 3 (the filtered token of a split-token admin).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElevationType {
    Default,
    Full,
    Limited,
    Unknown(u32),
}

impl ElevationType {
    pub fn from_raw(ty: u32) -> Self {
        match ty {
            1 => ElevationType::Default,
            2 => ElevationType::Full,
            3 => ElevationType::Limited,
            other => ElevationType::Unknown(other),
        }
    }

    pub fn as_str(self) -> String {
        match self {
            ElevationType::Default => "default".into(),
            ElevationType::Full => "full".into(),
            ElevationType::Limited => "limited".into(),
            ElevationType::Unknown(n) => format!("unknown_{n}"),
        }
    }
}

/// A mandatory integrity level, by its RID.
pub fn integrity_word(rid: u32) -> &'static str {
    match rid {
        0x0000 => "untrusted",
        0x1000 => "low",
        0x2000 => "medium",
        0x2100 => "medium_plus",
        0x3000 => "high",
        0x4000 => "system",
        0x5000 => "protected_process",
        _ => "other",
    }
}

/// How Pitboard reads a token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reading {
    /// A change is refused. The reason names what made it elevated.
    Elevated(&'static str),
    /// A change may proceed.
    NotElevated,
    /// Part of the token could not be read, so a change is refused. The reason names it.
    Unknown(&'static str),
}

impl Reading {
    pub fn is_elevated(self) -> bool {
        matches!(self, Reading::Elevated(_))
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Reading::Elevated(_) => "elevated",
            Reading::NotElevated => "not_elevated",
            Reading::Unknown(_) => "unknown",
        }
    }

    pub fn reason(self) -> Option<&'static str> {
        match self {
            Reading::Elevated(why) | Reading::Unknown(why) => Some(why),
            Reading::NotElevated => None,
        }
    }
}

/// What the Windows reader could read of a token, each `None` when its read failed.
#[derive(Debug, Clone, Copy, Default)]
pub struct TokenFacts {
    pub elevation_type: Option<ElevationType>,
    /// The `TokenElevation` flag.
    pub is_elevated: Option<bool>,
    /// Whether the token's user is LocalSystem, LocalService or NetworkService.
    pub user_is_service: Option<bool>,
    /// The integrity level's RID.
    pub integrity_rid: Option<u32>,
}

/// Classify a token the way Pitboard does. Any sign of elevation wins, read or not the rest;
/// then a part that could not be read makes it unknown; only a token read in full with no
/// sign of elevation is not elevated.
pub fn reading(t: TokenFacts) -> Reading {
    if t.user_is_service == Some(true) {
        return Reading::Elevated("a service account");
    }
    if t.is_elevated == Some(true) {
        return Reading::Elevated("an elevated token");
    }
    if t.elevation_type == Some(ElevationType::Full) {
        return Reading::Elevated("a full token");
    }
    if t.user_is_service.is_none() {
        return Reading::Unknown("the token's user could not be read");
    }
    if t.is_elevated.is_none() {
        return Reading::Unknown("the token's elevation flag could not be read");
    }
    match t.elevation_type {
        None => Reading::Unknown("the token's elevation type could not be read"),
        Some(ElevationType::Unknown(_)) => {
            Reading::Unknown("an elevation type Windows does not document")
        }
        Some(ElevationType::Default | ElevationType::Limited) => Reading::NotElevated,
        Some(ElevationType::Full) => unreachable!("handled above"),
    }
}

/// Whether a SID string is a well-known service account.
pub fn is_service_sid(sid: &str) -> bool {
    matches!(sid, "S-1-5-18" | "S-1-5-19" | "S-1-5-20")
}

/// How a principal a block found relates to the token that found it, so a report names it
/// without its SID: the token's own user, a well-known principal, or anyone else.
pub fn relation(sid: &str, own: Option<&str>) -> &'static str {
    if own == Some(sid) {
        return "self";
    }
    match sid {
        "S-1-5-18" => "system",
        "S-1-5-19" => "local_service",
        "S-1-5-20" => "network_service",
        "S-1-5-32-544" => "administrators",
        "S-1-5-32-545" => "users",
        "S-1-5-11" => "authenticated_users",
        "S-1-1-0" => "everyone",
        "S-1-3-0" => "creator_owner",
        "S-1-3-4" => "owner_rights",
        "S-1-5-4" => "interactive",
        "S-1-15-2-1" => "all_application_packages",
        "S-1-15-2-2" => "all_restricted_application_packages",
        s if s.starts_with("S-1-5-80-956008885-3418522649-1831038044-1853292631-2271478464") => {
            "trusted_installer"
        }
        s if s.starts_with("S-1-5-21-") => "other_account",
        s if s.starts_with("S-1-15-") => "app_container",
        s if s.starts_with("S-1-5-80-") => "service_sid",
        _ => "other",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read_in_full(ty: u32, flag: bool) -> TokenFacts {
        TokenFacts {
            elevation_type: Some(ElevationType::from_raw(ty)),
            is_elevated: Some(flag),
            user_is_service: Some(false),
            integrity_rid: Some(0x2000),
        }
    }

    #[test]
    fn a_standard_user_is_not_elevated() {
        let r = reading(read_in_full(3, false));
        assert_eq!(r, Reading::NotElevated);
        assert!(!r.is_elevated());
        assert_eq!(r.as_str(), "not_elevated");
        assert_eq!(reading(read_in_full(1, false)), Reading::NotElevated);
    }

    #[test]
    fn a_full_or_flagged_token_is_elevated() {
        assert!(reading(read_in_full(2, false)).is_elevated());
        assert!(reading(read_in_full(3, true)).is_elevated());
    }

    #[test]
    fn the_integrity_level_decides_nothing() {
        let high = TokenFacts {
            integrity_rid: Some(0x3000),
            ..read_in_full(1, false)
        };
        assert_eq!(reading(high), Reading::NotElevated);
        let unread = TokenFacts {
            integrity_rid: None,
            ..read_in_full(1, false)
        };
        assert_eq!(reading(unread), Reading::NotElevated);
        assert!(reading(read_in_full(1, true)).is_elevated());
    }

    #[test]
    fn a_service_account_is_always_elevated() {
        let t = TokenFacts {
            user_is_service: Some(true),
            ..read_in_full(1, false)
        };
        assert_eq!(reading(t), Reading::Elevated("a service account"));
    }

    #[test]
    fn uac_off_reads_elevated_through_the_flag() {
        assert!(reading(read_in_full(1, true)).is_elevated());
    }

    #[test]
    fn a_part_that_could_not_be_read_is_unknown_not_elevated() {
        let no_type = TokenFacts {
            elevation_type: None,
            ..read_in_full(1, false)
        };
        assert_eq!(reading(no_type).as_str(), "unknown");
        let no_flag = TokenFacts {
            is_elevated: None,
            ..read_in_full(3, false)
        };
        assert_eq!(reading(no_flag).as_str(), "unknown");
        let no_user = TokenFacts {
            user_is_service: None,
            ..read_in_full(3, false)
        };
        assert_eq!(reading(no_user).as_str(), "unknown");
        assert_eq!(reading(read_in_full(7, false)).as_str(), "unknown");
        assert_eq!(reading(TokenFacts::default()).as_str(), "unknown");
    }

    #[test]
    fn a_sign_of_elevation_wins_over_an_unread_part() {
        let t = TokenFacts {
            is_elevated: Some(true),
            ..TokenFacts::default()
        };
        assert!(reading(t).is_elevated());
    }

    #[test]
    fn relations_name_no_sid() {
        assert_eq!(
            relation("S-1-5-21-1-2-3-1001", Some("S-1-5-21-1-2-3-1001")),
            "self"
        );
        assert_eq!(
            relation("S-1-5-21-1-2-3-1002", Some("S-1-5-21-1-2-3-1001")),
            "other_account"
        );
        assert_eq!(relation("S-1-5-18", None), "system");
        assert_eq!(relation("S-1-5-32-544", None), "administrators");
        assert_eq!(relation("S-1-9-9", None), "other");
    }

    #[test]
    fn the_words_are_stable() {
        assert_eq!(ElevationType::from_raw(2).as_str(), "full");
        assert_eq!(ElevationType::from_raw(7).as_str(), "unknown_7");
        assert_eq!(integrity_word(0x2000), "medium");
        assert_eq!(integrity_word(0x3000), "high");
        assert!(is_service_sid("S-1-5-18"));
        assert!(!is_service_sid("S-1-5-21-1-2-3-1001"));
    }
}
