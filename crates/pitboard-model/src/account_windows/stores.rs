//! Which store keeps the data of each account's window.

use uuid::Uuid;

// Neither this namespace nor a store name hashed in it may ever change: every window would
// lose its data, and the next sweep would delete it. Golden tests pin both.
const NAMESPACE: [u8; 16] = [
    0x67, 0x4b, 0x09, 0xf3, 0x8d, 0x37, 0x4e, 0x48, 0xa3, 0x61, 0x5a, 0xf2, 0xa6, 0x85, 0x67, 0x73,
];

// Each character of the id is lowered alone, as Swift's `lowercased()` did for every store
// released, not by Unicode's final-sigma rule.
pub(crate) fn derive(store_name: &str, account_id: &str) -> String {
    let lowered: String = account_id.chars().flat_map(char::to_lowercase).collect();
    let name = format!("{store_name}:{lowered}");
    Uuid::new_v5(&Uuid::from_bytes(NAMESPACE), name.as_bytes()).to_string()
}

/// Whether two store ids name one store. Compared without regard to case, since a store id
/// comes back from an app as it keeps it: Foundation writes a UUID in upper case.
pub(crate) fn same(one: &str, other: &str) -> bool {
    one.eq_ignore_ascii_case(other)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pitboard_sites::{CHATGPT, CLAUDE};

    fn claude(id: &str) -> String {
        derive(CLAUDE.store_name, id)
    }

    /// The values the Swift `storeID` gave before the rule was Rust, which every window
    /// released since keeps its data under: a store can never change once released, or the
    /// sweep deletes every window's data.
    #[test]
    fn an_enrolled_claude_account_has_a_store_derived_from_its_account_id() {
        assert_eq!(
            claude("4f3c2a10-8b7e-4d2a-9c1e-5a6b7c8d9e0f"),
            "7e15c34f-69ec-55b4-9542-f1c1fe3d7085"
        );
        assert_eq!(
            claude("dana@work.example"),
            "323d12fb-2c52-55b5-baec-df74fb60bc24",
            "the fixture's account id"
        );
        assert_eq!(
            claude("4F3C2A10-8B7E-4D2A-9C1E-5A6B7C8D9E0F"),
            claude("4f3c2a10-8b7e-4d2a-9c1e-5a6b7c8d9e0f")
        );
        let id = claude("anything");
        let digit = |at: usize| u8::from_str_radix(&id[at..=at], 16).expect("hexadecimal");
        assert_eq!(digit(14), 5, "version 5");
        assert_eq!(digit(19) >> 2, 0b10, "the RFC's variant");
        assert_ne!(id, "00000000-0000-0000-0000-000000000000");
        assert_eq!(id, id.to_lowercase());
        let namespace: String = NAMESPACE.iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(namespace, "674b09f38d374e48a3615af2a6856773");
    }

    /// The account id is lowered a character at a time, as Swift's `lowercased()` lowered it
    /// for every store released, and not by Unicode's rule for a final sigma: `ΟΔΟΣ` is
    /// hashed as `οδοσ`, not `οδος`. Each value is the one the Swift `storeID` gave, measured
    /// on macOS 27.0 on 5 October 2026.
    #[test]
    fn an_account_id_is_lowered_as_swift_lowered_it() {
        for (site, id, store) in [
            (&CLAUDE, "ΟΔΟΣ", "33f9c2ee-7d09-5937-8e90-10d8cca7c17d"),
            (&CHATGPT, "ΟΔΟΣ", "0e87083d-324c-5deb-805c-f7de3a12f080"),
            (&CLAUDE, "ΑΣ Β", "0d0cab1f-0547-5ea4-8024-b25b2d2c4db2"),
            (&CLAUDE, "aΣ", "3ca86f44-7d1c-5e59-b73a-da1de3baa641"),
            (&CLAUDE, "ΣΑ", "81cd7f41-59a9-539a-842c-025d38b933ad"),
            (&CLAUDE, "İstanbul", "85987f19-d83f-5118-b5eb-c90d93ea9aef"),
            (&CLAUDE, "ẞ", "916799a7-b9b0-50fd-9eda-e6af71bf2f41"),
            (&CLAUDE, "ǅ", "c6750585-c424-557d-9819-7cb04bf20692"),
            (&CLAUDE, "Ⅻ", "274dacb3-949e-57f1-896c-73e43128c5b5"),
        ] {
            assert_eq!(
                derive(site.store_name, id),
                store,
                "{}:{id}",
                site.store_name
            );
        }
    }

    /// A Codex account's window is on chatgpt.com, and its store is named apart from any
    /// Claude Code account's, even one with the same id.
    #[test]
    fn a_codex_accounts_store_is_its_own() {
        assert_eq!(
            derive(CHATGPT.store_name, "team_user-1"),
            "8ff9e0e6-a7e2-53e2-a594-6e53ddadd38a",
            "the name hashed is codex:<account id>, which never changes once shipped"
        );
        assert_ne!(derive(CHATGPT.store_name, "dana"), claude("dana"));
    }

    /// A store id an app kept in upper case, as Foundation writes a UUID, names the same
    /// store.
    #[test]
    fn a_store_is_the_same_whatever_case_it_is_written_in() {
        assert!(same(
            "7E15C34F-69EC-55B4-9542-F1C1FE3D7085",
            "7e15c34f-69ec-55b4-9542-f1c1fe3d7085"
        ));
        assert!(!same(
            "7e15c34f-69ec-55b4-9542-f1c1fe3d7085",
            "323d12fb-2c52-55b5-baec-df74fb60bc24"
        ));
    }
}
