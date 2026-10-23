//! Which store Codex keeps its login in, read from every layer of configuration Codex reads
//! outside a project, in the order it reads them.
//!
//! Read from `codex-rs/config/src/loader` at tag `rust-v0.160.0` for macOS and Linux:
//! `load_config_layers_state`, `load_requirements_from_sources` and
//! `ConfigLayerSource::precedence` for the layers and their order, `apply_to_config` and the
//! `cli_auth_credentials_store_mode` it feeds for how a requirement pins the store, and
//! `loader/macos.rs` for the managed preferences. The register calls these
//! `codex_store_layers` and `codex_managed_preferences`. Windows' are read from the same
//! loader at tag `rust-v0.163.0`, `codex_windows_store_layers`.
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
//!
//! Which ChatGPT logins Codex keeps is read from the same layers at tag `rust-v0.164.0`,
//! [`Restrictions`] (`codex_login_restrictions`).

use super::paths::Backend;
use crate::context::Context;
use crate::host::{Administered, Os};
use crate::words::listed_values;
use base64::prelude::{BASE64_STANDARD, Engine as _};
use std::fmt::Write as _;
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

    fn place(self, system: &Path, own: &Path) -> String {
        let file = |name: &str| system.join(name).display().to_string();
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

// Codex falls back to `C:\ProgramData` where Windows cannot say `FOLDERID_ProgramData`, and
// never reads the variable (`codex_windows_store_layers`).
fn system_dir(os: Os, program_data: Option<PathBuf>) -> PathBuf {
    match os {
        Os::MacOs | Os::Linux => PathBuf::from("/etc/codex"),
        Os::Windows => program_data
            .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"))
            .join("OpenAI")
            .join("Codex"),
    }
}

fn system_dir_of(ctx: &Context) -> PathBuf {
    system_dir(crate::host::OS, ctx.host().program_data())
}

// This machine's file `name` of Codex's configuration, as `ctx`'s host finds it.
#[cfg(test)]
pub(crate) fn system_file(ctx: &Context, name: &str) -> PathBuf {
    system_dir_of(ctx).join(name)
}

// Lowest first. An enterprise's cloud layers, a profile's and a project's are not read, as the
// module says. Windows reads no `managed_config.toml` and has no managed preferences.
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

impl Backend {
    // Codex has no one word for the last two.
    pub(crate) fn word(self) -> &'static str {
        match self {
            Backend::File => Mode::File.word(),
            Backend::Keyring => Mode::Keyring.word(),
            Backend::Either => Mode::Auto.word(),
            Backend::Ephemeral => Mode::Ephemeral.word(),
            Backend::Secrets => "secrets",
            Backend::Unknown => "unknown",
        }
    }
}

