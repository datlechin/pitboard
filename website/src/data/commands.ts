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
} satisfies Record<string, Command>;

const scoop: Command[] = [
  {
    copy: "scoop bucket add pitboard https://github.com/datlechin/scoop-bucket",
    html: `scoop bucket add pitboard <wbr>https://<wbr>github.com/<wbr><span class="${group}">datlechin/<wbr>scoop-bucket</span>`,
  },
  { copy: "scoop install pitboard/pitboard", html: "scoop install pitboard/pitboard" },
];

// Scoop's bucket holds no release until 1.0.0, and never a beta.
export function windowsScoop(release: { version: string; prerelease: boolean }): Command[] | null {
  return !release.prerelease && Number(release.version.split(".")[0]) >= 1 ? scoop : null;
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

const KEYRING = "/usr/share/keyrings/pitboard-archive-keyring.pgp";

// Each distribution's setup of the repository and install of the app, from the beta channel until a
// full release has the Linux app, which stable then holds.
export function linuxWays(release: { prerelease: boolean }) {
  const setup = release.prerelease ? "pitboard-beta" : "pitboard";
  return {
    apt: [
      fetched(KEYRING, "pitboard-archive-keyring.pgp"),
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
