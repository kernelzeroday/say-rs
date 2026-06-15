use std::io::{self, IsTerminal, Write};
use std::time::Duration;

struct Utf16Map {
    utf16_to_byte: Vec<usize>,
}

impl Utf16Map {
    fn new(text: &str) -> Self {
        let mut map = Vec::new();
        for (byte_idx, ch) in text.char_indices() {
            for _ in 0..ch.len_utf16() {
                map.push(byte_idx);
            }
        }
        map.push(text.len());
        Self { utf16_to_byte: map }
    }

    fn to_byte_range(&self, utf16_pos: usize, utf16_len: usize) -> (usize, usize) {
        let start = self
            .utf16_to_byte
            .get(utf16_pos)
            .copied()
            .unwrap_or(self.utf16_to_byte.last().copied().unwrap_or(0));
        let end = self
            .utf16_to_byte
            .get(utf16_pos + utf16_len)
            .copied()
            .unwrap_or(self.utf16_to_byte.last().copied().unwrap_or(0));
        (start, end)
    }
}

pub struct Display {
    text: String,
    map: Utf16Map,
    interactive: bool,
    progress: bool,
    total_utf16: usize,
    emitted_up_to: usize,
    progress_visible: bool,
    stderr_is_tty: bool,
    last_pct: f64,
}

impl Display {
    pub fn new(text: &str, interactive: bool, progress: bool) -> Self {
        Self {
            text: text.to_string(),
            map: Utf16Map::new(text),
            interactive,
            progress,
            total_utf16: text.encode_utf16().count(),
            emitted_up_to: 0,
            progress_visible: false,
            stderr_is_tty: io::stderr().is_terminal(),
            last_pct: 0.0,
        }
    }

    fn draw_progress(&mut self, utf16_done: usize) {
        let pct = if self.total_utf16 > 0 {
            (utf16_done as f64 / self.total_utf16 as f64).min(1.0)
        } else {
            1.0
        };
        self.last_pct = pct;
        let width = 30;
        let filled = (pct * width as f64) as usize;
        let mut err = io::stderr();
        write!(
            err,
            "  \x1b[2m[\x1b[32m{}{}\x1b[0;2m] {:3.0}%\x1b[0m",
            "\u{2588}".repeat(filled),
            "\u{2591}".repeat(width - filled),
            pct * 100.0,
        )
        .ok();
        err.flush().ok();
        self.progress_visible = true;
    }

    fn clear_progress(&mut self) {
        if self.progress_visible {
            let mut err = io::stderr();
            write!(err, "\r\x1b[2K").ok();
            err.flush().ok();
            self.progress_visible = false;
        }
    }

    pub fn on_word(&mut self, utf16_pos: usize, utf16_len: usize) {
        let (byte_start, byte_end) = self.map.to_byte_range(utf16_pos, utf16_len);

        if self.interactive && byte_start >= self.emitted_up_to {
            self.clear_progress();

            let chunk: String = self.text[self.emitted_up_to..byte_end].to_string();
            let mut out = io::stdout();
            let utf16_done = utf16_pos + utf16_len;

            for ch in chunk.chars() {
                if ch == '\n' {
                    write!(out, "\n").ok();
                    out.flush().ok();
                    if self.progress && self.stderr_is_tty {
                        self.draw_progress(utf16_done);
                    }
                } else {
                    write!(out, "{}", ch).ok();
                    out.flush().ok();
                    if !ch.is_whitespace() {
                        std::thread::sleep(Duration::from_millis(8));
                    }
                }
            }

            self.emitted_up_to = byte_end;
        } else if self.progress && self.stderr_is_tty && !self.interactive {
            let utf16_done = utf16_pos + utf16_len;
            self.clear_progress();
            self.draw_progress(utf16_done);
        }
    }

    pub fn finish(&mut self) {
        self.clear_progress();

        if self.interactive {
            let mut out = io::stdout();
            writeln!(out).ok();
            out.flush().ok();
        }

        if self.progress && self.stderr_is_tty {
            let width = 30;
            let mut err = io::stderr();
            write!(
                err,
                "  \x1b[2m[\x1b[32m{}\x1b[0;2m] 100%\x1b[0m\n",
                "\u{2588}".repeat(width),
            )
            .ok();
            err.flush().ok();
        }
    }
}

impl Drop for Display {
    fn drop(&mut self) {
        if self.progress_visible {
            let mut err = io::stderr();
            write!(err, "\r\x1b[2K").ok();
            err.flush().ok();
        }
    }
}
