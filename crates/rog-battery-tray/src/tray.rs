//! The tray application: one icon per device (or one merged icon), its tooltip, and the
//! right-click menu that holds every setting.

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

use crate::icon::{self, Face};
use crate::poller::{self, Command, DeviceStatus, Snapshot};
use crate::theme::{self, Look};

const PROJECT_URL: &str = "https://github.com/SteveWang92/rog-battery-tray";
const SUPPORTED_URL: &str = "https://github.com/SteveWang92/rog-battery-tray#supported-devices";

/// How often the taskbar's theme and DPI are checked for a change.
const LOOK_CHECK_MS: u32 = 2000;

const THRESHOLDS: [Option<u8>; 4] = [None, Some(10), Some(20), Some(30)];
const INTERVALS: [u64; 3] = [30, 60, 300];

struct Settings {
    low_threshold: Option<u8>,
    interval: Duration,
    merged: bool,
    logging: bool,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            low_threshold: Some(20),
            interval: Duration::from_secs(60),
            merged: false,
            logging: false,
        }
    }
}

struct Tray {
    settings: Settings,
    commands: mpsc::Sender<Command>,
    snapshot: Option<Snapshot>,
    icons: Vec<TrayIcon>,
    look: Look,
}

pub fn run(api: HidApi) {
    let look = Look::current();
    theme::apply_menu_theme(look.light);
    let settings = Settings::default();

    let main_thread = unsafe { GetCurrentThreadId() };
    let mut msg: MSG = unsafe { std::mem::zeroed() };
    // Creates this thread's message queue before the poller first posts to it.
    unsafe { PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_NOREMOVE) };

    let (snapshots, inbox) = mpsc::channel();
    let commands = poller::spawn(
        api,
        settings.interval,
        settings.logging,
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
    /// Applies a menu click; returns whether the tray must be redrawn.
    fn on_menu(&mut self, id: &str) -> bool {
        let (kind, value) = id.split_once(':').unwrap_or((id, ""));
        match kind {
            "refresh" => {
                let _ = self.commands.send(Command::Refresh);
                false
            }
            "low" => {
                self.settings.low_threshold = value.parse().ok();
                true
            }
            "interval" => {
                let seconds = value.parse().expect("interval ids carry seconds");
                self.settings.interval = Duration::from_secs(seconds);
                let _ = self
                    .commands
                    .send(Command::Interval(self.settings.interval));
                true
            }
            "icons" => {
                self.settings.merged = value == "merged";
                true
            }
            "log" => {
                self.settings.logging = !self.settings.logging;
                let _ = self.commands.send(Command::Logging(self.settings.logging));
                true
            }
            "about" => {
                open_url(PROJECT_URL);
                false
            }
            "supported" => {
                open_url(SUPPORTED_URL);
                false
            }
            "quit" => {
                self.icons.clear();
                unsafe { PostQuitMessage(0) };
                false
            }
            _ => false,
        }
    }

    fn redraw(&mut self) {
        let Some(snapshot) = &self.snapshot else {
            return;
        };
        let menu = self.menu(snapshot);
        let time = snapshot.at.format("%H:%M");

        // Each icon: what it shows and the devices its tooltip lists.
        let faces: Vec<(Face, Vec<&DeviceStatus>)> = if snapshot.devices.is_empty() {
            vec![(Face::NoDevice, Vec::new())]
        } else if self.settings.merged {
            let lowest = snapshot
                .devices
                .iter()
                .filter(|d| matches!(d.reading, Reading::Battery { .. }))
                .min_by_key(|d| match d.reading {
                    Reading::Battery { percent, .. } => percent,
                    Reading::Asleep => u8::MAX,
                });
            let face = lowest.map_or(Face::Asleep, |d| self.face(d.reading));
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
                "未找到 ROG 设备".to_owned()
            } else {
                let lines: Vec<String> = devices
                    .iter()
                    .map(|d| format!("{}\n{}", device_label(d), status_line(d.reading)))
                    .collect();
                format!("{}\n{time} 更新", lines.join("\n"))
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
                        // the next snapshot tries again.
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
                low: self.settings.low_threshold.is_some_and(|t| percent <= t),
            },
            Reading::Asleep => Face::Asleep,
        }
    }

    fn menu(&self, snapshot: &Snapshot) -> Menu {
        let menu = Menu::new();
        let quit = MenuItem::with_id("quit", "退出", true, None);
        if snapshot.devices.is_empty() {
            append(
                &menu,
                &[
                    &MenuItem::new("未找到 ROG 设备", false, None),
                    &MenuItem::with_id("supported", "查看支持的设备…", true, None),
                    &PredefinedMenuItem::separator(),
                    &quit,
                ],
            );
            return menu;
        }

        for (i, device) in snapshot.devices.iter().enumerate() {
            let right = match device.reading {
                Reading::Battery {
                    percent,
                    charging: true,
                } => format!("{percent}% 充电中"),
                Reading::Battery { percent, .. } => format!("{percent}%"),
                Reading::Asleep => "未响应".to_owned(),
            };
            let row = format!("{}\t{right}", device_label(device));
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
            .map(|&t| {
                let text = t.map_or("关闭".to_owned(), |t| format!("{t}%"));
                let id = format!("low:{}", t.map_or(String::new(), |t| t.to_string()));
                radio(id, &text, s.low_threshold == t)
            })
            .collect();
        let intervals: Vec<CheckMenuItem> = INTERVALS
            .iter()
            .map(|&secs| {
                let text = if secs < 60 {
                    format!("{secs} 秒")
                } else {
                    format!("{} 分钟", secs / 60)
                };
                radio(
                    format!("interval:{secs}"),
                    &text,
                    s.interval.as_secs() == secs,
                )
            })
            .collect();
        let icons = [
            radio("icons:each".to_owned(), "每个设备一个", !s.merged),
            radio(
                "icons:merged".to_owned(),
                "合并成一个（显示最低）",
                s.merged,
            ),
        ];
        let version = env!("CARGO_PKG_VERSION");

        append(
            &menu,
            &[
                &PredefinedMenuItem::separator(),
                &MenuItem::with_id("refresh", "立即刷新", true, None),
                &PredefinedMenuItem::separator(),
                &submenu("低电量提醒", &low),
                &submenu("刷新间隔", &intervals),
                &submenu("托盘图标", &icons),
                &CheckMenuItem::with_id("log", "记录电量日志", true, s.logging, None),
                &PredefinedMenuItem::separator(),
                &MenuItem::with_id(
                    "about",
                    format!("关于 rog-battery-tray\tv{version}"),
                    true,
                    None,
                ),
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

fn device_label(device: &DeviceStatus) -> String {
    let connection = match device.connection {
        Connection::OmniReceiver => "OMNI",
        Connection::Receiver => "接收器",
        Connection::Wired => "有线",
    };
    format!("{} ({connection})", device.name)
}

fn status_line(reading: Reading) -> String {
    match reading {
        Reading::Battery { percent, charging } => {
            let state = if charging { "充电中" } else { "未充电" };
            format!("电量 {percent}% · {state}")
        }
        Reading::Asleep => "休眠或未响应".to_owned(),
    }
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
