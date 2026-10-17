# rog-battery-tray

A lightweight Windows tray battery monitor for ASUS ROG wireless peripherals. It needs no
Armoury Crate and runs as a single portable `.exe`.

- The tray icon shows the battery percentage as digits, drawn for the taskbar's scale and
  theme. A low battery shows on an orange-red fill and charging on a green fill. A device
  that falls asleep keeps its last reading in grey, or shows a grey empty battery if it
  has not been read yet. With no supported device connected, the icon is a grey battery
  with a slash through it.
- Hovering an icon shows the device, its battery, whether it is charging and when it was
  last read.
- Every setting is in the right-click menu: refresh now, the low-battery threshold, the
  refresh interval, one icon per device or one merged icon showing the lowest level, a
  full-charge alert, starting with Windows, the language (English or Simplified Chinese)
  and an optional battery log. The menu stays open while settings are changed and closes
  on a click outside it.
- A Windows notification appears once when a device drops to the low-battery threshold,
  and again only after it has been charged. The full-charge notification is off by
  default.
- It only ever sends battery queries; it never changes a device setting.

## Supported devices

Each cell shows the USB ID the device appears as over that connection, and its status:
![Tested](https://img.shields.io/badge/Tested-2ea44f) on real hardware,
![Untested](https://img.shields.io/badge/Untested-dfb317) implemented but not yet tried, or
![Not supported](https://img.shields.io/badge/Not%20supported-9e9e9e).

| Model | ROG OMNI receiver | Own 2.4 GHz receiver | USB cable | Bluetooth |
| --- | --- | --- | --- | --- |
| ROG Azoth | ![0B05:1ACE Tested](https://img.shields.io/badge/0B05%3A1ACE-Tested-2ea44f) | ![0B05:1A85 Untested](https://img.shields.io/badge/0B05%3A1A85-Untested-dfb317) | ![0B05:1A83 Untested](https://img.shields.io/badge/0B05%3A1A83-Untested-dfb317) | ![Not supported](https://img.shields.io/badge/Not%20supported-9e9e9e) |

A report from an untested device, working or not, is welcome as an issue.

## Usage

Run `rog-battery-tray.exe`. Only one copy runs at a time.

From a terminal, `rog-battery-tray.exe --once` prints every detected device's battery and
exits. PowerShell and Command Prompt do not wait for a windowed program, so pipe it there
to see the output: `rog-battery-tray.exe --once | more`.

Settings are saved to `%APPDATA%\rog-battery-tray\config.toml`. The battery log, when
switched on from the menu, is written to `%LOCALAPPDATA%\rog-battery-tray\battery.log`.

On first run the program adds itself to the current user's startup programs
(`HKCU\Software\Microsoft\Windows\CurrentVersion\Run`); switching off "Start with Windows"
removes that entry. To show notifications under its own name it registers
`HKCU\Software\Classes\AppUserModelId\rog-battery-tray` and keeps the notification icon in
`%LOCALAPPDATA%\rog-battery-tray\icon.ico`. If the program ever fails, the error is recorded
in `crash.log` in the same folder. Nothing else is written.

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
