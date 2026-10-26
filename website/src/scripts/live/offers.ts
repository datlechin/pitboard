// What an account offers (present/accounts.rs `actions`, AccountCommands.swift).
import { warmStanding, type State } from "../../lib/model.ts";
import { isInUse, signInExpired } from "../../lib/words.ts";
import { openSite, siteOf, switchOffer } from "../../data/window.ts";
import type { Account } from "../../data/world.ts";

export interface Offers {
  use: { title: string; enabled: boolean };
  signInAgain: boolean;
  rename: boolean;
  // only a Claude Code account warms up
  warm: { enabled: boolean } | null;
  // the account's window, where its tool has a site
  open: string | null;
  copy: boolean;
  forget: boolean;
}

export function offersOf(a: Account, s: State): Offers {
  const on = isInUse(a, s);
  const site = siteOf(a.tool);
  return {
    use: { title: switchOffer(a.label), enabled: !on && !signInExpired(a, s) && !s.switching },
    signInAgain: !s.signing,
    rename: true,
    warm: a.tool === "claude" ? { enabled: warmStanding(s, a, "now").stands === "due" } : null,
    open: site ? openSite(site) : null,
    copy: a.email !== "",
    forget: !on,
  };
}
