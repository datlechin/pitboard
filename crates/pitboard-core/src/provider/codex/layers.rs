//! Which store Codex keeps its login in, read from every layer of configuration Codex reads
//! outside a project, in the order it reads them.
//!
//! Read from `codex-rs/config/src/loader` at tag `rust-v0.160.0` for macOS and Linux:
//! `load_config_layers_state`, `load_requirements_from_sources` and
//! `ConfigLayerSource::precedence` for the layers and their order, `apply_to_config` and the
//! `cli_auth_credentials_store_mode` it feeds for how a requirement pins the store, and
//! `loader/macos.rs` for the managed preferences. The register calls these
//! `codex_store_layers` and `codex_managed_preferences`.
//!
//! A setting in a higher layer replaces the same setting in a lower one, and a requirement
//! pins it over every layer. Codex starts only where every layer it reads is TOML it can
//! take, so a layer that is there and cannot be read is a store nobody can tell, which
//! [`Backend::Unknown`] refuses, and never the file.
//!
//! Three layers Codex has are not read:
//! - A project's own `.codex/config.toml`, which applies while Codex runs inside that
//!   project once it is trusted: a fact about one folder, not about this machine.
//!   Pitboard's own sign-ins run from a private folder and name the file store over it.
//! - A profile's `<name>.config.toml`, which a run of 0.160.0 reads only when it is given
//!   `--profile <name>`, as a layer of its own just over the person's `config.toml`
//!   (`ConfigLayerSource::User` with a profile, precedence 21 to the person's 20) and under
//!   a project's, `-c` and the managed ones. `codex login` and `codex logout` refuse
//!   `--profile`, so no such profile moves the login they write and remove.
//! - An enterprise's cloud configuration, which Codex fetches with the login it is about.
//!   `codex login` does not load it, so it moves no login a sign-in writes, and its
//!   requirements may not set the store (`LOCAL_ONLY_AUTH_REQUIREMENTS`). Its configuration
//!   may: nothing strips the store from it. A session of the TUI loads it, between
//!   `/etc/codex/config.toml` and the person's own. Whether the store a running session keeps
//!   its login in follows it was not read.
//!
//! A `profile = "<name>"` line chooses the profile a `[profiles.<name>]` table defines, as
//! the owner's answer of 7 October 2026 has Pitboard read it, in whichever layer each is:
//! the layers are merged before a profile is looked up. 0.160.0 calls the line a legacy way
//! to choose a profile and does not start with one (`load_config_with_layer_stack`); 0.99.0
//! has no such refusal and chooses the profile with it. The store is still the one the
//! layers choose: neither build reads one from a profile's table. Neither one's
//! `ConfigProfile` has `cli_auth_credentials_store`, so serde drops one there, and each
//! takes the store from the merged top level alone. 0.160.0 does not apply a profile's
//! `[features]` (`load_config_with_layer_stack` hands `Features::from_sources` an empty
//! source for the profile); 0.99.0 applies them, and none of its features is about the
//! store. A line naming a profile no table defines is a store nobody can tell,
//! as a layer Codex cannot read is: 0.160.0 refuses every such line, and 0.99.0 holds the
//! error "config profile \`<name>\` not found". A project's layer may set neither the line
//! nor a table (`PROJECT_LOCAL_CONFIG_DENYLIST`).

use super::paths::Backend;
use crate::context::Context;
use crate::host::{Administered, Os};
use std::path::{Path, PathBuf};

/// The domain of Codex's managed preferences, and their two keys.
const PREFERENCES: &str = "com.openai.codex";
const CONFIG_KEY: &str = "config_toml_base64";
const REQUIREMENTS_KEY: &str = "requirements_toml_base64";

/// The setting that chooses the store.
const STORE: &str = "cli_auth_credentials_store";

/// The feature that keeps a keychain store's login in an encrypted file of secrets.
const SECRETS: &str = "secret_auth_storage";

/// What Pitboard's own sign-ins give `codex -c`: the file store, over every layer a person
/// sets. It is TOML as it stands, which is how Codex reads a `-c` value.
pub(crate) const FILE_STORE: &str = "cli_auth_credentials_store=\"file\"";

/// What Codex is built with, its `codex-rs/config/defaults.toml`, as far as the store goes.
const PACKAGED: &str = "cli_auth_credentials_store = \"file\"\n";

/// One place Codex reads its configuration from, outside a project.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Layer {
    /// What Codex is built with: `cli_auth_credentials_store = "file"`.
    Packaged,
    /// `/etc/codex/config.toml`: this machine's defaults, which a person's own replace.
    System,
    /// `config.toml` in Codex's home, the person's own.
    Own,
    /// `-c` on Codex's command line, as Pitboard's own sign-ins give it.
    CommandLine,
    /// `/etc/codex/managed_config.toml`, over everything a person sets.
    Managed,
    /// The managed preference `config_toml_base64` of `com.openai.codex`, macOS's: base64
    /// TOML, over every file.
    ManagedPreference,
    /// `/etc/codex/requirements.toml`, which pins what it sets over every layer.
    Required,
    /// The managed preference `requirements_toml_base64` of `com.openai.codex`, macOS's,
    /// which pins over the file.
    RequiredByPreference,
}

impl Layer {
    /// Whether what this layer sets is a requirement, which no layer of configuration
    /// changes.
    fn pins(self) -> bool {
        matches!(self, Layer::Required | Layer::RequiredByPreference)
    }

    /// Whether Pitboard reads this layer on `os`: every layer but this machine's own files,
    /// which it reads only where it knows their folder ([`system_dir`]).
    fn read_on(self, os: Os) -> bool {
        match self {
            Layer::System | Layer::Managed | Layer::Required => system_dir(os).is_some(),
            Layer::Packaged
            | Layer::Own
            | Layer::CommandLine
            | Layer::ManagedPreference
            | Layer::RequiredByPreference => true,
        }
    }

    /// Where this layer is, as a person finds it, on `os`, with `own` the person's
    /// `config.toml`.
    fn place(self, os: Os, own: &Path) -> String {
        let file = |name: &str| match system_dir(os) {
            Some(dir) => dir.join(name).display().to_string(),
            None => format!("Codex's own {name} for this machine"),
        };
        match self {
            Layer::Packaged => "Codex's default".into(),
            Layer::System => file("config.toml"),
            Layer::Own => own.display().to_string(),
            Layer::CommandLine => "the `-c` Pitboard's sign-ins give `codex login`".into(),
            Layer::Managed => file("managed_config.toml"),
            Layer::ManagedPreference => {
                format!("the managed preference `{CONFIG_KEY}` of `{PREFERENCES}`")
            }
            Layer::Required => file("requirements.toml"),
            Layer::RequiredByPreference => {
                format!("the managed preference `{REQUIREMENTS_KEY}` of `{PREFERENCES}`")
            }
        }
    }
}

/// Where Codex keeps this machine's own configuration: `/etc/codex` on macOS and Linux.
/// On Windows it is a folder under `%ProgramData%`, a folder Windows names for each machine,
/// which W21 finds and reads (the register's pending `codex_store_layers`); until then none is
/// named, and no layer of it is read ([`Layer::read_on`]).
fn system_dir(os: Os) -> Option<&'static Path> {
    match os {
        Os::MacOs | Os::Linux => Some(Path::new("/etc/codex")),
        Os::Windows => None,
    }
}