/// Why Pitboard follows Codex into no store but the file, as a sentence with no full stop.
/// `codex_keychain_stores_are_its_own` and `codex_windows_stores` in the register.
pub(crate) fn file_store_only(os: Os) -> &'static str {
    match os {
        Os::MacOs | Os::Linux => {
            "Pitboard handles Codex's file store, auth.json, and does not read an item Codex \
             created for itself, because every read would ask you for permission"
        }
        Os::Windows => {
            "Pitboard handles Codex's file store, auth.json, and does not follow Codex into a \
             store it uses only when told to"
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
    // This machine's folder of Codex's configuration.
    system: PathBuf,
    /// The person's own `config.toml`, where a line that changes the store goes.
    own: PathBuf,
}

/// Why nobody can tell the store, for [`Backend::Unknown`].
#[derive(Debug, Clone, PartialEq, Eq)]
struct Unknown {
    /// The layer that cannot be read as Codex reads it.
    layer: Layer,
    /// What is wrong with it, in words that follow the layer's place.
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
                    let _ = write!(said, ", with `{SECRETS}` in {}", self.place(layer));
                }
                None => {
                    let _ = write!(said, ", with `{SECRETS}` on by default");
                }
            }
        }
        said
    }

    /// What would put the login in the file store instead, as a sentence with no full stop,
    /// for a store that is not the file: the line to set and where, or who can.
    pub(crate) fn to_use_the_file(&self, program: &str) -> String {
        let line = format!("`{STORE} = \"file\"`");
        let place = self.place(self.chosen_by);
        match self.chosen_by {
            Layer::Packaged | Layer::System | Layer::Own | Layer::CommandLine => format!(
                "To use the file store, set {line} in {}, then sign in again with `{program} \
                 login`",
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
    /// as Codex reads it, and what is wrong there.
    pub(crate) fn why_unknown(&self) -> Option<String> {
        self.unknown
            .as_ref()
            .map(|unknown| format!("{} {}", self.place(unknown.layer), unknown.why))
    }

    /// What Codex does with what [`Store::why_unknown`] names, as sentences with no last full
    /// stop, for [`Backend::Unknown`]. Codex 0.160.0 does not start over any of it, and a
    /// `profile = "<name>"` line naming a profile no table defines is one 0.99.0, which
    /// chooses a profile with the line, refuses as not found.
    pub(crate) fn while_unknown(&self) -> Option<String> {
        self.unknown.as_ref().map(|unknown| match &unknown.then {
            Then::Stops => "Codex 0.160.0 does not start until that is put right".into(),
            Then::NoProfile(name) => format!(
                "Codex 0.160.0 does not start with that line, and an older Codex, such as \
                 0.99.0, which chooses a profile with it, refuses one no table defines: \
                 \"config profile `{name}` not found\""
            ),
        })
    }

    fn place(&self, layer: Layer) -> String {
        layer.place(&self.system, &self.own)
    }
}

/// The store this machine's Codex keeps its login in, read for the home `ctx` names.
pub(crate) fn of(ctx: &Context, own: &Path) -> Store {
    let system = system_dir_of(ctx);
    resolve(crate::host::OS, &system, own, &|layer| {
        read(ctx, &system, layer, Some(own), None)
    })
}

/// The store a sign-in Pitboard runs ends up with: in a private home, which holds no
/// `config.toml`, with [`FILE_STORE`] on its command line. `own` is the person's own
/// `config.toml`, for what the words name.
pub(crate) fn of_a_sign_in(ctx: &Context, own: &Path) -> Store {
    let system = system_dir_of(ctx);
    resolve(crate::host::OS, &system, own, &|layer| {
        read(ctx, &system, layer, None, Some(FILE_STORE))
    })
}

// `own` is the `config.toml` of Codex's home, where there is one, and `command_line` what `-c`
// gives.
fn read(
    ctx: &Context,
    system: &Path,
    layer: Layer,
    own: Option<&Path>,
    command_line: Option<&str>,
) -> Administered {
    let host = ctx.host();
    let file = |name: &str| host.administered_file(&system.join(name));
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

// Trimmed first, as Codex trims a forced value before decode_managed_preferences_base64.
fn decoded(forced: Administered) -> Administered {
    match forced {
        Administered::Set(encoded) => match BASE64_STANDARD.decode(encoded.trim()) {
            Err(_) => Administered::Unreadable("is not base64".into()),
            Ok(bytes) => String::from_utf8(bytes).map_or_else(
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
            Says::Nothing | Says::Broken(_) => None,
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

type Read = Vec<(Layer, Says)>;

// The requirements, then the configuration, each lowest first.
fn read_layers(os: Os, read: &dyn Fn(Layer) -> Administered) -> (Read, Read) {
    let read_all = |layers: &[Layer]| -> Read {
        layers
            .iter()
            .map(|&layer| (layer, Says::of(read(layer))))
            .collect()
    };
    (
        read_all(requirement_layers(os)),
        read_all(config_layers(os)),
    )
}

// The highest layer Codex cannot read, which stops it from starting.
fn broken(required: &Read, config: &Read) -> Option<(Layer, String)> {
    required
        .iter()
        .rev()
        .chain(config.iter().rev())
        .find_map(|(layer, says)| match says {
            Says::Broken(why) => Some((*layer, why.clone())),
            Says::Nothing | Says::Table(_) => None,
        })
}

// `system` and `own`, the person's `config.toml`, are for the words.
pub(crate) fn resolve(
    os: Os,
    system: &Path,
    own: &Path,
    read: &dyn Fn(Layer) -> Administered,
) -> Store {
    let (required, config) = read_layers(os, read);
    let store = |backend, chosen_by, mode, secrets_by, unknown| Store {
        backend,
        chosen_by,
        mode,
        secrets_by,
        unknown,
        system: system.to_path_buf(),
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

    // A layer Codex cannot read stops it from starting, whatever the others say.
    if let Some((layer, why)) = broken(&required, &config) {
        return unknown(layer, why, Then::Stops);
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

const FORCED_METHOD: &str = "forced_login_method";
const FORCED_WORKSPACES: &str = "forced_chatgpt_workspace_id";
const ALLOWED_METHODS: &str = "allowed_login_methods";
const ALLOWED_WORKSPACES: &str = "allowed_chatgpt_workspaces";

// As `ForcedLoginMethod` spells a way to sign in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Method {
    Chatgpt,
    Api,
}

impl Method {
    fn named(word: &str) -> Option<Method> {
        match word {
            "chatgpt" => Some(Method::Chatgpt),
            "api" => Some(Method::Api),
            _ => None,
        }
    }

    fn word(self) -> &'static str {
        match self {
            Method::Chatgpt => "chatgpt",
            Method::Api => "api",
        }
    }
}

// Which ChatGPT logins this machine's Codex keeps (`codex_login_restrictions`). What the
// configuration forces, the interactive `codex` signs out of and deletes as it starts; what
// either leaves out, every Codex leaves unused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Restrictions {
    method: Option<(Layer, Method)>,
    workspaces: Option<(Layer, Vec<String>)>,
    allowed_methods: Option<(Layer, Vec<Method>)>,
    allowed_workspaces: Option<(Layer, Vec<String>)>,
    // What Codex cannot take, which stops it from starting.
    unreadable: Option<(Layer, String)>,
    system: PathBuf,
    own: PathBuf,
}

impl Restrictions {
    // Why Codex would not keep a ChatGPT login of the workspace its ID token names, `claimed`,
    // and its `tokens.account_id` names, `stored`, as a sentence with no full stop.
    pub(crate) fn refuses(&self, claimed: Option<&str>, stored: Option<&str>) -> Option<String> {
        if let Some((layer, why)) = &self.unreadable {
            return Some(format!(
                "{} {why}, so Codex does not start, and which logins it keeps cannot be told",
                self.place(*layer)
            ));
        }
        let signs_out = "so the interactive `codex` signs out of it as it starts, and deletes it";
        let unused = "so Codex does not use it";
        let of = |ids: &[String], workspace: Option<&str>| {
            (!workspace.is_some_and(|id| ids.iter().any(|allowed| allowed == id))).then(|| {
                format!(
                    "{}, and this login is {}",
                    listed_values(ids),
                    workspace
                        .map_or_else(|| "of no workspace".to_owned(), |id| format!("of `{id}`"))
                )
            })
        };
        if let Some((layer, Method::Api)) = self.method {
            return Some(format!(
                "{} sets `{FORCED_METHOD} = \"api\"`, {signs_out}",
                self.place(layer)
            ));
        }
        if let Some((layer, ids)) = &self.workspaces
            && let Some(outside) = of(ids, claimed)
        {
            return Some(format!(
                "{} sets `{FORCED_WORKSPACES}` to {outside}, {signs_out}",
                self.place(*layer)
            ));
        }
        if let Some((layer, methods)) = &self.allowed_methods
            && !methods.contains(&Method::Chatgpt)
        {
            return Some(format!(
                "{} sets `{ALLOWED_METHODS}` to {}, which leaves out a ChatGPT sign-in, {unused}",
                self.place(*layer),
                listed_values(&methods.iter().map(|m| m.word()).collect::<Vec<_>>())
            ));
        }
        // Its AuthManager holds `tokens.account_id`, else the claim, to what either names.
        for (key, set) in [
            (FORCED_WORKSPACES, &self.workspaces),
            (ALLOWED_WORKSPACES, &self.allowed_workspaces),
        ] {
            if let Some((layer, ids)) = set
                && let Some(outside) = of(ids, stored.or(claimed))
            {
                return Some(format!(
                    "{} sets `{key}` to {outside}, {unused}",
                    self.place(*layer)
                ));
            }
        }
        None
    }

    // Each restriction and where it is set, for `doctor`.
    pub(crate) fn said(&self) -> Vec<String> {
        let set = |layer: Layer, key: &str, to: String| {
            format!("`{key}` set to {to} in {}", self.place(layer))
        };
        let methods = |methods: &[Method]| {
            listed_values(&methods.iter().map(|m| m.word()).collect::<Vec<_>>())
        };
        if let Some((layer, why)) = &self.unreadable {
            return vec![format!("{} {why}", self.place(*layer))];
        }
        self.method
            .map(|(layer, method)| set(layer, FORCED_METHOD, listed_values(&[method.word()])))
            .into_iter()
            .chain(
                self.workspaces
                    .as_ref()
                    .map(|(layer, ids)| set(*layer, FORCED_WORKSPACES, listed_values(ids))),
            )
            .chain(
                self.allowed_methods
                    .as_ref()
                    .map(|(layer, allowed)| set(*layer, ALLOWED_METHODS, methods(allowed))),
            )
            .chain(
                self.allowed_workspaces
                    .as_ref()
                    .map(|(layer, ids)| set(*layer, ALLOWED_WORKSPACES, listed_values(ids))),
            )
            .collect()
    }

    pub(crate) fn unreadable(&self) -> bool {
        self.unreadable.is_some()
    }

    fn place(&self, layer: Layer) -> String {
        layer.place(&self.system, &self.own)
    }
}

pub(crate) fn restrictions_of(ctx: &Context, own: &Path) -> Restrictions {
    let system = system_dir_of(ctx);
    restrictions(crate::host::OS, &system, own, &|layer| {
        read(ctx, &system, layer, Some(own), None)
    })
}

fn restrictions(
    os: Os,
    system: &Path,
    own: &Path,
    read: &dyn Fn(Layer) -> Administered,
) -> Restrictions {
    let (required, config) = read_layers(os, read);
    let mut found = Restrictions {
        method: None,
        workspaces: None,
        allowed_methods: None,
        allowed_workspaces: None,
        unreadable: broken(&required, &config),
        system: system.to_path_buf(),
        own: own.to_path_buf(),
    };
    if found.unreadable.is_some() {
        return found;
    }
    let mut wrong = None;
    found.method = taken(&config, method_in, &mut wrong);
    // Codex reads an empty list as no restriction here, and as none allowed in a requirement.
    found.workspaces = taken(
        &config,
        |table| workspaces_in(table, FORCED_WORKSPACES),
        &mut wrong,
    )
    .filter(|(_, ids)| !ids.is_empty());
    found.allowed_methods = taken(&required, methods_in, &mut wrong);
    found.allowed_workspaces = taken(
        &required,
        |table| workspaces_in(table, ALLOWED_WORKSPACES),
        &mut wrong,
    );
    found.unreadable = wrong;
    found
}

// What the highest of `layers` to set it sets, where Codex takes it. One it cannot take goes in
// `wrong`, the first such.
fn taken<T>(
    layers: &Read,
    get: impl Fn(&toml::Table) -> Set<T>,
    wrong: &mut Option<(Layer, String)>,
) -> Option<(Layer, T)> {
    match highest(layers.iter(), get)? {
        (layer, Ok(value)) => Some((layer, value)),
        (layer, Err(said)) => {
            wrong.get_or_insert((layer, format!("sets {said}")));
            None
        }
    }
}

fn method_in(table: &toml::Table) -> Set<Method> {
    match table.get(FORCED_METHOD) {
        None => Set::Unset,
        Some(toml::Value::String(word)) => Method::named(word).map_or_else(
            || {
                Set::Refused(format!(
                    "`{FORCED_METHOD}` to `\"{word}\"`, which is not a way Codex signs in"
                ))
            },
            Set::To,
        ),
        Some(other) => Set::Refused(format!(
            "`{FORCED_METHOD}` to {}, where Codex takes `\"chatgpt\"` or `\"api\"`",
            kind(other)
        )),
    }
}

fn methods_in(table: &toml::Table) -> Set<Vec<Method>> {
    match table.get(ALLOWED_METHODS) {
        None => Set::Unset,
        Some(toml::Value::Array(items)) => items
            .iter()
            .map(|item| item.as_str().and_then(Method::named))
            .collect::<Option<Vec<_>>>()
            .map_or_else(
                || {
                    Set::Refused(format!(
                        "`{ALLOWED_METHODS}` to a list with something in it other than \
                         `\"chatgpt\"` and `\"api\"`"
                    ))
                },
                Set::To,
            ),
        Some(other) => Set::Refused(format!(
            "`{ALLOWED_METHODS}` to {}, where Codex takes a list",
            kind(other)
        )),
    }
}

// Trimmed, the empty ones dropped, as Codex reads both settings. Only `forced_chatgpt_workspace_id`
// takes one workspace on its own, and never several in one string.
fn workspaces_in(table: &toml::Table, key: &str) -> Set<Vec<String>> {
    let trimmed = |ids: Vec<&str>| {
        Set::To(
            ids.into_iter()
                .map(str::trim)
                .filter(|id| !id.is_empty())
                .map(str::to_owned)
                .collect(),
        )
    };
    match table.get(key) {
        None => Set::Unset,
        Some(toml::Value::String(id)) if key == FORCED_WORKSPACES && !id.contains(',') => {
            trimmed(vec![id.as_str()])
        }
        Some(toml::Value::Array(items)) => items
            .iter()
            .map(toml::Value::as_str)
            .collect::<Option<Vec<_>>>()
            .map_or_else(
                || {
                    Set::Refused(format!(
                        "`{key}` to a list with something in it other than workspace ids"
                    ))
                },
                trimmed,
            ),
        Some(toml::Value::String(_)) if key == FORCED_WORKSPACES => Set::Refused(format!(
            "`{key}` to workspace ids apart by commas in one string, where Codex takes a list"
        )),
        Some(other) => Set::Refused(format!(
            "`{key}` to {}, where Codex takes a list of workspace ids",
            kind(other)
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    const OWN: &str = "/Users/someone/.codex/config.toml";

    fn etc_codex() -> PathBuf {
        system_dir(Os::Linux, None)
    }

    // Joined as this host joins a path, which is how `Layer::place` names it.
    fn etc(name: &str) -> String {
        etc_codex().join(name).display().to_string()
    }

    fn program_data(name: &str) -> String {
        system_dir(Os::Windows, None)
            .join(name)
            .display()
            .to_string()
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
        resolve(
            os,
            &system_dir(os, None),
            Path::new(OWN),
            &|layer| match layer {
                Layer::Packaged => Administered::Set(PACKAGED.into()),
                _ => holds.get(&layer).cloned().unwrap_or(Administered::Unset),
            },
        )
    }

    const KEYRING: &str = "cli_auth_credentials_store = \"keyring\"\n";
    const FILE: &str = "cli_auth_credentials_store = \"file\"\n";
    const EPHEMERAL: &str = "cli_auth_credentials_store = \"ephemeral\"\n";

    fn base64(text: &str) -> String {
        BASE64_STANDARD.encode(text)
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

    /// A forced value is read only where Codex 0.164.0 reads it: trimmed, then strict
    /// `BASE64_STANDARD` (codex-rs/config/src/loader/macos.rs:294).
    #[test]
    fn a_forced_value_is_decoded_as_codex_decodes_it() {
        for text in ["Zg", "Zh==", "Zm-v", "Zm9v=", "Zm 9v"] {
            assert_eq!(
                decoded(Administered::Set(text.into())),
                Administered::Unreadable("is not base64".into()),
                "{text}"
            );
        }
        assert_eq!(
            decoded(Administered::Set(" Zg== ".into())),
            Administered::Set("f".into())
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
            store.to_use_the_file("codex"),
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
            store.to_use_the_file("codex"),
            format!(
                "To use the file store, set `cli_auth_credentials_store = \"file\"` in {OWN}, \
                 then sign in again with `codex login`"
            )
        );
        let store = machine(Os::Linux, &[(Layer::Managed, KEYRING)]);
        assert_eq!(
            store.to_use_the_file("codex"),
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
                Some(format!(
                    "{} {why}",
                    layer.place(&system_dir(Os::Linux, None), Path::new(OWN))
                )),
                "{text}"
            );
        }
        let unreadable = resolve(
            Os::Linux,
            &etc_codex(),
            Path::new(OWN),
            &|layer| match layer {
                Layer::Packaged => Administered::Set(PACKAGED.into()),
                Layer::Required => Administered::Unreadable("cannot be read: denied".into()),
                _ => Administered::Unset,
            },
        );
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
                store.to_use_the_file("codex"),
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
                    layer.place(&system_dir(Os::Linux, None), Path::new(OWN))
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
    fn a_sign_in_takes_the_file_store_unless_an_administrator_set_another() {
        let host = crate::host::memory::MemoryHost::new();
        let ctx = Context::for_unit_test().with_memory_stores(host.clone());
        let (own, at) = (Path::new(OWN), |name| system_file(&ctx, name));
        assert_eq!(of_a_sign_in(&ctx, own).backend, Backend::File);
        host.administers(at("config.toml"), KEYRING);
        let store = of_a_sign_in(&ctx, own);
        assert_eq!(
            (store.backend, store.chosen_by),
            (Backend::File, Layer::CommandLine),
            "the -c is over the system's config"
        );
        host.administers(at("managed_config.toml"), KEYRING);
        let managed = match crate::host::OS {
            Os::MacOs | Os::Linux => Backend::Keyring,
            Os::Windows => Backend::File,
        };
        assert_eq!(of_a_sign_in(&ctx, own).backend, managed);
        host.administers(at("managed_config.toml"), FILE);
        host.administers(at("requirements.toml"), EPHEMERAL);
        let store = of_a_sign_in(&ctx, own);
        assert_eq!(
            (store.backend, store.chosen_by),
            (Backend::Ephemeral, Layer::Required)
        );
    }

    /// What the host says an administrator set is what is read, at Codex's paths, and a
    /// managed preference is read as the base64 Codex reads it as.
    #[test]
    fn what_an_administrator_set_is_read_through_the_host() {
        let host = crate::host::memory::MemoryHost::new();
        let ctx = Context::for_unit_test().with_memory_stores(host.clone());
        let own = Path::new(OWN);
        assert_eq!(of(&ctx, own).chosen_by, Layer::Packaged);
        host.administers(system_file(&ctx, "requirements.toml"), KEYRING);
        assert_eq!(of(&ctx, own).chosen_by, Layer::Required);
        host.administers_unreadably(system_file(&ctx, "config.toml"), "cannot be read: denied");
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
            Os::Linux | Os::Windows => assert_eq!(store.backend, Backend::File),
        }
    }

    // Codex 0.163.0 on Windows: OpenAI\Codex's config.toml and requirements.toml around the
    // person's own, no managed_config.toml, and the secret store on by default.
    #[test]
    fn on_windows_codex_reads_its_machine_folder_and_keeps_a_keyring_login_in_secrets() {
        let store = machine(Os::Windows, &[]);
        assert_eq!(
            (store.backend, store.chosen_by),
            (Backend::File, Layer::Packaged)
        );
        let store = machine(Os::Windows, &[(Layer::System, KEYRING)]);
        assert_eq!(
            (store.backend, store.chosen_by),
            (Backend::Secrets, Layer::System)
        );
        assert_eq!(
            store.setting(),
            format!(
                "`cli_auth_credentials_store = \"keyring\"` in {}, with `secret_auth_storage` on \
                 by default",
                program_data("config.toml")
            )
        );
        assert_eq!(
            store.to_use_the_file("codex"),
            format!(
                "To use the file store, set `cli_auth_credentials_store = \"file\"` in {OWN}, \
                 then sign in again with `codex login`"
            )
        );
        let auto = "cli_auth_credentials_store = \"auto\"\n";
        assert_eq!(
            machine(Os::Windows, &[(Layer::Own, auto)]).backend,
            Backend::Secrets
        );
        let off = "[features]\nsecret_auth_storage = false\n";
        assert_eq!(
            machine(Os::Windows, &[(Layer::Own, KEYRING), (Layer::System, off)]).backend,
            Backend::Keyring
        );
        assert_eq!(
            machine(Os::Windows, &[(Layer::Own, auto), (Layer::System, off)]).backend,
            Backend::Either
        );
        assert_eq!(
            machine(
                Os::Windows,
                &[(Layer::Own, FILE), (Layer::Managed, KEYRING)]
            )
            .backend,
            Backend::File,
            "no managed_config.toml"
        );
        let store = machine(
            Os::Windows,
            &[
                (Layer::Own, KEYRING),
                (Layer::System, KEYRING),
                (Layer::CommandLine, FILE_STORE),
            ],
        );
        assert_eq!(
            (store.backend, store.chosen_by),
            (Backend::File, Layer::CommandLine),
            "a sign-in's -c"
        );

        let store = machine(
            Os::Windows,
            &[(Layer::Own, FILE), (Layer::Required, EPHEMERAL)],
        );
        assert_eq!(
            (store.backend, store.chosen_by),
            (Backend::Ephemeral, Layer::Required)
        );
        let required = program_data("requirements.toml");
        assert_eq!(
            store.setting(),
            format!("pinned to `ephemeral` by {required}")
        );
        assert_eq!(
            store.to_use_the_file("codex"),
            format!(
                "No line in your own config.toml can change it: {required} pins it, and only an \
                 administrator can change that"
            )
        );
        let store = machine(
            Os::Windows,
            &[(Layer::Own, KEYRING), (Layer::Required, FILE)],
        );
        assert_eq!(
            (store.backend, store.chosen_by),
            (Backend::File, Layer::Required)
        );
        let store = machine(Os::Windows, &[(Layer::Own, FILE), (Layer::System, "]")]);
        assert_eq!(store.backend, Backend::Unknown);
        assert_eq!(
            store.why_unknown(),
            Some(format!(
                "{} is not TOML Codex can read, at line 1",
                program_data("config.toml")
            ))
        );
    }

    #[test]
    fn codexs_machine_folder_on_windows_is_in_the_known_program_data_folder() {
        let data = PathBuf::from(r"D:\Data");
        assert_eq!(
            system_dir(Os::Windows, Some(data.clone())),
            data.join("OpenAI").join("Codex")
        );
        assert_eq!(
            system_dir(Os::Windows, None),
            PathBuf::from(r"C:\ProgramData")
                .join("OpenAI")
                .join("Codex"),
            "Codex's own fallback"
        );
        for os in [Os::MacOs, Os::Linux] {
            assert_eq!(
                system_dir(os, Some(data.clone())),
                PathBuf::from("/etc/codex")
            );
        }
    }

    fn restricted(os: Os, holds: &[(Layer, &str)]) -> Restrictions {
        let holds: HashMap<Layer, Administered> = holds
            .iter()
            .map(|&(layer, text)| (layer, Administered::Set(text.to_string())))
            .collect();
        restrictions(os, &system_dir(os, None), Path::new(OWN), &|layer| {
            holds.get(&layer).cloned().unwrap_or(Administered::Unset)
        })
    }

    // Codex 0.164.0's enforce_login_restrictions holds the ID token's workspace to what the
    // configuration forces, trimmed, and signs out of any other.
    #[test]
    fn codex_signs_out_of_a_workspace_its_configuration_leaves_out() {
        let none = restricted(Os::Linux, &[]);
        assert_eq!(none.refuses(Some("a"), Some("a")), None);
        assert!(none.said().is_empty());

        let one = restricted(
            Os::Linux,
            &[(Layer::Own, "forced_chatgpt_workspace_id = \" a \"\n")],
        );
        assert_eq!(one.refuses(Some("a"), Some("a")), None);
        assert_eq!(
            one.refuses(Some("b"), Some("b")).as_deref(),
            Some(
                "/Users/someone/.codex/config.toml sets `forced_chatgpt_workspace_id` to `a`, and \
                 this login is of `b`, so the interactive `codex` signs out of it as it starts, \
                 and deletes it"
            )
        );
        assert!(
            one.refuses(None, Some("a"))
                .is_some_and(|said| said.contains("this login is of no workspace")),
            "the ID token's workspace is the one held to it"
        );
        assert_eq!(
            one.refuses(Some("a"), Some("b")).as_deref(),
            Some(
                "/Users/someone/.codex/config.toml sets `forced_chatgpt_workspace_id` to `a`, and \
                 this login is of `b`, so Codex does not use it"
            ),
            "and its AuthManager holds `tokens.account_id` to it"
        );

        let two = restricted(
            Os::Linux,
            &[(Layer::Own, "forced_chatgpt_workspace_id = [\"a\", \"b\"]\n")],
        );
        assert_eq!(two.refuses(Some("b"), Some("b")), None);
        assert!(two.refuses(Some("c"), Some("c")).is_some());

        for os in [Os::MacOs, Os::Linux, Os::Windows] {
            let replaced = restricted(
                os,
                &[
                    (Layer::System, "forced_chatgpt_workspace_id = \"a\"\n"),
                    (Layer::Own, "forced_chatgpt_workspace_id = []\n"),
                ],
            );
            assert_eq!(
                replaced.refuses(Some("b"), Some("b")),
                None,
                "{os:?}: an empty list over another is no restriction"
            );
        }
        let managed = [
            (Layer::Own, "forced_chatgpt_workspace_id = \"a\"\n"),
            (Layer::Managed, "forced_chatgpt_workspace_id = \"b\"\n"),
        ];
        assert!(
            restricted(Os::Linux, &managed)
                .refuses(Some("a"), Some("a"))
                .is_some()
        );
        assert_eq!(
            restricted(Os::Windows, &managed).refuses(Some("a"), Some("a")),
            None,
            "Windows reads no managed_config.toml"
        );

        let api = restricted(
            Os::MacOs,
            &[(Layer::System, "forced_login_method = \"api\"\n")],
        );
        assert!(
            api.refuses(Some("a"), Some("a"))
                .is_some_and(|said| said.contains("`forced_login_method = \"api\"`"))
        );
        let chatgpt = restricted(
            Os::MacOs,
            &[(Layer::System, "forced_login_method = \"chatgpt\"\n")],
        );
        assert_eq!(chatgpt.refuses(Some("a"), Some("a")), None);
        assert_eq!(
            chatgpt.said(),
            [format!(
                "`forced_login_method` set to `chatgpt` in {}",
                etc("config.toml")
            )]
        );
    }

    // Codex 0.164.0's AuthManager leaves unused a login its requirements leave out, looking at
    // `tokens.account_id` first, and an empty list of workspaces allows none.
    #[test]
    fn codex_leaves_unused_what_its_requirements_leave_out() {
        let required = |text: &str| restricted(Os::Linux, &[(Layer::Required, text)]);
        let workspaces = required("allowed_chatgpt_workspaces = [\"a\"]\n");
        assert_eq!(workspaces.refuses(Some("b"), Some("a")), None);
        assert!(
            workspaces.refuses(Some("a"), Some("b")).is_some_and(
                |said| said.ends_with("this login is of `b`, so Codex does not use it")
            )
        );
        assert!(
            required("allowed_chatgpt_workspaces = []\n")
                .refuses(Some("a"), Some("a"))
                .is_some()
        );
        assert!(
            required("allowed_login_methods = [\"api\"]\n")
                .refuses(Some("a"), Some("a"))
                .is_some()
        );
        assert_eq!(
            required("allowed_login_methods = [\"api\", \"chatgpt\"]\n")
                .refuses(Some("a"), Some("a")),
            None
        );
        assert!(
            restricted(
                Os::Linux,
                &[(Layer::Own, "allowed_chatgpt_workspaces = [\"a\"]\n")]
            )
            .refuses(Some("b"), Some("b"))
            .is_none(),
            "a requirement in a file of configuration is none of Codex's"
        );
        let preference = restricted(
            Os::MacOs,
            &[
                (Layer::Required, "allowed_chatgpt_workspaces = [\"a\"]\n"),
                (
                    Layer::RequiredByPreference,
                    "allowed_chatgpt_workspaces = [\"b\"]\n",
                ),
            ],
        );
        assert_eq!(preference.refuses(Some("b"), Some("b")), None);
    }

    // Codex 0.164.0 does not start over any of these, so which logins it keeps is not known.
    #[test]
    fn a_restriction_codex_cannot_take_refuses_every_login() {
        for (layer, text) in [
            (Layer::Own, "forced_chatgpt_workspace_id = \"a,b\"\n"),
            (Layer::Own, "forced_chatgpt_workspace_id = 7\n"),
            (Layer::System, "forced_login_method = \"sso\"\n"),
            (Layer::Required, "allowed_login_methods = \"chatgpt\"\n"),
            (Layer::Required, "allowed_chatgpt_workspaces = [1]\n"),
            (Layer::Required, "allowed_chatgpt_workspaces = [\n"),
        ] {
            let found = restricted(Os::Linux, &[(layer, text)]);
            assert!(found.unreadable(), "{text}");
            assert!(
                found
                    .refuses(Some("a"), Some("a"))
                    .is_some_and(|said| said.contains("so Codex does not start")),
                "{text}"
            );
        }
    }
}
