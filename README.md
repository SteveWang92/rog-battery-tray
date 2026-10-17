# rog-battery-tray

A lightweight Windows tray battery monitor for ASUS ROG wireless peripherals. It needs no
Armoury Crate and runs as a single portable `.exe`.

- The tray icon shows the battery percentage as digits, drawn for the taskbar's scale and
  theme. A low battery shows on an orange-red fill, charging on a green fill, a sleeping
  or unresponsive device as a grey `–`, and no supported device as a grey `?`.
- Hovering an icon shows the device, its battery, whether it is charging and when it was
  last read.
- Every setting is in the right-click menu: refresh now, the low-battery threshold, the
  refresh interval, one icon per device or one merged icon showing the lowest level, and an
  optional battery log.
- It only ever sends battery queries; it never changes a device setting.

## Supported devices

| Device | Connection | USB ID | Status |
| --- | --- | --- | --- |
| ROG Azoth | ROG OMNI receiver | `0B05:1ACE` | Tested |
| ROG Azoth | Azoth 2.4 GHz receiver | `0B05:1A85` | Untested |
| ROG Azoth | USB cable | `0B05:1A83` | Untested |

A report from an untested device, working or not, is welcome as an issue.

## Usage

Run `rog-battery-tray.exe`. Only one copy runs at a time.

From a terminal, `rog-battery-tray.exe --once` prints every detected device's battery and
exits. PowerShell and Command Prompt do not wait for a windowed program, so pipe it there
to see the output: `rog-battery-tray.exe --once | more`.

The battery log, when switched on from the menu, is written to
`%LOCALAPPDATA%\rog-battery-tray\battery.log`.

## Building

Requires a stable Rust toolchain with the MSVC target on Windows.

```sh
cargo build --release
```

The executable is `target/release/rog-battery-tray.exe`.

## Credits

The device IDs and HID protocol details come from
[G-Helper](https://github.com/seerge/g-helper) (GPL-3.0). This project is an independent
implementation and contains none of its code.

## License

[MIT](LICENSE)