/// The layers Codex 0.160.0 reads its configuration from on `os`, lowest first.
///
/// Between the system's and the person's own, Codex has an enterprise's cloud layers, and
/// over the person's, a profile's and a project's: none of the three is read here, as the
/// module says.
///
/// On Windows, Codex 0.99.0's loader reads the same layers but `managed_config.toml`, with
/// this machine's files in `%ProgramData%\OpenAI\Codex` (`load_config_layers_state` and
/// `windows_codex_system_dir` in `codex-rs/core/src/config_loader/mod.rs`). 0.160.0's are
/// the register's pending `codex_store_layers`, which W21 reads. Until then Pitboard reads
/// none of this machine's files there, so the store is one nobody can tell, whatever the
/// person's own `config.toml` says.
pub(crate) fn config_layers(os: Os) -> &'static [Layer] {
    use Layer::{CommandLine, Managed, ManagedPreference, Own, Packaged, System};
    match os {
        Os::MacOs => &[
            Packaged,
            System,
            Own,
            CommandLine,
            Managed,
            ManagedPreference,
        ],
        Os::Linux => &[Packaged, System, Own, CommandLine, Managed],
        Os::Windows => &[Packaged, System, Own, CommandLine],
    }
}

/// The layers Codex 0.160.0 reads requirements from on `os`, lowest first.
///
/// `/etc/codex/managed_config.toml` and its managed preference are requirements too, but only
/// for approvals and the sandbox (`legacy_requirements_to_toml_value`), so they pin no store.
/// On Windows the file of requirements is in this machine's folder Pitboard does not read
/// yet, as [`config_layers`] says.
pub(crate) fn requirement_layers(os: Os) -> &'static [Layer] {
    match os {
        Os::MacOs => &[Layer::Required, Layer::RequiredByPreference],
        Os::Linux | Os::Windows => &[Layer::Required],
    }
}

/// Whether Codex keeps a keychain store's login in its encrypted file of secrets when
/// nothing says: `secret_auth_storage` is on by default only where `cfg!(windows)`, so off
/// on macOS and Linux, and on on Windows.
fn secrets_by_default(os: Os) -> bool {
    match os {
        Os::MacOs | Os::Linux => false,
        Os::Windows => true,
    }
}

/// The store a store setting names, as Codex spells it in `AuthCredentialsStoreMode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    File,
    Keyring,
    Auto,
    Ephemeral,
}

impl Mode {
    fn named(word: &str) -> Option<Mode> {
        Some(match word {
            "file" => Mode::File,
            "keyring" => Mode::Keyring,
            "auto" => Mode::Auto,
            "ephemeral" => Mode::Ephemeral,
            _ => return None,
        })
    }

    fn word(self) -> &'static str {
        match self {
            Mode::File => "file",
            Mode::Keyring => "keyring",
            Mode::Auto => "auto",
            Mode::Ephemeral => "ephemeral",
        }
    }
}

/// Where Codex keeps its login on this machine, and which layer said so.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Store {
    pub(crate) backend: Backend,
    /// The layer whose setting chose the store: [`Layer::Packaged`] for Codex's own default.
    pub(crate) chosen_by: Layer,
    /// The store as that layer names it.
    mode: Mode,
    /// Where `secret_auth_storage` was turned on, for [`Backend::Secrets`], or `None` where it
    /// is on by default.
    secrets_by: Option<Layer>,
    /// For [`Backend::Unknown`], why nobody can tell.
    unknown: Option<Unknown>,
    os: Os,
    /// The person's own `config.toml`, where a line that changes the store goes.
    own: PathBuf,
}

/// Why nobody can tell the store, for [`Backend::Unknown`].
#[derive(Debug, Clone, PartialEq, Eq)]
struct Unknown {
    /// The layer that cannot be read as Codex reads it.
    layer: Layer,
    /// What is wrong with it, in words that follow the layer's place: nothing where Pitboard
    /// does not read it.
    why: String,
    /// What Codex does over it.
    then: Then,
}

/// What Codex does over the layer [`Unknown`] names.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Then {
    /// Codex 0.160.0 does not start.
    Stops,
    /// The profile a `profile = "<name>"` line chooses where no table defines it, which
    /// 0.160.0 refuses as it refuses every such line, and 0.99.0 as one it cannot find.
    NoProfile(String),
    /// Nobody here can tell: Pitboard does not read the layer on this system yet
    /// ([`Layer::read_on`]), which says nothing of Codex itself.
    Unread,
}

impl Store {
    /// The setting that chose this store, in words that can follow its name in brackets:
    /// Codex's default, `` `cli_auth_credentials_store = "keyring"` in `` a file, or `pinned to
    /// keyring by` a requirement.
    pub(crate) fn setting(&self) -> String {
        let place = self.place(self.chosen_by);
        let mut said = match self.chosen_by {
            Layer::Packaged => place,
            layer if layer.pins() => format!("pinned to `{}` by {place}", self.mode.word()),
            _ => format!("`{STORE} = \"{}\"` in {place}", self.mode.word()),
        };
        if self.backend == Backend::Secrets {
            match self.secrets_by {
                Some(layer) => {
                    said.push_str(&format!(", with `{SECRETS}` in {}", self.place(layer)))
                }
                None => said.push_str(&format!(", with `{SECRETS}` on by default")),
            }
        }
        said
    }

    /// What would put the login in the file store instead, as a sentence with no full stop,
    /// for a store that is not the file: the line to set and where, or who can.
    pub(crate) fn to_use_the_file(&self) -> String {
        let line = format!("`{STORE} = \"file\"`");
        let place = self.place(self.chosen_by);
        match self.chosen_by {
            Layer::Packaged | Layer::System | Layer::Own | Layer::CommandLine => format!(
                "To use the file store, set {line} in {}, then sign in again with `codex login`",
                self.own.display()
            ),
            Layer::Managed => format!(
                "{place} sets it over your own config.toml, so only an administrator can \
                 change it there to {line}"
            ),
            Layer::ManagedPreference => format!(
                "An administrator's configuration profile sets it in {place}, over every \
                 file, so only they can change it to {line}"
            ),
            Layer::Required | Layer::RequiredByPreference => format!(
                "No line in your own config.toml can change it: {place} pins it, and only \
                 an administrator can change that"
            ),
        }
    }

    /// Why the store cannot be told, for [`Backend::Unknown`]: the place that cannot be read
    /// as Codex reads it, and what is wrong there, or that Pitboard does not read Codex's
    /// configuration on this system yet.
    pub(crate) fn why_unknown(&self) -> Option<String> {
        self.unknown.as_ref().map(|unknown| match unknown.then {
            Then::Unread => {
                "Pitboard does not read Codex's configuration on this system yet".into()
            }
            Then::Stops | Then::NoProfile(_) => {
                format!("{} {}", self.place(unknown.layer), unknown.why)
            }
        })
    }

