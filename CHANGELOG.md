# Changelog

All notable changes to rog-battery-tray are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses
[Semantic Versioning](https://semver.org/spec/v2.0.0.html). Each release is tagged `vX.Y.Z`
on `main`, with the matching section below as its notes.

## [Unreleased]

## [0.1.0] - 2026-10-18

### Added

- `--once` prints the battery of each ROG Azoth found over OMNI, its receiver or USB.
- Other battery-powered ROG keyboards and mice, untested, as listed in the README.
- Tray icon per device showing battery digits, coloured when low or charging.
- A sleeping device keeps showing its last battery reading, greyed out.
- Grey crossed-out battery icon when no supported device is found.
- Tray icons keep their "always show" setting across restarts and language changes.
- Clicking an icon shows device, battery, charging state and last update time.
- Right-click menu for refresh, low-battery threshold, refresh interval and merged icon.
- The menu stays open while changing settings until clicked outside.
- Application icon in Explorer, Task Manager and notifications.
- Optional battery log, switched on from the tray menu.
- Toasts when a device runs low and, optionally, when charging completes.
- Settings are saved and restored across restarts.
- Starts with Windows by default, switchable from the menu.
- English and Simplified Chinese, following the system language by default.

[Unreleased]: https://github.com/SteveWang92/rog-battery-tray/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/SteveWang92/rog-battery-tray/releases/tag/v0.1.0
