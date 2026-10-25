// What an account offers, as present/accounts.rs `offers` and its item's windows and Forget say:
// a shortcut or context menu shows only what applies now, and macOS's Account menu shows all of it,
// dimming what does not (AccountCommands.swift).
import { warmStanding, type State } from "../../lib/model.ts";
import { isInUse } from "../../lib/words.ts";
import { openSite, siteOf, useOffer } from "../../data/window.ts";
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

// a parked login past its life signs in again in place of a switch
export const expired = (a: Account, s: State) =>
  !isInUse(a, s) && a.refreshExpiresAt !== undefined && a.refreshExpiresAt <= s.now;

export function offersOf(a: Account, s: State): Offers {
  const on = isInUse(a, s);
  const site = siteOf(a.tool);
  return {
    use: { title: useOffer(a.label), enabled: !on && !expired(a, s) && !s.switching },
    signInAgain: !s.signing,
    rename: true,
    warm: a.tool === "claude" ? { enabled: warmStanding(s, a, "now").stands === "due" } : null,
    open: site ? openSite(site) : null,
    copy: a.email !== "",
    forget: !on,
  };
}
