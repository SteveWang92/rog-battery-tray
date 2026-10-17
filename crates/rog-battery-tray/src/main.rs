use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::thread;
use std::time::{Duration, Instant};

use rog_battery::{Device, HidApi, HidError, Reading};

const USAGE: &str = "Usage: rog-battery-tray --once | --watch <seconds>";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.iter().map(String::as_str).collect::<Vec<_>>()[..] {
        ["--once"] => print_once(),
        ["--watch", seconds] => match seconds.parse::<u64>() {
            Ok(seconds) if seconds > 0 => watch(Duration::from_secs(seconds)),
            _ => usage(),
        },
        _ => usage(),
    }
}

fn usage() -> ExitCode {
    eprintln!("{USAGE}");
    ExitCode::FAILURE
}

fn open_hid() -> Option<HidApi> {
    HidApi::new()
        .inspect_err(|error| eprintln!("Cannot open the HID subsystem: {error}"))
        .ok()
}

fn describe(result: &Result<Reading, HidError>) -> String {
    match result {
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
    }
}

/// Prints every detected device's battery and exits.
fn print_once() -> ExitCode {
    let Some(api) = open_hid() else {
        return ExitCode::FAILURE;
    };
    let devices = rog_battery::discover(&api);
    if devices.is_empty() {
        println!("No supported ROG device found.");
        return ExitCode::FAILURE;
    }
    for device in devices {
        let status = describe(&device.read_battery(&api));
        println!("{} ({}): {status}", device.name(), device.connection());
    }
    ExitCode::SUCCESS
}

/// Queries every device at a fixed interval until stopped, printing each result and
/// appending it to a log file, for observing sleep behaviour and battery drain over time.
fn watch(interval: Duration) -> ExitCode {
    let Some(mut api) = open_hid() else {
        return ExitCode::FAILURE;
    };
    let Some(log_path) = log_path() else {
        eprintln!("LOCALAPPDATA is not set");
        return ExitCode::FAILURE;
    };
    let mut log = match open_log(&log_path) {
        Ok(log) => log,
        Err(error) => {
            eprintln!("Cannot open {}: {error}", log_path.display());
            return ExitCode::FAILURE;
        }
    };
    println!(
        "Logging every {}s to {} (Ctrl+C to stop)",
        interval.as_secs(),
        log_path.display()
    );
    let mut line = |text: String| {
        let text = format!(
            "{}  {text}",
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
        );
        println!("{text}");
        if let Err(error) = writeln!(log, "{text}") {
            eprintln!("Cannot write the log: {error}");
        }
    };
    line(format!("watch started, interval {}s", interval.as_secs()));

    // Discovery probes every candidate interface, so it runs again only when no device is
    // known or a query fails; otherwise each tick sends exactly one battery query.
    let mut devices: Vec<Device> = Vec::new();
    loop {
        if devices.is_empty() {
            if let Err(error) = api.refresh_devices() {
                line(format!("device refresh failed: {error}"));
            }
            devices = rog_battery::discover(&api);
            if devices.is_empty() {
                line("no supported ROG device found".to_owned());
            }
        }
        let mut failed = false;
        for device in &devices {
            let started = Instant::now();
            let result = device.read_battery(&api);
            let elapsed = started.elapsed().as_millis();
            failed |= result.is_err();
            line(format!(
                "{} ({}): {}  [{elapsed} ms]",
                device.name(),
                device.connection(),
                describe(&result)
            ));
        }
        if failed {
            devices.clear();
        }
        thread::sleep(interval);
    }
}

fn log_path() -> Option<PathBuf> {
    let base = PathBuf::from(std::env::var_os("LOCALAPPDATA")?);
    Some(base.join("rog-battery-tray").join("watch.log"))
}

fn open_log(path: &Path) -> std::io::Result<File> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    OpenOptions::new().create(true).append(true).open(path)
}
