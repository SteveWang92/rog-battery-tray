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
| `crates/rog-battery-tray` | The tray application. With `--once` it prints every detected device's battery and exits instead. |

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

## Tray rules

- Battery is shown only through the tray icon, the card a left click on it shows, the
  native right-click menu and Windows toasts. There is no main window or settings window,
  and no native tooltip: the shell shows it only after a hover delay no program can
  shorten, and cuts it at 128 characters.
- The card (`popup.rs`) is a GDI-drawn popup that closes when it loses activation. Its
  window procedure runs inside Win32 calls the tray makes while holding its own state, so
  the card keeps its state apart from the tray's.
- Icons are drawn in code, never stored as image files: tray icons at runtime for the
  taskbar's DPI and theme (`icon.rs`), the application icon from `app_icon.rs`, which
  `build.rs` embeds in the executable with its version information and the tray writes out
  for toasts.
- Native menus close after a click. A `WH_MSGFILTER` hook in `tray.rs` forwards a click on
  a command item as its `WM_COMMAND` and swallows it, so the menu stays open; menu items
  are therefore updated in place while it shows, and anything that replaces the menu or
  an icon waits until it closes. A language change rebuilds the menu after it closes and
  reopens it from the same icon.
- The shell knows a tray icon by the executable's path and the icon's number, and keeps
  its "always show" setting under them. Icons are numbered in creation order, so as long
  as none is recreated, every run numbers them the same. Never recreate an icon to change
  it: give the existing one a new image, tooltip or menu, or it appears as a new program in
  the taskbar settings. Icons carry no GUID: the shell binds a GUID to one executable
  path and refuses it while a stale registration holds it.
- A menu shown again by the tray itself goes through `TrackPopupMenu` directly, never
  `TrayIcon::show_menu`, which holds the icon borrowed for as long as the menu is open;
  the menu hook updating that icon would then panic and end the process.
- A panic is written to `%LOCALAPPDATA%\rog-battery-tray\crash.log`; the tray has no
  console to show it.
- HID queries run on the poller thread, never on the thread that owns the tray.
- Settings live in `%APPDATA%\rog-battery-tray\config.toml`. The file may be edited by
  hand, so it is validated on load; an invalid file means defaults plus one toast, and the
  file is left alone until a setting changes.
- Autostart is the per-user `Run` registry value, the single source of truth for that
  setting; it is not duplicated in the config file. A value pointing at another copy's
  path counts as off.
- Every user-facing string lives in `strings.rs`, in English and Simplified Chinese.
- The battery log is diagnostic: off by default, switched from the menu, written to
  `%LOCALAPPDATA%\rog-battery-tray\battery.log`.

## Releases

- Work lands on `dev`; `main` only receives releases.
- Releases are cut with [changedeck](https://github.com/SteveWang92/changedeck)
  (`prep`, `reversion`, `ship`), configured by `changedeck.json`, which lists every file
  that carries the version. A new crate's `Cargo.toml` and `Cargo.lock` entry go there.
- Publishing a release runs `.github/workflows/release.yml`, which builds
  `rog-battery-tray.exe` from the tag and attaches it under that unversioned name, so an
  update replaced at the same path keeps its autostart entry.
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
