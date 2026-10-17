//! The tray application: one icon per device (or one merged icon), its tooltip, the
//! right-click menu that holds every setting, and the battery toasts.

use std::collections::HashMap;
use std::sync::mpsc;
use std::time::Duration;

use rog_battery::{Connection, HidApi, Reading};
use tray_icon::menu::{
    CheckMenuItem, IsMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu,
};
use tray_icon::{TrayIcon, TrayIconBuilder};
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::Shell::ShellExecuteW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, MSG, PM_NOREMOVE, PeekMessageW, PostQuitMessage,
    PostThreadMessageW, SW_SHOWNORMAL, SetTimer, TranslateMessage, WM_APP, WM_TIMER,
};

use crate::config::{self, Loaded, Settings};
use crate::icon::{self, Face};
use crate::poller::{self, Command, DeviceStatus, Snapshot};
use crate::strings::{Language, Strings};
use crate::system;
use crate::theme::{self, Look};

const PROJECT_URL: &str = "https://github.com/SteveWang92/rog-battery-tray";
const SUPPORTED_URL: &str = "https://github.com/SteveWang92/rog-battery-tray#supported-devices";

/// How often the taskbar's theme and DPI are checked for a change.
const LOOK_CHECK_MS: u32 = 2000;

const THRESHOLDS: [u8; 4] = [0, 10, 20, 30];
const INTERVALS: [u64; 3] = [30, 60, 300];

/// Which one-shot toasts a device can still raise.
struct Alerts {
    /// Cleared by the low-battery toast; set again once the device is seen charging.
    low_armed: bool,
    /// Set while the device charges below 100%; cleared by the full-charge toast.
    full_pending: bool,
}

struct Tray {
    settings: Settings,
    commands: mpsc::Sender<Command>,
    snapshot: Option<Snapshot>,
    icons: Vec<TrayIcon>,
    look: Look,
    alerts: HashMap<(&'static str, Connection), Alerts>,
}

pub fn run(api: HidApi) {
    system::register_toasts();
    let settings = match config::load() {
        Loaded::Saved(settings) => settings,
        Loaded::FirstRun => {
            let settings = Settings::default();
            system::set_autostart(true);
            config::save(&settings);
            settings
        }
        // The file is left as it is until a setting is changed from the menu.
        Loaded::Invalid => {
            let settings = Settings::default();
            system::toast("rog-battery-tray", settings.language.strings().bad_config);
            settings
        }
    };
    let look = Look::current();
    theme::apply_menu_theme(look.light);

    let main_thread = unsafe { GetCurrentThreadId() };
    let mut msg: MSG = unsafe { std::mem::zeroed() };
    // Creates this thread's message queue before the poller first posts to it.
    unsafe { PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_NOREMOVE) };

    let (snapshots, inbox) = mpsc::channel();
    let commands = poller::spawn(
        api,
        Duration::from_secs(settings.interval_secs),
        settings.battery_log,
        snapshots,
        move || unsafe {
            PostThreadMessageW(main_thread, WM_APP, 0, 0);
        },
    );
    let mut tray = Tray {
        settings,
        commands,
        snapshot: None,
        icons: Vec::new(),
        look,
        alerts: HashMap::new(),
    };
    unsafe { SetTimer(std::ptr::null_mut(), 0, LOOK_CHECK_MS, None) };

    while unsafe { GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) } > 0 {
        unsafe {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        // Snapshots and menu clicks arrive on channels; any message, including the poller's
        // wake-up and the look-check timer, is a chance to drain them.
        let mut changed = false;
        while let Ok(snapshot) = inbox.try_recv() {
            tray.raise_alerts(&snapshot);
            tray.snapshot = Some(snapshot);
            changed = true;
        }
        while let Ok(event) = MenuEvent::receiver().try_recv() {
            changed |= tray.on_menu(event.id.as_ref());
        }
        if msg.message == WM_TIMER && msg.hwnd.is_null() {
            let look = Look::current();
            if look != tray.look {
                if look.light != tray.look.light {
                    theme::apply_menu_theme(look.light);
                }
                tray.look = look;
                changed = true;
            }
            // Icons the shell refused earlier are retried here rather than a whole poll
            // interval later.
            changed |= tray.icons.is_empty();
        }
        if changed {
            tray.redraw();
        }
    }
}

