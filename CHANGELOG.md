# Changelog

All notable changes are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Added
- `enroll`, `use`, `forget`, `status` and `doctor`.
- Live usage in `status`, asked of Anthropic for every enrolled account at once.
- `--json` on every command, emitting a versioned envelope with stable error codes.
- Shell completions and a man page, generated from the command definition.
- Linux support, using a file vault for parked logins. Not yet confirmed on a real install.
- An audit log of every change pitboard makes.

### State file
- Schema 2. Earlier schema-1 files are refused rather than migrated; nothing was ever
  released that wrote them.
