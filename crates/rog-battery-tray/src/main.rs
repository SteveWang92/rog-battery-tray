// A GUI-subsystem binary, so launching the tray opens no console window.
#![windows_subsystem = "windows"]

mod app_icon;
mod config;
mod icon;
mod poller;
mod strings;
mod system;
mod theme;
mod tray;

use std::process::ExitCode;

use rog_battery::{HidApi, Reading};
use windows_sys::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError};
use windows_sys::Win32::System::Console::{ATTACH_PARENT_PROCESS, AttachConsole};
use windows_sys::Win32::System::Threading::CreateMutexW;
use windows_sys::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
};

const USAGE: &str = "Usage: rog-battery-tray [--once]";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.iter().map(String::as_str).collect::<Vec<_>>()[..] {
        [] => run_tray(),
        ["--once"] => {
            attach_console();
            print_once()
        }
        _ => {
            attach_console();
            eprintln!("{USAGE}");
            ExitCode::FAILURE
        }
    }
}

/// Lets command-line output reach the terminal that started this GUI-subsystem binary.
fn attach_console() {
    unsafe { AttachConsole(ATTACH_PARENT_PROCESS) };
}

fn run_tray() -> ExitCode {
    let name: Vec<u16> = "Local\\rog-battery-tray\0".encode_utf16().collect();
    // The handle stays open for the life of the process; a second launch finds it and exits.
    let mutex = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
    if !mutex.is_null() && unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        return ExitCode::SUCCESS;
    }
    unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    let Ok(api) = HidApi::new() else {
        return ExitCode::FAILURE;
    };
    tray::run(api);
    ExitCode::SUCCESS
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
