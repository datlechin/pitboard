use super::BUNDLE_ID;
use super::paths::REAL;
use crate::context::Context;
use crate::host::Listed;
use std::path::Path;

const NATIVE_HOST: &str = "Contents/Helpers/chrome-native-host";
const MAIN: &str = "Contents/MacOS/Claude";
const HELPERS: &str = "Contents/Frameworks";

// A build for tests counts its scratch app alone, never a developer's own Claude.
pub(super) fn is_the_app(ctx: &Context, root: &Path, one: &Listed, all: &[Listed]) -> bool {
    let scratch = ctx.scratch_claude();
    let in_app = |path: &Path| {
        if REAL {
            in_claude(path, &|bundle| ctx.host().app_id(bundle))
        } else {
            scratch.is_some_and(|scratch| path.starts_with(&scratch.app))
        }
    };
    counts(&in_app, root, one, all)
}

// By its identifier, whatever the bundle's name: Finder's Keep Both makes `Claude 2.app`. A
// bundle whose identifier does not read counts by its name or Claude's layout, since counting
// more only refuses.
fn in_claude(path: &Path, id_of: &dyn Fn(&Path) -> Option<String>) -> bool {
    let outermost = path
        .ancestors()
        .skip(1)
        .filter(|dir| dir.extension().is_some_and(|extension| extension == "app"))
        .last();
    let Some(bundle) = outermost else {
        return false;
    };
    match id_of(bundle) {
        Some(id) => id == BUNDLE_ID,
        None => {
            path.ancestors()
                .skip(1)
                .any(|dir| dir.file_name().is_some_and(|name| name == "Claude.app"))
                || laid_out_as_claude(path, bundle)
        }
    }
}

// The main program and the helpers the register's `desktop_app` names, under `bundle`.
fn laid_out_as_claude(path: &Path, bundle: &Path) -> bool {
    let Ok(inside) = path.strip_prefix(bundle) else {
        return false;
    };
    inside == Path::new(MAIN)
        || inside
            .strip_prefix(HELPERS)
            .ok()
            .and_then(|rest| rest.components().next())
            .and_then(|helper| helper.as_os_str().to_str())
            .is_some_and(|helper| helper.starts_with("Claude Helper") && helper.ends_with(".app"))
}

