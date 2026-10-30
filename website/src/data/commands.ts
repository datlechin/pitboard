import { SLUG } from "./site.ts";

// Each command as copied, and as shown: <wbr> marks where a phone may break it, a group where it
// may not unless the screen is very narrow, a word where it never may.
export interface Command {
  copy: string;
  html: string;
}

const group = "whitespace-nowrap max-xs:whitespace-normal";
const word = "whitespace-nowrap";

const installSh = `curl -fsSL <wbr>https://<wbr>usepitboard.com/<wbr><span class="${group}">install.sh | sh</span>`;

export const commands = {
  brewApp: {
    copy: "brew install --cask datlechin/tap/pitboard-app",
    html: `brew install --cask <wbr><span class="${group}">datlechin/<wbr>tap/<wbr><span class="${word}">pitboard-app</span></span>`,
  },
  brewCli: {
    copy: "brew install datlechin/tap/pitboard",
    html: `brew install <wbr><span class="${group}">datlechin/<wbr>tap/<wbr><span class="${word}">pitboard</span></span>`,
  },
  installScript: { copy: "curl -fsSL https://usepitboard.com/install.sh | sh", html: installSh },
  binstall: { copy: "cargo binstall pitboard", html: "cargo binstall pitboard" },
  cargoInstall: {
    copy: "cargo install --locked pitboard",
    html: "cargo install --locked pitboard",
  },
  wget: {
    copy: "wget -qO- https://usepitboard.com/install.sh | sh",
    html: `wget -qO- <wbr>https://<wbr>usepitboard.com/<wbr><span class="${group}">install.sh | sh</span>`,
  },
  installDir: {
    copy: 'curl -fsSL https://usepitboard.com/install.sh | PITBOARD_INSTALL_DIR="$HOME/tools/bin" sh',
    html: `curl -fsSL <wbr>https://<wbr>usepitboard.com/<wbr><span class="${group}">install.sh |</span> <wbr><span class="${group}">PITBOARD_INSTALL_DIR=<wbr>&quot;$HOME/<wbr>tools/<wbr>bin&quot; sh</span>`,
  },
  shasum: {
    copy: "shasum -a 256 -c SHA256SUMS --ignore-missing",
    html: `shasum -a 256 -c SHA256SUMS <wbr><span class="${group}"><span class="${word}">--ignore-missing</span></span>`,
  },
  sha256sum: {
    copy: "sha256sum -c SHA256SUMS --ignore-missing",
    html: `sha256sum -c SHA256SUMS <wbr><span class="${group}"><span class="${word}">--ignore-missing</span></span>`,
  },
  selectString: {
    copy: "Select-String -Path SHA256SUMS -Pattern (Get-ChildItem Pitboard-*.msi, pitboard-*.zip | Get-FileHash).Hash",
    html: `Select-String <wbr><span class="${group}">-Path SHA256SUMS</span> <wbr><span class="${word}">-Pattern</span> <wbr><span class="${group}">(Get-ChildItem <wbr>Pitboard-*.msi, <wbr>pitboard-*.zip</span> <wbr><span class="${group}">| <span class="${word}">Get-FileHash).Hash</span></span>`,
  },
} satisfies Record<string, Command>;

const scoop: Command[] = [
  {
    copy: "scoop bucket add pitboard https://github.com/datlechin/scoop-bucket",
    html: `scoop bucket add pitboard <wbr>https://<wbr>github.com/<wbr><span class="${group}">datlechin/<wbr>scoop-bucket</span>`,
  },
  { copy: "scoop install pitboard/pitboard", html: "scoop install pitboard/pitboard" },
];

// Cargo's newest build refuses every change on Windows until 1.0.0, and Scoop has none until then.
export function windowsWays(release: { version: string; prerelease: boolean }) {
  const current = !release.prerelease && Number(release.version.split(".")[0]) >= 1;
  const crate = current ? "pitboard" : `pitboard@${release.version}`;
  const name = `<span class="${word}">${crate}</span>`;
  return {
    binstall: { copy: `cargo binstall ${crate}`, html: `cargo binstall <wbr>${name}` },
    cargoInstall: {
      copy: `cargo install --locked ${crate}`,
      html: `cargo install --locked <wbr>${name}`,
    },
    scoop: current ? scoop : null,
  } satisfies Record<string, Command | Command[] | null>;
}

// A path or URL that may break after each slash, its last part never.
function slashed(path: string): string {
  const [scheme, rest] = path.includes("://") ? path.split("://") : [null, path];
  const parts = rest.split("/");
  const last = parts.pop();
  return `${scheme ? `${scheme}://<wbr>` : ""}${parts.map((p) => `${p}/<wbr>`).join("")}<span class="${word}">${last}</span>`;
}

const PACKAGES = "https://packages.usepitboard.com";

// sudo curl, saving a file the repository serves where the system reads it.
function fetched(to: string, file: string): Command {
  return {
    copy: `sudo curl -fsSL -o ${to} ${PACKAGES}/${file}`,
    html: `sudo curl -fsSL -o <wbr>${slashed(to)} <wbr>${slashed(`${PACKAGES}/${file}`)}`,
  };
}

const said = (copy: string): Command => ({ copy, html: copy });

// The repository's beta channel until a full release has the Linux app, which stable then holds.
export function linuxWays(release: { prerelease: boolean }) {
  const setup = release.prerelease ? "pitboard-beta" : "pitboard";
  return {
    apt: [
      fetched("/usr/share/keyrings/pitboard-archive-keyring.pgp", "pitboard-archive-keyring.pgp"),
      fetched(`/etc/apt/sources.list.d/${setup}.sources`, `${setup}.sources`),
      fetched("/etc/apt/preferences.d/pitboard.pref", "pitboard.pref"),
      said("sudo apt update"),
      said("sudo apt install pitboard-app"),
    ],
    dnf: [
      {
        copy: `sudo dnf config-manager addrepo --from-repofile=${PACKAGES}/${setup}.repo`,
        html: `sudo dnf config-manager <wbr>addrepo <wbr>--from-repofile=<wbr>${slashed(`${PACKAGES}/${setup}.repo`)}`,
      },
      said("sudo dnf install pitboard-app"),
    ],
    zypper: [
      {
        copy: `sudo zypper addrepo --refresh ${PACKAGES}/${setup}.repo`,
        html: `sudo zypper addrepo <wbr>--refresh <wbr>${slashed(`${PACKAGES}/${setup}.repo`)}`,
      },
      said("sudo zypper install pitboard-app"),
    ],
  } satisfies Record<string, Command[]>;
}

export function attestCommand(file: string): Command {
  const dot = file.lastIndexOf(".");
  const [stem, ext] = [file.slice(0, dot), file.slice(dot)];
  return {
    copy: `gh attestation verify ${file} --repo ${SLUG}`,
    html: `gh attestation verify <wbr><span class="${group}">${stem.split("-").join("-<wbr>")}<wbr><span class="${word}">${ext}</span></span> <wbr><span class="${group}"><span class="${word}">--repo</span> <span class="${word}">${SLUG}</span></span>`,
  };
}