impl Tray {
    fn strings(&self) -> &'static Strings {
        self.settings.language.strings()
    }

    /// Applies a menu click; returns whether the tray must be redrawn.
    fn on_menu(&mut self, id: &str) -> bool {
        let (kind, value) = id.split_once(':').unwrap_or((id, ""));
        let s = &mut self.settings;
        match kind {
            "refresh" => {
                let _ = self.commands.send(Command::Refresh);
                return false;
            }
            "low" => s.low_threshold = value.parse().expect("low ids carry a percentage"),
            "interval" => {
                s.interval_secs = value.parse().expect("interval ids carry seconds");
                let interval = Duration::from_secs(s.interval_secs);
                let _ = self.commands.send(Command::Interval(interval));
            }
            "icons" => s.merged_icon = value == "merged",
            "full" => s.full_alert = !s.full_alert,
            "autostart" => {
                system::set_autostart(!system::autostart_enabled());
                return true;
            }
            "language" => {
                s.language = match value {
                    "en" => Language::English,
                    "zh-CN" => Language::Chinese,
                    _ => Language::System,
                }
            }
            "log" => {
                s.battery_log = !s.battery_log;
                let _ = self.commands.send(Command::Logging(s.battery_log));
            }
            "about" => {
                open_url(PROJECT_URL);
                return false;
            }
            "supported" => {
                open_url(SUPPORTED_URL);
                return false;
            }
            "quit" => {
                self.icons.clear();
                unsafe { PostQuitMessage(0) };
                return false;
            }
            _ => return false,
        }
        config::save(&self.settings);
        true
    }

    fn raise_alerts(&mut self, snapshot: &Snapshot) {
        let strings = self.strings();
        for device in &snapshot.devices {
            let Reading::Battery { percent, charging } = device.reading else {
                continue;
            };
            let alerts = self
                .alerts
                .entry((device.name, device.connection))
                .or_insert(Alerts {
                    low_armed: true,
                    full_pending: false,
                });
            if charging {
                alerts.low_armed = true;
            } else if alerts.low_armed
                && self.settings.low_threshold().is_some_and(|t| percent <= t)
            {
                let (title, body) = strings.low_toast(device.name, percent);
                system::toast(&title, &body);
                alerts.low_armed = false;
            }
            // A device may stop reporting "charging" once full, so reaching 100% after
            // charging below it is what counts.
            if charging && percent < 100 {
                alerts.full_pending = true;
            } else if percent == 100 && alerts.full_pending {
                if self.settings.full_alert {
                    let (title, body) = strings.full_toast(device.name);
                    system::toast(&title, &body);
                }
                alerts.full_pending = false;
            }
        }
    }

    fn redraw(&mut self) {
        let Some(snapshot) = &self.snapshot else {
            return;
        };
        let strings = self.strings();
        let menu = self.menu(snapshot);
        let time = snapshot.at.format("%H:%M").to_string();

        // Each icon: what it shows and the devices its tooltip lists.
        let faces: Vec<(Face, Vec<&DeviceStatus>)> = if snapshot.devices.is_empty() {
            vec![(Face::NoDevice, Vec::new())]
        } else if self.settings.merged_icon {
            let lowest = snapshot
                .devices
                .iter()
                .filter_map(|d| match d.reading {
                    Reading::Battery { percent, .. } => Some((percent, d)),
                    Reading::Asleep => None,
                })
                .min_by_key(|(percent, _)| *percent);
            let face = lowest.map_or(Face::Asleep, |(_, d)| self.face(d.reading));
            vec![(face, snapshot.devices.iter().collect())]
        } else {
            snapshot
                .devices
                .iter()
                .map(|d| (self.face(d.reading), vec![d]))
                .collect()
        };

        if self.icons.len() != faces.len() {
            self.icons.clear();
        }
        for (i, (face, devices)) in faces.into_iter().enumerate() {
            let image = icon::render(face, self.look.dpi, self.look.light);
            let tooltip = if devices.is_empty() {
                strings.no_device.to_owned()
            } else {
                let lines: Vec<String> = devices
                    .iter()
                    .map(|d| {
                        let status = match d.reading {
                            Reading::Battery { percent, charging } => {
                                strings.battery_line(percent, charging)
                            }
                            Reading::Asleep => strings.asleep.to_owned(),
                        };
                        format!("{}\n{status}", device_label(strings, d))
                    })
                    .collect();
                format!("{}\n{}", lines.join("\n"), strings.updated_at(&time))
            };
            match self.icons.get(i) {
                Some(tray) => {
                    let _ = tray.set_icon(Some(image));
                    let _ = tray.set_tooltip(Some(&tooltip));
                    tray.set_menu(Some(Box::new(menu.clone())));
                }
                None => {
                    let built = TrayIconBuilder::new()
                        .with_icon(image)
                        .with_tooltip(&tooltip)
                        .with_menu(Box::new(menu.clone()))
                        .with_menu_on_left_click(false)
                        .build();
                    match built {
                        Ok(tray) => self.icons.push(tray),
                        // The shell refuses icons until the taskbar exists, as at sign-in;
                        // the look-check timer tries again.
                        Err(_) => {
                            self.icons.clear();
                            return;
                        }
                    }
                }
            }
        }
    }

    fn face(&self, reading: Reading) -> Face {
        match reading {
            Reading::Battery { percent, charging } => Face::Level {
                percent,
                charging,
                low: self.settings.low_threshold().is_some_and(|t| percent <= t),
            },
            Reading::Asleep => Face::Asleep,
        }
    }

    fn menu(&self, snapshot: &Snapshot) -> Menu {
        let t = self.strings();
        let menu = Menu::new();
        let quit = MenuItem::with_id("quit", t.quit, true, None);
        if snapshot.devices.is_empty() {
            append(
                &menu,
                &[
                    &MenuItem::new(t.no_device, false, None),
                    &MenuItem::with_id("supported", t.supported, true, None),
                    &PredefinedMenuItem::separator(),
                    &quit,
                ],
            );
            return menu;
        }

        for (i, device) in snapshot.devices.iter().enumerate() {
            let right = match device.reading {
                Reading::Battery { percent, charging } => t.row_status(percent, charging),
                Reading::Asleep => t.no_response.to_owned(),
            };
            let row = format!("{}\t{right}", device_label(t, device));
            append(
                &menu,
                &[&MenuItem::with_id(format!("device:{i}"), row, true, None)],
            );
        }

        let s = &self.settings;
        let radio = |id: String, text: &str, checked: bool| {
            CheckMenuItem::with_id(id, text, true, checked, None)
        };
        let low: Vec<CheckMenuItem> = THRESHOLDS
            .iter()
            .map(|&p| {
                let text = if p == 0 {
                    t.off.to_owned()
                } else {
                    format!("{p}%")
                };
                radio(format!("low:{p}"), &text, s.low_threshold == p)
            })
            .collect();
        let intervals: Vec<CheckMenuItem> = INTERVALS
            .iter()
            .map(|&secs| {
                let text = t.interval_label(secs);
                radio(format!("interval:{secs}"), &text, s.interval_secs == secs)
            })
            .collect();
        let icons = [
            radio("icons:each".to_owned(), t.icons_each, !s.merged_icon),
            radio("icons:merged".to_owned(), t.icons_merged, s.merged_icon),
        ];
        // Language names stay in their own language so either can be found.
        let languages = [
            radio(
                "language:system".to_owned(),
                t.follow_system,
                s.language == Language::System,
            ),
            radio(
                "language:en".to_owned(),
                "English",
                s.language == Language::English,
            ),
            radio(
                "language:zh-CN".to_owned(),
                "简体中文",
                s.language == Language::Chinese,
            ),
        ];
        let version = env!("CARGO_PKG_VERSION");

        append(
            &menu,
            &[
                &PredefinedMenuItem::separator(),
                &MenuItem::with_id("refresh", t.refresh, true, None),
                &PredefinedMenuItem::separator(),
                &submenu(t.low_alert, &low),
                &submenu(t.interval, &intervals),
                &submenu(t.icons, &icons),
                &CheckMenuItem::with_id("full", t.full_alert, true, s.full_alert, None),
                &CheckMenuItem::with_id(
                    "autostart",
                    t.autostart,
                    true,
                    system::autostart_enabled(),
                    None,
                ),
                &submenu(t.language, &languages),
                &CheckMenuItem::with_id("log", t.battery_log, true, s.battery_log, None),
                &PredefinedMenuItem::separator(),
                &MenuItem::with_id("about", format!("{}\tv{version}", t.about), true, None),
                &quit,
            ],
        );
        menu
    }
}

fn append(menu: &Menu, items: &[&dyn IsMenuItem]) {
    menu.append_items(items).expect("menu items are new");
}

fn submenu(text: &str, items: &[CheckMenuItem]) -> Submenu {
    let items: Vec<&dyn IsMenuItem> = items.iter().map(|i| i as &dyn IsMenuItem).collect();
    Submenu::with_items(text, true, &items).expect("menu items are new")
}

fn device_label(strings: &Strings, device: &DeviceStatus) -> String {
    format!(
        "{} ({})",
        device.name,
        strings.connection(device.connection)
    )
}

fn open_url(url: &str) {
    let verb: Vec<u16> = "open\0".encode_utf16().collect();
    let target: Vec<u16> = url.encode_utf16().chain(Some(0)).collect();
    unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            target.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        );
    }
}
