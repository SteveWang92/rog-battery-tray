# rog-battery-tray contributor guidance

This is the authoritative guidance for working in rog-battery-tray, for a coding agent and
a human contributor alike. The maintainer's own cross-project rules live outside this
repository; where they and the rules below differ, the rules below win.

## What this is

A lightweight Windows tray battery monitor for ASUS ROG wireless peripherals that needs no
Armoury Crate. It is a Cargo workspace:

| Crate | Role |
| --- | --- |
| `crates/rog-battery` | Device discovery and battery queries over HID. No UI, so other applications can reuse it. |
| `crates/rog-battery-tray` | The tray application, plus the `--once` command-line mode that prints every detected device's battery and exits. |

## Project exceptions

- Planned work and progress live in the maintainer's untracked `PLAN.local.md`, not in
  GitHub issues. Read it before starting work when it is present; do not open issues for
  planned work.
- The repository stays private until every milestone has been tested. Only the maintainer
  decides when it becomes public; never change its visibility.

## Device rules

- Read-only: send query commands only. Never write a device setting, firmware value or
  lighting state.
- The protocol facts come from G-Helper (GPL-3.0). This project is an independent MIT
  implementation: reuse the facts (IDs, command bytes, reply offsets), never its code.
- A device that is not tested on real hardware is marked untested wherever it is listed.

## Repository conventions

- Line endings are LF everywhere (`.gitattributes`); formatting is rustfmt with the
  settings in `rustfmt.toml`.
- The root `.gitignore` is an allowlist: a new top-level file or directory needs its own
  `!/name` entry, or Git skips it silently.
- Local-only files carry `.local` in their name and are covered by the repository-wide
  `*.local` and `*.local.*` rules; do not add one-off ignore entries.
- Keep tracked content free of machine-specific paths, account details and anything else
  personal to one contributor.
- Documents and comments describe the current state only; history belongs in commits and
  `CHANGELOG.md`.

## Releases

- Work lands on `dev`; `main` only receives releases.
- Releases are cut with [changedeck](https://github.com/SteveWang92/changedeck)
  (`prep`, `reversion`, `ship`), configured by `changedeck.json`, which lists every file
  that carries the version. A new crate's `Cargo.toml` and `Cargo.lock` entry go there.
- Record each user-facing change under `## [Unreleased]` in `CHANGELOG.md` in the same
  commit that makes it.
- Only the maintainer releases.

## Checks

Run before committing; CI runs the same on pull requests:

```sh
cargo fmt --all --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo build --locked --workspace --release
```