// Chrome starts `chrome-native-host` for the browser extension: Claude's only where Claude did.
fn counts(in_app: &dyn Fn(&Path) -> bool, root: &Path, one: &Listed, all: &[Listed]) -> bool {
    if one.path.starts_with(root) {
        return true;
    }
    if !in_app(&one.path) {
        return false;
    }
    if one.path.ends_with(NATIVE_HOST) {
        return all
            .iter()
            .find(|parent| parent.pid == one.parent)
            .is_some_and(|parent| in_app(&parent.path));
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn listed(listing: &[(u32, u32, &str)]) -> Vec<Listed> {
        listing
            .iter()
            .map(|&(pid, parent, path)| Listed {
                pid,
                parent,
                path: PathBuf::from(path),
                package: None,
                placed: true,
            })
            .collect()
    }

    // The contributor's `ps` sample of 3 October 2026: the native host, started both ways.
    #[test]
    fn a_helper_counts_and_chrome_native_host_does_not() {
        let all = listed(&[
            (501, 1, "/Applications/Claude.app/Contents/MacOS/Claude"),
            (
                502,
                501,
                "/Applications/Claude.app/Contents/Frameworks/Claude Helper (Renderer).app/Contents/MacOS/Claude Helper (Renderer)",
            ),
            (
                503,
                501,
                "/Applications/Claude.app/Contents/Frameworks/Electron Framework.framework/Helpers/chrome_crashpad_handler",
            ),
            (
                590,
                1,
                "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
            ),
            (
                600,
                590,
                "/Applications/Claude.app/Contents/Helpers/chrome-native-host",
            ),
            (
                601,
                501,
                "/Applications/Claude.app/Contents/Helpers/chrome-native-host",
            ),
            (
                700,
                1,
                "/Users/a/Downloads/Claude.app/Contents/Frameworks/Squirrel.framework/Resources/ShipIt",
            ),
            (702, 1, "/usr/local/bin/claude"),
            (703, 1, "Claude"),
        ]);
        let root = Path::new("/Users/a/Library/Application Support/Claude");
        let id_of = |bundle: &Path| {
            let id = if bundle.ends_with("Claude.app") {
                BUNDLE_ID
            } else {
                "com.google.Chrome"
            };
            Some(id.to_string())
        };
        let in_app = |path: &Path| in_claude(path, &id_of);
        let found: Vec<u32> = all
            .iter()
            .filter(|one| counts(&in_app, root, one, &all))
            .map(|one| one.pid)
            .collect();
        assert_eq!(found, [501, 502, 503, 601, 700]);
    }

    // Finder's Keep Both names a second copy `Claude 2.app`.
    #[test]
    fn claude_is_told_by_its_bundle_identifier_whatever_its_bundle_is_named() {
        let id_of = |bundle: &Path| match bundle.to_str()? {
            "/Applications/Claude 2.app" => Some(BUNDLE_ID.to_string()),
            "/Applications/Google Chrome.app" => Some("com.google.Chrome".to_string()),
            "/Users/a/Apps/Claude.app" => Some("com.example.claude".to_string()),
            _ => None,
        };
        let all = listed(&[
            (501, 1, "/Applications/Claude 2.app/Contents/MacOS/Claude"),
            (
                502,
                501,
                "/Applications/Claude 2.app/Contents/Frameworks/Claude Helper (Renderer).app/Contents/MacOS/Claude Helper (Renderer)",
            ),
            (
                503,
                1,
                "/Applications/Claude 2.app/Contents/Frameworks/Squirrel.framework/Resources/ShipIt",
            ),
            (
                590,
                1,
                "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
            ),
            (
                600,
                590,
                "/Applications/Claude 2.app/Contents/Helpers/chrome-native-host",
            ),
            (
                601,
                501,
                "/Applications/Claude 2.app/Contents/Helpers/chrome-native-host",
            ),
            (700, 1, "/Users/a/Apps/Claude.app/Contents/MacOS/Claude"),
            (800, 1, "/Volumes/Gone/Claude.app/Contents/MacOS/Claude"),
            (801, 1, "/Volumes/Gone/Other.app/Contents/MacOS/Other"),
            (802, 1, "/Volumes/Gone/Claude 2.app/Contents/MacOS/Claude"),
            (
                803,
                802,
                "/Volumes/Gone/Claude 2.app/Contents/Frameworks/Claude Helper (Renderer).app/Contents/MacOS/Claude Helper (Renderer)",
            ),
            (
                805,
                1,
                "/Volumes/Gone/Other.app/Contents/MacOS/Claude Helper",
            ),
        ]);
        let root = Path::new("/Users/a/Library/Application Support/Claude");
        let in_app = |path: &Path| in_claude(path, &id_of);
        let found: Vec<u32> = all
            .iter()
            .filter(|one| counts(&in_app, root, one, &all))
            .map(|one| one.pid)
            .collect();
        assert_eq!(
            found,
            [501, 502, 503, 601, 800, 802, 803],
            "a bundle that does not read counts by its name or Claude's layout"
        );
    }

    #[test]
    fn a_program_inside_the_data_folder_counts() {
        let root = Path::new("/Users/a/Library/Application Support/Claude");
        let all = listed(&[(
            900,
            1,
            "/Users/a/Library/Application Support/Claude/claude-code/2.1.290/claude",
        )]);
        assert!(counts(
            &|path| in_claude(path, &|_| None),
            root,
            &all[0],
            &all
        ));
    }

    #[test]
    fn a_build_for_tests_counts_its_scratch_app_alone() {
        let folder = tempfile::tempdir().expect("a scratch folder");
        let scratch = folder.path().join("claude");
        let ctx = Context::for_unit_test().with_scratch_claude(scratch.clone());
        let root = scratch.join("data");
        let own = scratch.join("Claude.app/Contents/MacOS/Claude");
        let inside = root.join("claude-code/x/claude");
        let (own, inside) = (own.to_string_lossy(), inside.to_string_lossy());
        let all = listed(&[
            (1, 0, "/Applications/Claude.app/Contents/MacOS/Claude"),
            (2, 0, own.as_ref()),
            (3, 0, inside.as_ref()),
        ]);
        let found: Vec<u32> = all
            .iter()
            .filter(|one| is_the_app(&ctx, &root, one, &all))
            .map(|one| one.pid)
            .collect();
        assert_eq!(found, [2, 3]);
        assert!(!is_the_app(&Context::for_unit_test(), &root, &all[0], &all));
    }
}
