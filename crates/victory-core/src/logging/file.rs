//! The log file: opening it, lines written beside the subscriber (the
//! start, before logging is up; a crash, while the subscriber may be held
//! by the crashing thread).

use std::ffi::OsStr;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use tracing_subscriber::fmt::format::Writer;
use tracing_subscriber::fmt::time::{FormatTime, SystemTime};

use super::SUMMARY;

/// This process's log file, once [`super::init`] opened it.
static LOG_FILE: OnceLock<PathBuf> = OnceLock::new();

pub(super) fn remember(path: PathBuf) {
    // `init` runs once per process; a second keeps the first file.
    if LOG_FILE.set(path).is_err() {
        tracing::warn!("a second log file was opened; crashes go to the first");
    }
}

/// Opens `path` to append: every write lands at the end, even with
/// [`append_crash`] writing through another handle. (Opened again right
/// after it was made: a virus scanner may hold a new file for a moment, so
/// a refusal is tried again a few times.)
pub(super) fn open(path: &OsStr) -> std::io::Result<File> {
    let mut tries = 0;
    loop {
        match OpenOptions::new().create(true).append(true).open(path) {
            Ok(file) => return Ok(file),
            Err(_) if tries < 20 => {
                tries += 1;
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            Err(e) => return Err(e),
        }
    }
}

/// The time as the JSON lines write it (RFC 3339, UTC).
fn now_rfc3339() -> String {
    let mut text = String::new();
    if SystemTime.format_time(&mut Writer::new(&mut text)).is_err() {
        text.clear();
    }
    text
}

/// A [`SUMMARY`] event as a line of the log file, with the keys the
/// subscriber writes and `fields` beside them; for what is written while
/// the subscriber cannot be used.
pub fn event_line(
    level: &str,
    message: &str,
    fields: serde_json::Map<String, serde_json::Value>,
) -> String {
    let thread = std::thread::current();
    let mut line = serde_json::Map::new();
    line.insert("timestamp".into(), now_rfc3339().into());
    line.insert("level".into(), level.into());
    line.insert("target".into(), SUMMARY.into());
    line.insert("threadId".into(), format!("{:?}", thread.id()).into());
    if let Some(name) = thread.name() {
        line.insert("threadName".into(), name.into());
    }
    line.insert("message".into(), message.into());
    line.extend(fields);
    serde_json::Value::Object(line).to_string()
}

/// A crash as a line of the log file.
pub(super) fn crash_line(report: &str) -> String {
    let mut fields = serde_json::Map::new();
    fields.insert("report".into(), report.into());
    event_line("ERROR", "crash", fields)
}

/// Appends `report` to this process's log file, if it has one. It opens
/// the file anew rather than going through the subscriber, which the
/// crashing thread may be in the middle of.
pub fn append_crash(report: &str) -> std::io::Result<()> {
    match LOG_FILE.get() {
        Some(path) => append_crash_to(path, report),
        None => Ok(()),
    }
}

pub(super) fn append_crash_to(path: &Path, report: &str) -> std::io::Result<()> {
    let mut line = crash_line(report);
    line.push('\n');
    OpenOptions::new().append(true).open(path)?.write_all(line.as_bytes())
}
