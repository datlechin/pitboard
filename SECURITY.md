# Security

pitboard handles OAuth refresh tokens that grant full access to a paid account. This file
says where they live, what pitboard defends against, and what it does not.

## Where your credentials are

**macOS.** Claude Code's own login stays in the keychain item it created. Parked copies are
keychain items named `pitboard-park-<account>-<time>`, in your login keychain. They are
read and written only through `/usr/bin/security`, the one application the item's access
list trusts. No token is ever passed on a command line, where `ps` could see it; it goes
to `security` on standard input.

**Linux.** Claude Code keeps its login in a plaintext file, `.credentials.json`, in its
config directory. That is Claude Code's design and pitboard cannot change it. Parked copies
live in `~/.pitboard/vault/`, one file per copy, each 0600, in a directory held at 0700.
Any process running as your user can read them — the same exposure Claude Code's own file
already has.

**Never written anywhere else.** pitboard's account list, `~/.pitboard/state.json`, holds
email addresses and the identity Claude Code recorded, but no token. The audit log holds
labels, codes and times only.

## What pitboard defends against

- Mixing up accounts: every parked copy is bound to its account by a fingerprint of its
  refresh token, and a copy that does not match is refused.
- Corruption from a crash mid-switch: a switch records its intent before acting, and the
  next run finishes or safely abandons what the interrupted one started.
- Restoring a login Claude Code has already moved past: a copy that has been installed is
  never offered again, because presenting a superseded refresh token makes Claude Code
  discard the login.
- Writing alongside a running Claude Code: pitboard takes the same lock Claude Code takes
  around every credential write.
- A state directory inside a cloud-synced folder: refused, because a parked login belongs
  to exactly one machine.

## What pitboard does not defend against

- Another process running as your user. It can read what you can read.
- Another user with administrative access to your machine.
- A compromised Claude Code binary, or a compromised dependency of pitboard itself. The
  dependency tree is checked for known advisories, licences and sources in CI.

## If something goes wrong

If a switch is interrupted, run `pitboard doctor` before the next `pitboard use`.

Never copy `~/.pitboard` to another machine. pitboard refuses to read a state file written
elsewhere, and a parked login presented from a second machine can end the login on both.

## Reporting a vulnerability

Please report privately through GitHub's **Report a vulnerability** button on this
repository's Security tab, rather than in a public issue.

This is maintained by one person. Reports are acknowledged as soon as possible, and
anything that could expose a credential is handled before all other work.