    /// What Codex does with what [`Store::why_unknown`] names, as sentences with no last full
    /// stop, for [`Backend::Unknown`]. Codex 0.160.0 does not start over any of it, and a
    /// `profile = "<name>"` line naming a profile no table defines is one 0.99.0, which
    /// chooses a profile with the line, refuses as not found. A place Pitboard does not read
    /// on this system yet says nothing of Codex.
    pub(crate) fn while_unknown(&self) -> Option<String> {
        self.unknown.as_ref().map(|unknown| match &unknown.then {
            Then::Stops => "Codex 0.160.0 does not start until that is put right".into(),
            Then::Unread => "This says nothing about Codex itself".into(),
            Then::NoProfile(name) => format!(
                "Codex 0.160.0 does not start with that line, and an older Codex, such as \
                 0.99.0, which chooses a profile with it, refuses one no table defines: \
                 \"config profile `{name}` not found\""
            ),
        })
    }

    fn place(&self, layer: Layer) -> String {
        layer.place(self.os, &self.own)
    }
}

/// The store this machine's Codex keeps its login in, read for the home `ctx` names.
pub(crate) fn of(ctx: &Context, own: &Path) -> Store {
    resolve(crate::host::OS, own, &|layer| {
        read(ctx, layer, Some(own), None)
    })
}

/// The store a sign-in Pitboard runs ends up with: in a private home, which holds no
/// `config.toml`, with [`FILE_STORE`] on its command line. `own` is the person's own
/// `config.toml`, for what the words name.
pub(crate) fn of_a_sign_in(ctx: &Context, own: &Path) -> Store {
    resolve(crate::host::OS, own, &|layer| {
        read(ctx, layer, None, Some(FILE_STORE))
    })
}

/// What `layer` holds on the machine `ctx` reaches, on this system: `own` is the
/// `config.toml` of Codex's home, where there is one, and `command_line` what `-c` gives.
fn read(
    ctx: &Context,
    layer: Layer,
    own: Option<&Path>,
    command_line: Option<&str>,
) -> Administered {
    let host = ctx.host();
    // A layer of this machine's own files is not asked for where there is no folder named
    // for them ([`Layer::read_on`]), and would be one nobody can read.
    let file = |name: &str| match system_dir(crate::host::OS) {
        Some(dir) => host.administered_file(&dir.join(name)),
        None => Administered::Unreadable("is not read on this system yet".into()),
    };
    match layer {
        Layer::Packaged => Administered::Set(PACKAGED.into()),
        Layer::System => file("config.toml"),
        // In a home, so read whatever the build: a test makes a home of its own.
        Layer::Own => own.map_or(Administered::Unset, crate::host::administered::read_text),
        Layer::CommandLine => command_line.map_or(Administered::Unset, |said| {
            Administered::Set(said.to_string())
        }),
        Layer::Managed => file("managed_config.toml"),
        Layer::ManagedPreference => decoded(host.managed_preference(PREFERENCES, CONFIG_KEY)),
        Layer::Required => file("requirements.toml"),
        Layer::RequiredByPreference => {
            decoded(host.managed_preference(PREFERENCES, REQUIREMENTS_KEY))
        }
    }
}

/// A managed preference's TOML out of the base64 it is forced as, trimmed first, as Codex
/// decodes it (`decode_managed_preferences_base64`).
fn decoded(forced: Administered) -> Administered {
    match forced {
        Administered::Set(encoded) => match standard_base64(encoded.trim()) {
            None => Administered::Unreadable("is not base64".into()),
            Some(bytes) => String::from_utf8(bytes).map_or_else(
                |_| Administered::Unreadable("does not decode to UTF-8 text".into()),
                Administered::Set,
            ),
        },
        other => other,
    }
}

/// What one layer says, once read.
enum Says {
    Nothing,
    Table(toml::Table),
    /// It is there and Codex cannot read it, which stops Codex from starting.
    Broken(String),
    /// Pitboard does not read it on this system yet, so whatever it says nobody here can tell.
    Unread,
}

impl Says {
    fn of(read: Administered) -> Says {
        match read {
            Administered::Unset => Says::Nothing,
            Administered::Unreadable(why) => Says::Broken(why),
            Administered::Set(text) => match text.parse::<toml::Table>() {
                Ok(table) => Says::Table(table),
                // The line only: the error's own words quote the file, which may hold a key.
                Err(e) => Says::Broken(match e.span() {
                    Some(at) => format!(
                        "is not TOML Codex can read, at line {}",
                        text.bytes().take(at.start).filter(|&b| b == b'\n').count() + 1
                    ),
                    None => "is not TOML Codex can read".into(),
                }),
            },
        }
    }

    fn table(&self) -> Option<&toml::Table> {
        match self {
            Says::Table(table) => Some(table),
            Says::Nothing | Says::Broken(_) | Says::Unread => None,
        }
    }
}

/// What a layer sets something to: nothing, a value Codex takes, or one it refuses, with
/// what is wrong with it in words that follow `sets`.
enum Set<T> {
    Unset,
    To(T),
    Refused(String),
}

/// The store a layer's table sets.
fn store_in(table: &toml::Table) -> Set<Mode> {
    match table.get(STORE) {
        None => Set::Unset,
        Some(toml::Value::String(word)) => {
            Mode::named(word).map_or_else(|| not_a_store(&format!("`\"{word}\"`")), Set::To)
        }
        Some(other) => not_a_store(&kind(other)),
    }
}

fn not_a_store<T>(written: &str) -> Set<T> {
    Set::Refused(format!(
        "{STORE} to {written}, which is not a store Codex has"
    ))
}

/// What kind of TOML value `value` is, as a person would say it: `an integer`.
fn kind(value: &toml::Value) -> String {
    let name = value.type_str();
    let article = if name.starts_with(['a', 'e', 'i', 'o', 'u']) {
        "an"
    } else {
        "a"
    };
    format!("{article} {name}")
}

/// The profile a layer's table chooses with `profile = "<name>"`. 0.160.0 calls the line
/// legacy and no longer supported: once the layers are merged, and none can take it away, it
/// refuses to start with one (`load_config_with_layer_stack`). 0.99.0 has no such refusal,
/// and chooses the profile with it, as Pitboard reads it. A `profile` that is not a name
/// stops 0.160.0 before that, which reads it as an `Option<String>`.
fn profile_in(table: &toml::Table) -> Set<String> {
    match table.get("profile") {
        None => Set::Unset,
        Some(toml::Value::String(name)) => Set::To(name.clone()),
        Some(other) => Set::Refused(format!(
            "`profile` to {}, where Codex takes a profile's name",
            kind(other)
        )),
    }
}

/// Whether a layer's table defines the profile `name`, as `[profiles.<name>]`. Codex reads
/// `profiles` as a `HashMap<String, ConfigProfile>`, so a `profiles`, or a profile in it,
/// that is not a table is a value it cannot take. What the profile's table holds is not
/// read: neither 0.99.0's `ConfigProfile` nor 0.160.0's has `cli_auth_credentials_store`,
/// 0.160.0 does not apply its `[features]`, and none of 0.99.0's features is about the
/// store.
fn profile_table_in(table: &toml::Table, name: &str) -> Set<()> {
    let not_a_table = |key: &str, value: &toml::Value| {
        Set::Refused(format!(
            "`{key}` to {}, where Codex takes a table",
            kind(value)
        ))
    };
    match table.get("profiles") {
        None => Set::Unset,
        Some(toml::Value::Table(profiles)) => match profiles.get(name) {
            None => Set::Unset,
            Some(toml::Value::Table(_)) => Set::To(()),
            Some(other) => not_a_table(&format!("profiles.{name}"), other),
        },
        Some(other) => not_a_table("profiles", other),
    }
}

