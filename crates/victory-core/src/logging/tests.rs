use std::collections::BTreeSet;
use std::io::Write;
use std::sync::{Arc, Mutex};

use super::file::{append_crash_to, crash_line, open};
use super::*;

/// An in-memory destination standing in for the console or the log file.
#[derive(Clone, Default)]
struct Buf(Arc<Mutex<Vec<u8>>>);

impl Write for Buf {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Buf {
    fn text(&self) -> String {
        String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
    }
    fn lines(&self) -> Vec<serde_json::Value> {
        self.text().lines().map(|l| serde_json::from_str(l).unwrap()).collect()
    }
}

/// Runs `body` under a subscriber built from `spec` and returns what the
/// console and the log file received.
fn logged(spec: &Spec, body: impl FnOnce()) -> (Buf, Buf) {
    let (console, file) = (Buf::default(), Buf::default());
    let (c, f) = (console.clone(), file.clone());
    let subscriber = subscriber(spec, move || c.clone(), Some(move || f.clone()));
    tracing::subscriber::with_default(subscriber, body);
    (console, file)
}

fn keys(value: &serde_json::Value) -> BTreeSet<String> {
    value.as_object().unwrap().keys().cloned().collect()
}

#[test]
fn a_file_line_is_flat_json_with_utc_time_and_thread() {
    let spec = Spec::new(None, "info");
    let (_, file) = logged(&spec, || tracing::info!(hash = "ab12", stage = "Ps", "shader"));
    let lines = file.lines();
    assert_eq!(lines.len(), 1, "{}", file.text());
    let line = &lines[0];
    assert_eq!(line["message"], "shader");
    assert_eq!(line["hash"], "ab12");
    assert_eq!(line["level"], "INFO");
    assert!(line.get("fields").is_none(), "fields are flattened: {line}");
    assert!(line["threadId"].as_str().unwrap().starts_with("ThreadId("));
    let time = line["timestamp"].as_str().unwrap();
    assert!(time.ends_with('Z') && time.contains('T'), "{time}");
}

#[test]
fn the_console_stays_text() {
    let spec = Spec::new(None, "info");
    let (console, _) = logged(&spec, || tracing::info!("shown"));
    let text = console.text();
    assert!(text.contains("shown") && !text.trim_start().starts_with('{'), "{text}");
}

#[test]
fn summary_events_pass_a_warn_filter_in_both_sinks() {
    let spec = Spec::new(Some(Ok("warn".into())), "info");
    let (console, file) = logged(&spec, || {
        tracing::info!(target: SUMMARY, "run_start");
        tracing::info!("ordinary");
        tracing::warn!("trouble");
    });
    for text in [console.text(), file.text()] {
        assert!(text.contains("run_start"), "{text}");
        assert!(text.contains("trouble"), "{text}");
        assert!(!text.contains("ordinary"), "{text}");
    }
}

#[test]
fn other_events_follow_the_filter() {
    let spec = Spec::new(Some(Ok("info,noisy=warn".into())), "info");
    let (console, file) = logged(&spec, || {
        tracing::info!(target: "noisy", "hidden");
        tracing::warn!(target: "noisy", "shown");
        tracing::info!(target: "quiet", "kept");
    });
    for text in [console.text(), file.text()] {
        assert!(!text.contains("hidden") && text.contains("shown") && text.contains("kept"));
    }
}

#[test]
fn an_invalid_filter_falls_back_and_is_reported() {
    let spec = Spec::new(Some(Ok("victory=loud".into())), "info");
    assert!(spec.refused().is_some_and(|why| why.contains("victory=loud")));
    let (console, _) = logged(&spec, || tracing::info!("still logged"));
    assert!(console.text().contains("still logged"));
    let unreadable = Spec::new(Some(Err("not unicode".into())), "info");
    assert!(unreadable.refused().is_some());
    assert!(Spec::new(None, "info").refused().is_none());
}

#[test]
fn a_crash_line_has_the_keys_of_an_event_line() {
    let spec = Spec::new(None, "info");
    let (_, file) = logged(&spec, || tracing::error!(target: SUMMARY, report = "boom", "crash"));
    let event = &file.lines()[0];
    let crash: serde_json::Value = serde_json::from_str(&crash_line("boom")).unwrap();
    assert_eq!(keys(event), keys(&crash));
    for key in ["level", "target", "message", "report", "threadId"] {
        assert_eq!(event[key], crash[key], "{key}");
    }
}

#[test]
fn an_event_line_carries_its_fields() {
    let mut fields = serde_json::Map::new();
    fields.insert("console".into(), "series-x".into());
    let line: serde_json::Value =
        serde_json::from_str(&event_line("INFO", "run_start", fields)).unwrap();
    assert_eq!(line["message"], "run_start");
    assert_eq!(line["target"], SUMMARY);
    assert_eq!(line["console"], "series-x");
}

#[test]
fn a_crash_is_appended_after_the_lines_already_written() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("start.jsonl");
    let mut file = open(path.as_os_str()).unwrap();
    file.write_all(b"{\"message\":\"before\"}\n").unwrap();
    append_crash_to(&path, "boom").unwrap();
    file.write_all(b"{\"message\":\"after\"}\n").unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    let messages: Vec<String> = text
        .lines()
        .map(|l| serde_json::from_str::<serde_json::Value>(l).unwrap()["message"].to_string())
        .collect();
    assert_eq!(messages, ["\"before\"", "\"crash\"", "\"after\""]);
}

#[test]
fn append_crash_without_a_log_file_does_nothing() {
    // No `init` in this test binary, so there is no log file.
    assert!(append_crash("boom").is_ok());
}
