//! Battery polling on a background thread, so a device that is slow to answer never blocks
//! the tray. Each round's results go to the tray as a [`Snapshot`] and, when logging is on,
//! to the log file.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant};

use chrono::{DateTime, Local};
use rog_battery::{Connection, Device, HidApi, Reading};

pub enum Command {
    Refresh,
    Interval(Duration),
    Logging(bool),
}

pub struct DeviceStatus {
    pub name: &'static str,
    pub connection: Connection,
    /// A failed query counts as not responding; its error goes to the log.
    pub reading: Reading,
}

pub struct Snapshot {
    pub devices: Vec<DeviceStatus>,
    pub at: DateTime<Local>,
}

/// The log file, in the per-user local application data folder.
pub fn log_path() -> Option<PathBuf> {
    let base = PathBuf::from(std::env::var_os("LOCALAPPDATA")?);
    Some(base.join("rog-battery-tray").join("battery.log"))
}

/// Starts polling. `notify` runs on the polling thread after each snapshot is sent, to wake
/// the tray's message loop. The thread ends when the returned sender is dropped.
pub fn spawn(
    api: HidApi,
    interval: Duration,
    logging: bool,
    snapshots: Sender<Snapshot>,
    notify: impl Fn() + Send + 'static,
) -> Sender<Command> {
    let (commands, inbox) = mpsc::channel();
    thread::spawn(move || {
        Poller {
            api,
            devices: Vec::new(),
            fingerprint: Vec::new(),
            log: None,
        }
        .run(interval, logging, inbox, snapshots, notify)
    });
    commands
}

struct Poller {
    api: HidApi,
    devices: Vec<Device>,
    /// The ASUS HID paths present at the last discovery.
    fingerprint: Vec<String>,
    log: Option<File>,
}

impl Poller {
    fn run(
        mut self,
        mut interval: Duration,
        logging: bool,
        inbox: Receiver<Command>,
        snapshots: Sender<Snapshot>,
        notify: impl Fn(),
    ) {
        self.set_logging(logging);
        loop {
            let polled = Instant::now();
            if snapshots.send(self.poll()).is_err() {
                return;
            }
            notify();
            loop {
                let wait = (polled + interval).saturating_duration_since(Instant::now());
                match inbox.recv_timeout(wait) {
                    Ok(Command::Refresh) | Err(RecvTimeoutError::Timeout) => break,
                    Ok(Command::Interval(new)) => interval = new,
                    Ok(Command::Logging(on)) => self.set_logging(on),
                    Err(RecvTimeoutError::Disconnected) => return,
                }
            }
        }
    }

    fn poll(&mut self) -> Snapshot {
        // Discovery probes every candidate interface, so it runs only when the set of ASUS
        // HID interfaces changes or a query failed; otherwise a round sends exactly one
        // battery query per device.
        if let Err(error) = self.api.refresh_devices() {
            self.log_line(&format!("device refresh failed: {error}"));
        }
        let mut fingerprint: Vec<String> = self
            .api
            .device_list()
            .filter(|info| info.vendor_id() == 0x0B05)
            .map(|info| info.path().to_string_lossy().into_owned())
            .collect();
        fingerprint.sort();
        if self.devices.is_empty() || fingerprint != self.fingerprint {
            self.devices = rog_battery::discover(&self.api);
            self.fingerprint = fingerprint;
            if self.devices.is_empty() {
                self.log_line("no supported ROG device found");
            }
        }

        let mut failed = false;
        let mut devices = Vec::with_capacity(self.devices.len());
        for i in 0..self.devices.len() {
            let device = &self.devices[i];
            let started = Instant::now();
            let result = device.read_battery(&self.api);
            let elapsed = started.elapsed().as_millis();
            let (name, connection) = (device.name(), device.connection());
            let text = match &result {
                Ok(Reading::Battery { percent, charging }) => {
                    let state = if *charging {
                        "charging"
                    } else {
                        "not charging"
                    };
                    format!("{percent}%, {state}")
                }
                Ok(Reading::Asleep) => "asleep or not responding".to_owned(),
                Err(error) => format!("query failed: {error}"),
            };
            self.log_line(&format!("{name} ({connection}): {text}  [{elapsed} ms]"));
            failed |= result.is_err();
            devices.push(DeviceStatus {
                name,
                connection,
                reading: result.unwrap_or(Reading::Asleep),
            });
        }
        if failed {
            self.devices.clear();
        }
        Snapshot {
            devices,
            at: Local::now(),
        }
    }

    fn set_logging(&mut self, on: bool) {
        self.log = if on { open_log() } else { None };
        if on {
            self.log_line("logging started");
        }
    }

    fn log_line(&mut self, text: &str) {
        if let Some(log) = &mut self.log {
            let stamp = Local::now().format("%Y-%m-%d %H:%M:%S");
            // A failed write loses one line; the next round tries again.
            let _ = writeln!(log, "{stamp}  {text}");
        }
    }
}

fn open_log() -> Option<File> {
    let path = log_path()?;
    fs::create_dir_all(path.parent()?).ok()?;
    OpenOptions::new().create(true).append(true).open(path).ok()
}