/// Which of a layer's tables holds its features: `[features]`, or in a requirement also
/// `[feature_requirements]`, the alias `ConfigRequirementsToml` gives it. A configuration
/// layer's table of that name is none of Codex's, and sets nothing.
#[derive(Debug, Clone, Copy)]
enum Features {
    OfConfiguration,
    OfRequirement,
}

/// Whether a layer's table turns `secret_auth_storage` on or off.
fn secrets_in(table: &toml::Table, of: Features) -> Set<bool> {
    let features = table.get("features").or_else(|| match of {
        Features::OfRequirement => table.get("feature_requirements"),
        Features::OfConfiguration => None,
    });
    match features {
        None => Set::Unset,
        Some(toml::Value::Table(features)) => match features.get(SECRETS) {
            None => Set::Unset,
            Some(toml::Value::Boolean(on)) => Set::To(*on),
            Some(other) => Set::Refused(format!(
                "`{SECRETS}` to {}, where Codex takes only true or false",
                kind(other)
            )),
        },
        Some(other) => Set::Refused(format!(
            "`features` to {}, where Codex takes a table",
            kind(other)
        )),
    }
}

/// The highest of `layers`, given lowest first, that sets what `get` reads, with what it set.
fn highest<'a, T>(
    layers: impl DoubleEndedIterator<Item = &'a (Layer, Says)>,
    get: impl Fn(&toml::Table) -> Set<T>,
) -> Option<(Layer, Result<T, String>)> {
    layers
        .rev()
        .find_map(|(layer, says)| match get(says.table()?) {
            Set::Unset => None,
            Set::To(value) => Some((*layer, Ok(value))),
            Set::Refused(written) => Some((*layer, Err(written))),
        })
}

/// The store Codex 0.160.0 keeps its login in on `os`, from what `read` says each layer holds,
/// with `own` the person's `config.toml`, for the words.
pub(crate) fn resolve(os: Os, own: &Path, read: &dyn Fn(Layer) -> Administered) -> Store {
    let read_all = |layers: &[Layer]| -> Vec<(Layer, Says)> {
        layers
            .iter()
            .map(|&layer| {
                let says = if layer.read_on(os) {
                    Says::of(read(layer))
                } else {
                    Says::Unread
                };
                (layer, says)
            })
            .collect()
    };
    let (required, config) = (
        read_all(requirement_layers(os)),
        read_all(config_layers(os)),
    );
    let store = |backend, chosen_by, mode, secrets_by, unknown| Store {
        backend,
        chosen_by,
        mode,
        secrets_by,
        unknown,
        os,
        own: own.to_path_buf(),
    };
    let unknown = |layer: Layer, why: String, then: Then| {
        store(
            Backend::Unknown,
            layer,
            Mode::File,
            None,
            Some(Unknown { layer, why, then }),
        )
    };

    // A layer Codex cannot read stops it from starting, whatever the others say, and one
    // Pitboard does not read leaves nobody here able to tell, whatever the others say.
    if let Some((layer, why, then)) =
        required
            .iter()
            .rev()
            .chain(config.iter().rev())
            .find_map(|(layer, says)| match says {
                Says::Broken(why) => Some((*layer, why.clone(), Then::Stops)),
                Says::Unread => Some((*layer, String::new(), Then::Unread)),
                Says::Nothing | Says::Table(_) => None,
            })
    {
        return unknown(layer, why, then);
    }

    // Codex reads the configuration whole before a requirement replaces anything in it, so a
    // value it cannot take there stops it even under a requirement.
    let refused = |layer, wrong: String| unknown(layer, format!("sets {wrong}"), Then::Stops);
    let configured = match highest(config.iter(), store_in) {
        Some((layer, Ok(mode))) => (layer, mode),
        Some((layer, Err(wrong))) => return refused(layer, wrong),
        // Every list starts with what Codex is built with, which sets it.
        None => (Layer::Packaged, Mode::File),
    };
    let (chosen_by, mode) = match highest(required.iter(), store_in) {
        Some((layer, Ok(mode))) => (layer, mode),
        Some((layer, Err(wrong))) => return refused(layer, wrong),
        None => configured,
    };

    let configured_secrets = match highest(config.iter(), |table| {
        secrets_in(table, Features::OfConfiguration)
    }) {
        Some((layer, Ok(on))) => Some((layer, on)),
        Some((layer, Err(wrong))) => return refused(layer, wrong),
        None => None,
    };
    let (secrets_by, secrets) = match highest(required.iter(), |table| {
        secrets_in(table, Features::OfRequirement)
    }) {
        Some((layer, Ok(on))) => (Some(layer), on),
        Some((layer, Err(wrong))) => return refused(layer, wrong),
        None => configured_secrets.map_or((None, secrets_by_default(os)), |(layer, on)| {
            (Some(layer), on)
        }),
    };

    // A profile chosen in a file chooses the profile a table defines, wherever each is.
    // Its table holds no store 0.99.0 or 0.160.0 reads, so the store is still the layers',
    // but a profile no table defines is one nobody can tell: 0.160.0 refuses every such line
    // once it has read the configuration, and 0.99.0, which chooses a profile with it,
    // refuses one it cannot find.
    match highest(config.iter(), profile_in) {
        Some((layer, Ok(name))) => {
            match highest(config.iter(), |table| profile_table_in(table, &name)) {
                Some((_, Ok(()))) => {}
                Some((defined, Err(wrong))) => return refused(defined, wrong),
                None => {
                    return unknown(
                        layer,
                        format!(
                            "sets `profile = \"{name}\"`, a profile no `[profiles.{name}]` \
                             table defines"
                        ),
                        Then::NoProfile(name),
                    );
                }
            }
        }
        Some((layer, Err(wrong))) => return refused(layer, wrong),
        None => {}
    }

    let backend = match mode {
        Mode::File => Backend::File,
        Mode::Ephemeral => Backend::Ephemeral,
        Mode::Keyring | Mode::Auto if secrets => Backend::Secrets,
        Mode::Keyring => Backend::Keyring,
        Mode::Auto => Backend::Either,
    };
    let secrets_by = secrets_by.filter(|_| backend == Backend::Secrets);
    store(backend, chosen_by, mode, secrets_by, None)
}

