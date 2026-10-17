use std::process::ExitCode;

use rog_battery::{HidApi, Reading};

fn main() -> ExitCode {
    match std::env::args().nth(1).as_deref() {
        Some("--once") => print_once(),
        _ => {
            eprintln!("Usage: rog-battery-tray --once");
            ExitCode::FAILURE
        }
    }
}

/// Prints every detected device's battery and exits.
fn print_once() -> ExitCode {
    let api = match HidApi::new() {
        Ok(api) => api,
        Err(error) => {
            eprintln!("Cannot open the HID subsystem: {error}");
            return ExitCode::FAILURE;
        }
    };
    let devices = rog_battery::discover(&api);
    if devices.is_empty() {
        println!("No supported ROG device found.");
        return ExitCode::FAILURE;
    }
    for device in devices {
        let status = match device.read_battery(&api) {
            Ok(Reading::Battery { percent, charging }) => {
                let state = if charging { "charging" } else { "not charging" };
                format!("{percent}%, {state}")
            }
            Ok(Reading::Asleep) => "asleep or not responding".to_owned(),
            Err(error) => format!("query failed: {error}"),
        };
        println!("{} ({}): {status}", device.name(), device.connection());
    }
    ExitCode::SUCCESS
}
