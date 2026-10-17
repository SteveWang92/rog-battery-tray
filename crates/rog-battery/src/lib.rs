//! Battery queries for ASUS ROG wireless peripherals over HID.
//!
//! Only query commands are ever sent; nothing here writes a device setting.

use std::ffi::CString;
use std::fmt;
use std::time::{Duration, Instant};

pub use hidapi::{HidApi, HidError};

const ASUS_VID: u16 = 0x0B05;

/// How long a device gets to answer one command before it counts as not responding.
const REPLY_TIMEOUT: Duration = Duration::from_millis(500);

const CMD_GET: u8 = 0x12;
const SUB_PROBE: u8 = 0x00;
const SUB_BATTERY: u8 = 0x01;

/// How a device reaches the computer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Connection {
    OmniReceiver,
    Receiver,
    Wired,
}

impl fmt::Display for Connection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Connection::OmniReceiver => "OMNI receiver",
            Connection::Receiver => "2.4 GHz receiver",
            Connection::Wired => "wired",
        })
    }
}

/// One supported USB product and the HID interface that carries its control channel.
struct Model {
    pid: u16,
    name: &'static str,
    connection: Connection,
    interface: i32,
    /// The vendor usage page of the control collection, where the interface has several.
    usage_page: Option<u16>,
    report_id: u8,
    /// Output report length including the report ID byte.
    report_len: usize,
}

const MODELS: &[Model] = &[
    Model {
        pid: 0x1ACE,
        name: "ROG Azoth",
        connection: Connection::OmniReceiver,
        interface: 2,
        usage_page: Some(0xFF00),
        report_id: 0x02,
        report_len: 64,
    },
    Model {
        pid: 0x1A85,
        name: "ROG Azoth",
        connection: Connection::Receiver,
        interface: 1,
        usage_page: None,
        report_id: 0x00,
        report_len: 65,
    },
    Model {
        pid: 0x1A83,
        name: "ROG Azoth",
        connection: Connection::Wired,
        interface: 1,
        usage_page: None,
        report_id: 0x00,
        report_len: 65,
    },
];

/// A device whose control interface answered the probe.
pub struct Device {
    model: &'static Model,
    path: CString,
}

/// The result of one battery query.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reading {
    Battery {
        percent: u8,
        charging: bool,
    },
    /// The receiver or device gave no answer: the device is asleep, off or out of range.
    Asleep,
}

impl Device {
    pub fn name(&self) -> &'static str {
        self.model.name
    }

    pub fn connection(&self) -> Connection {
        self.model.connection
    }

    pub fn read_battery(&self, api: &HidApi) -> Result<Reading, HidError> {
        let reply = match query(api, self.model, &self.path, SUB_BATTERY)? {
            Some(reply) if !is_no_answer(&reply) => reply,
            _ => return Ok(Reading::Asleep),
        };
        Ok(Reading::Battery {
            percent: reply[6],
            charging: reply[9] == 1,
        })
    }
}

/// Finds every connected supported device by probing the candidate HID interfaces.
pub fn discover(api: &HidApi) -> Vec<Device> {
    api.device_list()
        .filter(|info| info.vendor_id() == ASUS_VID)
        .filter_map(|info| {
            let model = MODELS.iter().find(|m| {
                m.pid == info.product_id()
                    && m.interface == info.interface_number()
                    && m.usage_page.is_none_or(|page| page == info.usage_page())
            })?;
            let path = info.path().to_owned();
            // An interface that fails to open or answers with the wrong shape is not the
            // control channel; the right one sits beside it in the same enumeration.
            let reply = query(api, model, &path, SUB_PROBE).ok()??;
            let found = is_no_answer(&reply) || reply[5] != 0xFA;
            found.then_some(Device { model, path })
        })
        .collect()
}

/// A receiver's reply when the device behind it did not answer.
fn is_no_answer(reply: &[u8]) -> bool {
    reply[1] == 0xFF && reply[2] == 0xAA
}

/// Sends `[report ID, 0x12, sub]` and waits for its reply, returned with the report ID at
/// index 0. `None` means nothing matching arrived in time.
fn query(
    api: &HidApi,
    model: &Model,
    path: &CString,
    sub: u8,
) -> Result<Option<Vec<u8>>, HidError> {
    let device = api.open_path(path)?;
    let mut packet = vec![0u8; model.report_len];
    packet[0] = model.report_id;
    packet[1] = CMD_GET;
    packet[2] = sub;
    device.write(&packet)?;

    // hidapi drops the report ID from unnumbered (ID 0) reports, so the buffer starts one
    // byte in for them and every reply keeps the same offsets.
    let offset = usize::from(model.report_id == 0);
    let mut buf = vec![0u8; model.report_len];
    let deadline = Instant::now() + REPLY_TIMEOUT;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Ok(None);
        }
        let n = device.read_timeout(&mut buf[offset..], left.as_millis() as i32)?;
        if n == 0 {
            return Ok(None);
        }
        // Other input reports can share the interface; skip anything that is not this
        // command's reply.
        let reply = &buf[..offset + n];
        if reply.len() >= 10 && (reply[1] == CMD_GET && reply[2] == sub || is_no_answer(reply)) {
            return Ok(Some(reply.to_vec()));
        }
    }
}