/// Standard base64 with its padding, as Codex decodes a managed preference: the base64
/// crate's `BASE64_STANDARD`, which takes canonical padding and nothing else.
///
/// Written out rather than taken as a dependency, for the reason `jwt`'s base64url is.
fn standard_base64(text: &str) -> Option<Vec<u8>> {
    let bytes = text.as_bytes();
    if !bytes.len().is_multiple_of(4) {
        return None;
    }
    let padding = bytes.iter().rev().take_while(|&&b| b == b'=').count();
    if padding > 2 {
        return None;
    }
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    let (mut held, mut bits) = (0u32, 0);
    for &byte in &bytes[..bytes.len() - padding] {
        let sextet = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        };
        held = (held << 6) | u32::from(sextet);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(u8::try_from((held >> bits) & 0xff).expect("masked to one byte"));
        }
    }
    // What is left over is what the padding stands for, and must be zero to be canonical.
    ((held & ((1u32 << bits) - 1)) == 0).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    const OWN: &str = "/Users/someone/.codex/config.toml";

    // Joined as this host joins a path, which is how `Layer::place` names it.
    fn etc(name: &str) -> String {
        Path::new("/etc/codex").join(name).display().to_string()
    }

    /// What each layer holds, by layer, as a test says: unset where it says nothing. A managed
    /// preference is given as it is forced, in base64, and decoded as [`read`] decodes it.
    fn machine(os: Os, holds: &[(Layer, &str)]) -> Store {
        let holds: HashMap<Layer, Administered> = holds
            .iter()
            .map(|&(layer, text)| {
                let forced = Administered::Set(text.to_string());
                let holds = match layer {
                    Layer::ManagedPreference | Layer::RequiredByPreference => decoded(forced),
                    _ => forced,
                };
                (layer, holds)
            })
            .collect();
        resolve(os, Path::new(OWN), &|layer| match layer {
            Layer::Packaged => Administered::Set(PACKAGED.into()),
            _ => holds.get(&layer).cloned().unwrap_or(Administered::Unset),
        })
    }

    const KEYRING: &str = "cli_auth_credentials_store = \"keyring\"\n";
    const FILE: &str = "cli_auth_credentials_store = \"file\"\n";
    const EPHEMERAL: &str = "cli_auth_credentials_store = \"ephemeral\"\n";

    fn base64(text: &str) -> String {
        const ALPHABET: &[u8; 64] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::new();
        for chunk in text.as_bytes().chunks(3) {
            let n = chunk
                .iter()
                .enumerate()
                .fold(0u32, |n, (i, &b)| n | (u32::from(b) << (16 - 8 * i)));
            for i in 0..4 {
                if i <= chunk.len() {
                    out.push(char::from(ALPHABET[((n >> (18 - 6 * i)) & 63) as usize]));
                } else {
                    out.push('=');
                }
            }
        }
        out
    }

    /// Nothing set anywhere is Codex's own default, the file.
    #[test]
    fn nothing_set_is_codex_default_the_file() {
        for os in [Os::MacOs, Os::Linux] {
            let store = machine(os, &[]);
            assert_eq!(store.backend, Backend::File);
            assert_eq!(store.chosen_by, Layer::Packaged);
            assert_eq!(store.setting(), "Codex's default");
        }
    }

    /// Each layer replaces what the ones under it set, in Codex's order, on each system.
    #[test]
    fn a_higher_layer_replaces_a_lower_one() {
        for os in [Os::MacOs, Os::Linux] {
            let store = machine(os, &[(Layer::System, KEYRING)]);
            assert_eq!(
                (store.backend, store.chosen_by),
                (Backend::Keyring, Layer::System)
            );
            let store = machine(os, &[(Layer::System, KEYRING), (Layer::Own, FILE)]);
            assert_eq!(
                (store.backend, store.chosen_by),
                (Backend::File, Layer::Own),
                "the person's own config.toml is over the system's"
            );
            let store = machine(os, &[(Layer::Own, FILE), (Layer::Managed, KEYRING)]);
            assert_eq!(
                (store.backend, store.chosen_by),
                (Backend::Keyring, Layer::Managed),
                "managed_config.toml is over the person's"
            );
            let store = machine(
                os,
                &[(Layer::Own, KEYRING), (Layer::CommandLine, FILE_STORE)],
            );
            assert_eq!(
                (store.backend, store.chosen_by),
                (Backend::File, Layer::CommandLine),
                "a -c is over the person's config.toml"
            );
            let store = machine(
                os,
                &[(Layer::Managed, KEYRING), (Layer::CommandLine, FILE_STORE)],
            );
            assert_eq!(
                (store.backend, store.chosen_by),
                (Backend::Keyring, Layer::Managed),
                "and under managed_config.toml"
            );
        }
    }

    /// Only macOS has managed preferences. There, the forced config is over every file, and
    /// the forced requirements pin over the file of requirements. Linux reads neither.
    #[test]
    fn managed_preferences_are_read_on_macos_alone() {
        let forced = base64(KEYRING);
        let broken = format!("{}\n{}", &forced[..4], &forced[4..]);
        let holds = [
            (Layer::Managed, FILE),
            (Layer::ManagedPreference, forced.as_str()),
        ];
        let store = machine(Os::MacOs, &holds);
        assert_eq!(
            (store.backend, store.chosen_by),
            (Backend::Keyring, Layer::ManagedPreference)
        );
        assert_eq!(machine(Os::Linux, &holds).backend, Backend::File);

        let holds = [
            (Layer::Own, KEYRING),
            (Layer::Required, KEYRING),
            (Layer::RequiredByPreference, broken.as_str()),
        ];
        assert_eq!(
            machine(Os::MacOs, &holds).backend,
            Backend::Unknown,
            "a forced value that is not canonical base64 is one Codex cannot read"
        );
        let file = base64(FILE);
        let holds = [
            (Layer::Own, KEYRING),
            (Layer::Required, KEYRING),
            (Layer::RequiredByPreference, file.as_str()),
        ];
        let store = machine(Os::MacOs, &holds);
        assert_eq!(
            (store.backend, store.chosen_by),
            (Backend::File, Layer::RequiredByPreference)
        );
        let store = machine(Os::Linux, &holds);
        assert_eq!(
            (store.backend, store.chosen_by),
            (Backend::Keyring, Layer::Required)
        );
    }

    /// A requirement pins the store over every layer, even the forced managed config, and the
    /// words say no line of the person's own can change it.
    #[test]
    fn a_requirement_pins_the_store_over_every_layer() {
        let forced = base64(FILE);
        let store = machine(
            Os::MacOs,
            &[
                (Layer::Own, FILE),
                (Layer::CommandLine, FILE_STORE),
                (Layer::Managed, FILE),
                (Layer::ManagedPreference, forced.as_str()),
                (Layer::Required, EPHEMERAL),
            ],
        );
        assert_eq!(
            (store.backend, store.chosen_by),
            (Backend::Ephemeral, Layer::Required)
        );
        assert_eq!(
            store.setting(),
            format!("pinned to `ephemeral` by {}", etc("requirements.toml"))
        );
        assert_eq!(
            store.to_use_the_file(),
            format!(
                "No line in your own config.toml can change it: {} pins it, and only an \
                 administrator can change that",
                etc("requirements.toml")
            )
        );
    }

    /// The person's own setting, or the system's under it, is changed with one line in the
    /// person's own config.toml, as Codex resolves it.
    #[test]
    fn a_setting_of_the_persons_own_names_the_line_and_the_file() {
        let store = machine(
            Os::Linux,
            &[(Layer::System, "cli_auth_credentials_store = 'auto'")],
        );
        assert_eq!(store.backend, Backend::Either);
        assert_eq!(
            store.setting(),
            format!(
                "`cli_auth_credentials_store = \"auto\"` in {}",
                etc("config.toml")
            )
        );
        assert_eq!(
            store.to_use_the_file(),
            format!(
                "To use the file store, set `cli_auth_credentials_store = \"file\"` in {OWN}, \
                 then sign in again with `codex login`"
            )
        );
        let store = machine(Os::Linux, &[(Layer::Managed, KEYRING)]);
        assert_eq!(
            store.to_use_the_file(),
            format!(
                "{} sets it over your own config.toml, so only an administrator can change it \
                 there to `cli_auth_credentials_store = \"file\"`",
                etc("managed_config.toml")
            )
        );
    }

    /// `secret_auth_storage` makes a keychain store an encrypted file of secrets, read from
    /// the same layers in the same order and pinned the same way. It is off by default on
    /// macOS and Linux, and changes nothing for the file store.
    #[test]
    fn the_secret_store_is_read_from_the_same_layers() {
        let on = "[features]\nsecret_auth_storage = true\n";
        let off = "[features]\nsecret_auth_storage = false\n";
        for os in [Os::MacOs, Os::Linux] {
            assert_eq!(
                machine(os, &[(Layer::Own, KEYRING)]).backend,
                Backend::Keyring
            );
            let store = machine(os, &[(Layer::Own, KEYRING), (Layer::System, on)]);
            assert_eq!(store.backend, Backend::Secrets);
            assert_eq!(
                store.setting(),
                format!(
                    "`cli_auth_credentials_store = \"keyring\"` in {OWN}, with \
                     `secret_auth_storage` in {}",
                    etc("config.toml")
                )
            );
            assert_eq!(
                machine(
                    os,
                    &[
                        (Layer::Own, KEYRING),
                        (Layer::System, on),
                        (Layer::Managed, off)
                    ]
                )
                .backend,
                Backend::Keyring
            );
            assert_eq!(
                machine(
                    os,
                    &[
                        (Layer::Own, KEYRING),
                        (Layer::Managed, off),
                        (
                            Layer::Required,
                            "[feature_requirements]\nsecret_auth_storage = true\n"
                        )
                    ]
                )
                .backend,
                Backend::Secrets,
                "a requirement pins a feature too, under either name of its table"
            );
            assert_eq!(
                machine(
                    os,
                    &[
                        (Layer::Own, KEYRING),
                        (
                            Layer::System,
                            "[feature_requirements]\nsecret_auth_storage = true\n"
                        )
                    ]
                )
                .backend,
                Backend::Keyring,
                "only a requirement's table has that second name"
            );
            assert_eq!(
                machine(
                    os,
                    &[
                        (Layer::Own, KEYRING),
                        (Layer::Managed, "feature_requirements = 1\n")
                    ]
                )
                .backend,
                Backend::Keyring,
                "and a configuration layer's table of that name sets nothing"
            );
            assert_eq!(machine(os, &[(Layer::Own, on)]).backend, Backend::File);
        }
    }

    /// Where an administrator's layer chooses the file over a keychain or memory store of the
    /// person's own, Codex keeps its login in `auth.json`, and so the store is the file:
    /// `/etc/codex/managed_config.toml` replaces the person's setting, and a requirement
    /// replaces the configured one (`apply_exact_requirement`). Pitboard refused these,
    /// reading the person's `config.toml` alone. On Linux and on macOS, which also has the
    /// managed preferences.
    #[test]
    fn an_administrator_who_chooses_the_file_is_over_the_persons_own_store() {
        let forced = base64(FILE);
        for (own, alone) in [
            (KEYRING, Backend::Keyring),
            ("cli_auth_credentials_store = \"auto\"\n", Backend::Either),
            (EPHEMERAL, Backend::Ephemeral),
        ] {
            for os in [Os::Linux, Os::MacOs] {
                assert_eq!(machine(os, &[(Layer::Own, own)]).backend, alone);
                for layer in [Layer::Managed, Layer::Required] {
                    let store = machine(os, &[(Layer::Own, own), (layer, FILE)]);
                    assert_eq!(
                        (store.backend, store.chosen_by),
                        (Backend::File, layer),
                        "{os:?}: {layer:?} over {alone:?}"
                    );
                }
            }
            for layer in [Layer::ManagedPreference, Layer::RequiredByPreference] {
                let holds = [(Layer::Own, own), (layer, forced.as_str())];
                assert_eq!(
                    (
                        machine(Os::MacOs, &holds).backend,
                        machine(Os::Linux, &holds).backend
                    ),
                    (Backend::File, alone),
                    "{layer:?} over {alone:?}, which Linux does not read"
                );
            }
        }
        assert_eq!(
            machine(Os::Linux, &[(Layer::Own, KEYRING), (Layer::System, FILE)]).backend,
            Backend::Keyring,
            "/etc/codex/config.toml is under the person's own, so it lifts nothing"
        );
    }

    /// A layer that is there and that Codex cannot read stops Codex from starting, so the
    /// store is unknown, which refuses, and never the file.
    #[test]
    fn a_layer_codex_cannot_read_is_unknown_never_the_file() {
        for (layer, text, why) in [
            (
                Layer::System,
                "cli_auth_credentials_store = \n",
                "is not TOML Codex can read, at line 1",
            ),
            (
                Layer::Own,
                "model = \"o3\"\n[[[\n",
                "is not TOML Codex can read, at line 2",
            ),
            (
                Layer::Required,
                "]",
                "is not TOML Codex can read, at line 1",
            ),
            (
                Layer::Own,
                "cli_auth_credentials_store = \"nonsense\"\n",
                "sets cli_auth_credentials_store to `\"nonsense\"`, which is not a store Codex has",
            ),
            (
                Layer::Managed,
                "cli_auth_credentials_store = 1\n",
                "sets cli_auth_credentials_store to an integer, which is not a store Codex has",
            ),
            (
                Layer::System,
                "[features]\nsecret_auth_storage = \"yes\"\n",
                "sets `secret_auth_storage` to a string, where Codex takes only true or false",
            ),
        ] {
            let store = machine(Os::Linux, &[(Layer::Own, FILE), (layer, text)]);
            assert_eq!(store.backend, Backend::Unknown, "{text}");
            assert_eq!(
                store.why_unknown(),
                Some(format!("{} {why}", layer.place(Os::Linux, Path::new(OWN)))),
                "{text}"
            );
        }
        let unreadable = resolve(Os::Linux, Path::new(OWN), &|layer| match layer {
            Layer::Packaged => Administered::Set(PACKAGED.into()),
            Layer::Required => Administered::Unreadable("cannot be read: denied".into()),
            _ => Administered::Unset,
        });
        assert_eq!(unreadable.backend, Backend::Unknown);
        assert_eq!(
            unreadable.why_unknown(),
            Some(format!(
                "{} cannot be read: denied",
                etc("requirements.toml")
            ))
        );
    }

    /// A line that chooses the profile `work`, and the table that defines it.
    const PROFILED: &str = "profile = \"work\"\n[profiles.work]\nmodel = \"o3\"\n";

    /// A `profile = "<name>"` line in a layer chooses the profile `[profiles.<name>]`
    /// defines, as the owner's answer of 7 October 2026 reads it, wherever each is set: Codex
    /// merges the layers before it looks a profile up. A profile's table holds no store
    /// Codex 0.99.0 or 0.160.0 reads, so the store is still the one the layers choose, and
    /// where that is the file, the account is taken, where Pitboard refused every profile
    /// line.
    #[test]
    fn a_profile_line_chooses_its_profile_and_the_store_is_still_the_layers() {
        let chosen_over_a_file = format!("profile = \"work\"\n{FILE}");
        for os in [Os::MacOs, Os::Linux] {
            for layer in [Layer::System, Layer::Own, Layer::Managed] {
                let store = machine(os, &[(layer, PROFILED)]);
                assert_eq!(
                    (store.backend, store.chosen_by),
                    (Backend::File, Layer::Packaged),
                    "{os:?}: {layer:?}"
                );
                assert_eq!(store.setting(), "Codex's default");
                assert_eq!(store.why_unknown(), None);
            }
            let store = machine(
                os,
                &[
                    (Layer::System, "[profiles.work]\nmodel = \"o3\"\n"),
                    (Layer::Own, chosen_over_a_file.as_str()),
                ],
            );
            assert_eq!(
                (store.backend, store.chosen_by),
                (Backend::File, Layer::Own),
                "{os:?}: a profile one layer defines and another chooses"
            );
        }
    }

    /// What a chosen profile's table says of the store is not read, as neither Codex 0.99.0
    /// nor 0.160.0 reads one there: neither one's `ConfigProfile` has
    /// `cli_auth_credentials_store`, so serde drops the key, and 0.160.0 does not apply a
    /// profile's `[features]`, while 0.99.0 has no `secret_auth_storage` to apply. So a
    /// keychain store or the secret store there changes nothing, a store Codex does not have
    /// there stops nothing, a file store there lifts no keychain store of the person's own,
    /// which is refused naming the person's own line, and a table nothing chooses changes
    /// nothing either.
    #[test]
    fn a_chosen_profiles_table_holds_no_store_codex_reads() {
        for os in [Os::MacOs, Os::Linux] {
            let keychain_in_profile = "profile = \"work\"\n[profiles.work]\n\
                                       cli_auth_credentials_store = \"keyring\"\n\
                                       [profiles.work.features]\nsecret_auth_storage = true\n";
            let store = machine(os, &[(Layer::Own, keychain_in_profile)]);
            assert_eq!(
                (store.backend, store.chosen_by),
                (Backend::File, Layer::Packaged),
                "{os:?}"
            );
            let nonsense_in_profile = "profile = \"work\"\n[profiles.work]\n\
                                       cli_auth_credentials_store = \"nonsense\"\n";
            assert_eq!(
                machine(os, &[(Layer::Own, nonsense_in_profile)]).backend,
                Backend::File,
                "{os:?}: a key Codex drops stops nothing"
            );
            let file_in_profile = "cli_auth_credentials_store = \"keyring\"\nprofile = \"work\"\n\
                                   [profiles.work]\ncli_auth_credentials_store = \"file\"\n";
            let store = machine(os, &[(Layer::Own, file_in_profile)]);
            assert_eq!(
                (store.backend, store.chosen_by),
                (Backend::Keyring, Layer::Own),
                "{os:?}"
            );
            assert_eq!(
                store.setting(),
                format!("`cli_auth_credentials_store = \"keyring\"` in {OWN}")
            );
            assert_eq!(
                store.to_use_the_file(),
                format!(
                    "To use the file store, set `cli_auth_credentials_store = \"file\"` in \
                     {OWN}, then sign in again with `codex login`"
                )
            );
            let tables_alone = "[profiles.work]\ncli_auth_credentials_store = \"keyring\"\n";
            assert_eq!(
                machine(os, &[(Layer::Own, tables_alone)]).backend,
                Backend::File,
                "{os:?}: a profile's table that nothing chooses changes nothing"
            );
        }
    }

    /// With a profile chosen, a store the layers choose other than the file is refused as
    /// that store, as it is with no profile: the person's own keyring, a managed file's
    /// memory store, a requirement's pin.
    #[test]
    fn a_chosen_profile_leaves_a_store_that_is_not_the_file_to_be_refused_as_itself() {
        let keyring_chosen = format!("profile = \"work\"\n{KEYRING}[profiles.work]\n");
        let store = machine(Os::Linux, &[(Layer::Own, keyring_chosen.as_str())]);
        assert_eq!(
            (store.backend, store.chosen_by),
            (Backend::Keyring, Layer::Own)
        );
        let store = machine(
            Os::Linux,
            &[(Layer::Own, PROFILED), (Layer::Managed, EPHEMERAL)],
        );
        assert_eq!(
            (store.backend, store.chosen_by),
            (Backend::Ephemeral, Layer::Managed)
        );
        let store = machine(
            Os::MacOs,
            &[(Layer::Own, PROFILED), (Layer::Required, KEYRING)],
        );
        assert_eq!(
            (store.backend, store.chosen_by),
            (Backend::Keyring, Layer::Required)
        );
        assert_eq!(
            store.setting(),
            format!("pinned to `keyring` by {}", etc("requirements.toml"))
        );
    }

    /// A line that chooses a profile no layer defines is a store nobody can tell, which
    /// refuses, and never the file: Codex 0.160.0 does not start with the line at all, and an
    /// older Codex that chooses a profile with it, such as 0.99.0, refuses one it cannot
    /// find. A table of another name defines nothing, and the words say what each Codex does.
    #[test]
    fn a_profile_no_table_defines_is_unknown_never_the_file() {
        for (layer, holds) in [
            (Layer::Own, vec![(Layer::Own, "profile = \"work\"\n")]),
            (
                Layer::System,
                vec![
                    (
                        Layer::System,
                        "profile = \"work\"\n[profiles.home]\nmodel = \"o3\"\n",
                    ),
                    (Layer::Own, FILE),
                ],
            ),
            (
                Layer::Managed,
                vec![
                    (Layer::Managed, "profile = \"work\"\n"),
                    (Layer::Own, "[profiles.home]\nmodel = \"o3\"\n"),
                ],
            ),
        ] {
            let store = machine(Os::Linux, &holds);
            assert_eq!(store.backend, Backend::Unknown, "{layer:?}");
            assert_eq!(
                store.why_unknown(),
                Some(format!(
                    "{} sets `profile = \"work\"`, a profile no `[profiles.work]` table defines",
                    layer.place(Os::Linux, Path::new(OWN))
                )),
                "{layer:?}"
            );
            assert_eq!(
                store.while_unknown().as_deref(),
                Some(
                    "Codex 0.160.0 does not start with that line, and an older Codex, such as \
                     0.99.0, which chooses a profile with it, refuses one no table defines: \
                     \"config profile `work` not found\""
                ),
                "{layer:?}"
            );
        }
    }

    /// A profile line, or a table of profiles, that Codex cannot read stops 0.160.0 from
    /// starting, as a store it does not have does: a `profile` that is not a name, and a
    /// `profiles` or a `[profiles.<name>]` that is not a table, which its
    /// `HashMap<String, ConfigProfile>` cannot take. A store Codex does not have is told
    /// first, as 0.160.0 reads the configuration whole before it looks at the line.
    #[test]
    fn a_profile_codex_cannot_read_stops_it_as_any_value_it_cannot_take() {
        for (text, why) in [
            (
                "profile = 3\n",
                "sets `profile` to an integer, where Codex takes a profile's name",
            ),
            (
                "profile = \"work\"\nprofiles = 3\n",
                "sets `profiles` to an integer, where Codex takes a table",
            ),
            (
                "profile = \"work\"\n[profiles]\nwork = \"o3\"\n",
                "sets `profiles.work` to a string, where Codex takes a table",
            ),
            (
                "profile = \"work\"\ncli_auth_credentials_store = \"nonsense\"\n",
                "sets cli_auth_credentials_store to `\"nonsense\"`, which is not a store \
                 Codex has",
            ),
        ] {
            let store = machine(Os::Linux, &[(Layer::Own, text)]);
            assert_eq!(store.backend, Backend::Unknown, "{text}");
            assert_eq!(store.why_unknown(), Some(format!("{OWN} {why}")), "{text}");
            assert_eq!(
                store.while_unknown().as_deref(),
                Some("Codex 0.160.0 does not start until that is put right"),
                "{text}"
            );
        }
    }

    /// A value Codex cannot take is replaced by a higher layer's, as Codex merges them before
    /// it reads the store, so only the one that wins has to be one Codex has.
    #[test]
    fn a_value_codex_cannot_take_is_replaced_by_a_higher_one() {
        let store = machine(
            Os::Linux,
            &[
                (Layer::System, "cli_auth_credentials_store = \"nonsense\"\n"),
                (Layer::Own, FILE),
            ],
        );
        assert_eq!(store.backend, Backend::File);
    }

    /// A sign-in Pitboard runs has no config.toml of its own and the file store on its
    /// command line, so only what an administrator set over that keeps it from the file.
    #[test]
    #[cfg_attr(windows, ignore = "W21: switching Codex on Windows")]
    fn a_sign_in_takes_the_file_store_unless_an_administrator_set_another() {
        let host = crate::host::memory::MemoryHost::new();
        let ctx = Context::for_unit_test().with_memory_stores(host.clone());
        let own = Path::new(OWN);
        assert_eq!(of_a_sign_in(&ctx, own).backend, Backend::File);
        host.administers("/etc/codex/config.toml", KEYRING);
        let store = of_a_sign_in(&ctx, own);
        assert_eq!(
            (store.backend, store.chosen_by),
            (Backend::File, Layer::CommandLine),
            "the -c is over the system's config"
        );
        host.administers("/etc/codex/managed_config.toml", KEYRING);
        assert_eq!(of_a_sign_in(&ctx, own).backend, Backend::Keyring);
        host.administers("/etc/codex/managed_config.toml", FILE);
        host.administers("/etc/codex/requirements.toml", EPHEMERAL);
        let store = of_a_sign_in(&ctx, own);
        assert_eq!(
            (store.backend, store.chosen_by),
            (Backend::Ephemeral, Layer::Required)
        );
    }

    /// What the host says an administrator set is what is read, at Codex's paths, and a
    /// managed preference is read as the base64 Codex reads it as.
    #[test]
    #[cfg_attr(windows, ignore = "W21: the layers Codex reads on Windows")]
    fn what_an_administrator_set_is_read_through_the_host() {
        let host = crate::host::memory::MemoryHost::new();
        let ctx = Context::for_unit_test().with_memory_stores(host.clone());
        let own = Path::new(OWN);
        assert_eq!(of(&ctx, own).chosen_by, Layer::Packaged);
        host.administers("/etc/codex/requirements.toml", KEYRING);
        assert_eq!(of(&ctx, own).chosen_by, Layer::Required);
        host.administers_unreadably("/etc/codex/config.toml", "cannot be read: denied");
        assert_eq!(of(&ctx, own).backend, Backend::Unknown);

        let host = crate::host::memory::MemoryHost::new();
        let ctx = Context::for_unit_test().with_memory_stores(host.clone());
        host.forces(
            PREFERENCES,
            CONFIG_KEY,
            &format!("  {}\n", base64(EPHEMERAL)),
        );
        let store = of(&ctx, own);
        match crate::host::OS {
            Os::MacOs => assert_eq!(
                (store.backend, store.chosen_by),
                (Backend::Ephemeral, Layer::ManagedPreference)
            ),
            Os::Linux => assert_eq!(store.backend, Backend::File),
            // Windows has no managed preferences, and none of this machine's files is read
            // there until W21.
            Os::Windows => assert_eq!(store.backend, Backend::Unknown),
        }
    }

    /// On Windows Pitboard does not read this machine's own files of Codex's configuration
    /// yet, so the store is one nobody can tell, whatever the person's own `config.toml` or a
    /// `-c` says, and never Codex's default: the file is then no answer, and doctor fails it
    /// as a store nobody can tell rather than passing it as the file. It says nothing of
    /// Codex itself, which is not stopped by it as by a layer it cannot read.
    #[test]
    fn on_windows_the_store_is_one_nobody_can_tell_whatever_the_layers_say() {
        for holds in [
            &[][..],
            &[(Layer::Own, FILE)],
            &[(Layer::Own, KEYRING)],
            &[(Layer::Own, FILE), (Layer::CommandLine, FILE_STORE)],
            &[(Layer::System, KEYRING), (Layer::Required, FILE)],
        ] {
            let store = machine(Os::Windows, holds);
            assert_eq!(store.backend, Backend::Unknown, "{holds:?}");
            assert_eq!(
                store.why_unknown().as_deref(),
                Some("Pitboard does not read Codex's configuration on this system yet"),
                "{holds:?}"
            );
            assert_eq!(
                store.while_unknown().as_deref(),
                Some("This says nothing about Codex itself"),
                "{holds:?}"
            );
        }
        let host = crate::host::memory::MemoryHost::new();
        let ctx = Context::for_unit_test().with_memory_stores(host);
        let signing_in = resolve(Os::Windows, Path::new(OWN), &|layer| {
            read(&ctx, layer, None, Some(FILE_STORE))
        });
        assert_eq!(signing_in.backend, Backend::Unknown, "a sign-in's too");
    }

    /// Canonical standard base64 only, as the base64 crate's `BASE64_STANDARD` decodes it.
    #[test]
    fn base64_is_read_as_codex_reads_it() {
        for text in ["", "f", "fo", "foo", "foob", "fooba", "foobar"] {
            assert_eq!(
                standard_base64(&base64(text)),
                Some(text.as_bytes().to_vec()),
                "{text}"
            );
        }
        assert_eq!(standard_base64("+/+/"), Some(vec![0xfb, 0xff, 0xbf]));
        for refused in [
            "Zg", "Zg=", "Zm9v=", "Zh==", "Z===", "-_-_", "Zm 9v", "Zg==Zg==",
        ] {
            assert_eq!(standard_base64(refused), None, "{refused}");
        }
    }
}
