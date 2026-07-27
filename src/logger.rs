//! Minimal file logger. The exe is a windows-subsystem binary (no console),
//! so verbose output goes to %TEMP%\superpp.log.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::sync::Mutex;

static LOG: Mutex<Option<File>> = Mutex::new(None);

pub fn init() {
    let path = std::env::temp_dir().join("superpp.log");
    if let Ok(f) = OpenOptions::new().create(true).append(true).open(&path) {
        *LOG.lock().unwrap() = Some(f);
    }
}

pub fn log(msg: &str) {
    if let Ok(mut guard) = LOG.lock() {
        if let Some(f) = guard.as_mut() {
            let _ = writeln!(f, "{msg}");
        }
    }
}
