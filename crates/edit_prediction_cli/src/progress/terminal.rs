use std::{borrow::Cow, io::Write, time::Duration};

use log::{Level, Log, Metadata, Record};

use super::GLOBAL;

pub(super) struct ProgressLogger;

impl Log for ProgressLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= Level::Info
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }

        let level_color = match record.level() {
            Level::Error => "\x1b[31m",
            Level::Warn => "\x1b[33m",
            Level::Info => "\x1b[32m",
            Level::Debug => "\x1b[34m",
            Level::Trace => "\x1b[35m",
        };
        let reset = "\x1b[0m";
        let bold = "\x1b[1m";

        let level_label = match record.level() {
            Level::Error => "Error",
            Level::Warn => "Warn",
            Level::Info => "Info",
            Level::Debug => "Debug",
            Level::Trace => "Trace",
        };

        let message = format!(
            "{bold}{level_color}{level_label:>12}{reset} {}",
            record.args()
        );

        if let Some(progress) = GLOBAL.get() {
            progress.log(&message);
        } else {
            eprintln!("{}", message);
        }
    }

    fn flush(&self) {
        let _ = std::io::stderr().flush();
    }
}

#[cfg(unix)]
pub(super) fn get_terminal_width() -> usize {
    unsafe {
        let mut winsize: libc::winsize = std::mem::zeroed();
        if libc::ioctl(libc::STDERR_FILENO, libc::TIOCGWINSZ, &mut winsize) == 0
            && winsize.ws_col > 0
        {
            winsize.ws_col as usize
        } else {
            80
        }
    }
}

#[cfg(not(unix))]
pub(super) fn get_terminal_width() -> usize {
    80
}

pub(super) fn strip_ansi_len(s: &str) -> usize {
    let mut len = 0;
    let mut in_escape = false;
    for c in s.chars() {
        if c == '\x1b' {
            in_escape = true;
        } else if in_escape {
            if c == 'm' {
                in_escape = false;
            }
        } else {
            len += 1;
        }
    }
    len
}

pub(super) fn truncate_with_ellipsis(s: &str, max_len: usize) -> Cow<'_, str> {
    if s.len() <= max_len {
        Cow::Borrowed(s)
    } else {
        Cow::Owned(format!("{}…", &s[..max_len.saturating_sub(1)]))
    }
}

pub(super) fn truncate_to_visible_width(s: &str, max_visible_len: usize) -> &str {
    let mut visible_len = 0;
    let mut in_escape = false;
    let mut last_byte_index = 0;
    for (byte_index, c) in s.char_indices() {
        if c == '\x1b' {
            in_escape = true;
        } else if in_escape {
            if c == 'm' {
                in_escape = false;
            }
        } else {
            if visible_len >= max_visible_len {
                return &s[..last_byte_index];
            }
            visible_len += 1;
        }
        last_byte_index = byte_index + c.len_utf8();
    }
    s
}

pub(super) fn format_duration(duration: Duration) -> String {
    const MINUTE_IN_MILLIS: f32 = 60. * 1000.;

    let millis = duration.as_millis() as f32;
    if millis < 1000.0 {
        format!("{}ms", millis)
    } else if millis < MINUTE_IN_MILLIS {
        format!("{:.1}s", millis / 1_000.0)
    } else {
        format!("{:.1}m", millis / MINUTE_IN_MILLIS)
    }
}
