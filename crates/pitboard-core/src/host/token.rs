#[cfg(any(windows, test))]
use super::Elevation;

#[cfg(any(windows, test))]
pub(crate) const AS_ADMINISTRATOR: &str = "as administrator";

// Elevated and not split: User Account Control is off, or this is the built-in Administrator.
pub(crate) const IN_EVERY_PROGRAM: &str =
    "elevated, as every program this Windows account starts does";

#[cfg(any(windows, test))]
pub(crate) const AS_A_SERVICE_ACCOUNT: &str = "as a service account";

// TokenElevationTypeDefault, TokenElevationTypeFull and TokenElevationTypeLimited.
#[cfg(any(windows, test))]
const NOT_SPLIT: u32 = 1;
#[cfg(any(windows, test))]
const FULL: u32 = 2;
#[cfg(any(windows, test))]
const LIMITED: u32 = 3;

#[cfg(any(windows, test))]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Token {
    pub(crate) elevated: Option<bool>,
    pub(crate) elevation_type: Option<u32>,
    pub(crate) service_account: Option<bool>,
}

#[cfg(any(windows, test))]
pub(crate) fn elevation(token: Token) -> Elevation {
    let elevated = |why| Elevation::Elevated { why };
    if token.service_account == Some(true) {
        return elevated(AS_A_SERVICE_ACCOUNT);
    }
    match (token.elevated, token.elevation_type) {
        (Some(true), Some(NOT_SPLIT)) => return elevated(IN_EVERY_PROGRAM),
        (Some(true), _) | (_, Some(FULL)) => return elevated(AS_ADMINISTRATOR),
        _ => {}
    }
    match (token.service_account, token.elevated, token.elevation_type) {
        (Some(false), Some(false), Some(NOT_SPLIT | LIMITED)) => Elevation::Normal,
        _ => Elevation::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(elevated: bool, elevation_type: u32) -> Token {
        Token {
            elevated: Some(elevated),
            elevation_type: Some(elevation_type),
            service_account: Some(false),
        }
    }

    // CI run 37673429917, 8 October 2026, on windows-2025 and windows-11-arm.
    #[test]
    fn the_runners_tokens_read_as_measured() {
        let in_every_program = Elevation::Elevated {
            why: IN_EVERY_PROGRAM,
        };
        let job_user = read(true, NOT_SPLIT);
        let standard_user = read(false, NOT_SPLIT);
        let safer_from_the_job_user = read(true, NOT_SPLIT);
        assert_eq!(elevation(job_user), in_every_program);
        assert_eq!(elevation(standard_user), Elevation::Normal);
        assert_eq!(elevation(safer_from_the_job_user), in_every_program);
    }

    #[test]
    fn a_split_tokens_elevated_half_runs_as_administrator() {
        assert_eq!(elevation(read(false, LIMITED)), Elevation::Normal);
        assert_eq!(
            elevation(read(true, FULL)),
            Elevation::Elevated {
                why: AS_ADMINISTRATOR
            }
        );
    }

    // Pairs Windows does not document are taken at their most elevated.
    #[test]
    fn a_sign_of_elevation_wins() {
        let as_administrator = Elevation::Elevated {
            why: AS_ADMINISTRATOR,
        };
        assert_eq!(elevation(read(false, FULL)), as_administrator);
        assert_eq!(elevation(read(true, LIMITED)), as_administrator);
        assert_eq!(elevation(read(true, 7)), as_administrator);
        let flag_alone = Token {
            elevated: Some(true),
            ..Token::default()
        };
        assert_eq!(elevation(flag_alone), as_administrator);
        let type_alone = Token {
            elevation_type: Some(FULL),
            ..Token::default()
        };
        assert_eq!(elevation(type_alone), as_administrator);
    }

    #[test]
    fn a_service_account_is_elevated_whatever_else_it_says() {
        for token in [
            read(false, NOT_SPLIT),
            read(true, FULL),
            Token {
                service_account: Some(true),
                ..Token::default()
            },
        ] {
            let token = Token {
                service_account: Some(true),
                ..token
            };
            assert_eq!(
                elevation(token),
                Elevation::Elevated {
                    why: AS_A_SERVICE_ACCOUNT
                },
                "{token:?}"
            );
        }
    }

    #[test]
    fn a_token_not_read_in_full_is_unknown() {
        let unread = [
            Token::default(),
            Token {
                elevated: None,
                ..read(false, LIMITED)
            },
            Token {
                elevation_type: None,
                ..read(false, NOT_SPLIT)
            },
            Token {
                service_account: None,
                ..read(false, NOT_SPLIT)
            },
            read(false, 0),
            read(false, 7),
        ];
        for token in unread {
            assert_eq!(elevation(token), Elevation::Unknown, "{token:?}");
        }
    }
}
