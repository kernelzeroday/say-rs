use std::io::{self, IsTerminal, Write};
use std::time::Duration;

const GAP: usize = 2;
const CHAR_DELAY_MS: u64 = 3;

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
    pad_active: bool,
    is_tty: bool,
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
            pad_active: false,
            is_tty: io::stdout().is_terminal(),
        }
    }

    fn draw_bar(out: &mut io::Stdout, pct: f64) {
        let width = 30;
        let filled = (pct * width as f64) as usize;
        write!(
            out,
            "  \x1b[2m[\x1b[32m{}{}\x1b[0;2m] {:3.0}%\x1b[0m",
            "\u{2588}".repeat(filled),
            "\u{2591}".repeat(width - filled),
            pct * 100.0,
        )
        .ok();
    }

    pub fn on_word(&mut self, utf16_pos: usize, utf16_len: usize) {
        let (byte_start, byte_end) = self.map.to_byte_range(utf16_pos, utf16_len);

        if self.interactive && byte_start >= self.emitted_up_to {
            let mut out = io::stdout();

            if self.progress && self.pad_active && self.is_tty {
                write!(out, "\x1b[{}A", GAP + 1).ok();
            }

            let chunk: String = self.text[self.emitted_up_to..byte_end].to_string();
            for ch in chunk.chars() {
                write!(out, "{}", ch).ok();
                out.flush().ok();
                if !ch.is_whitespace() {
                    std::thread::sleep(Duration::from_millis(CHAR_DELAY_MS));
                }
            }
            self.emitted_up_to = byte_end;

            if self.progress && self.is_tty {
                let pct = if self.total_utf16 > 0 {
                    ((utf16_pos + utf16_len) as f64 / self.total_utf16 as f64).min(1.0)
                } else {
                    1.0
                };
                for _ in 0..GAP {
                    write!(out, "\n").ok();
                }
                write!(out, "\n\r\x1b[2K").ok();
                Self::draw_bar(&mut out, pct);
                out.flush().ok();
                self.pad_active = true;
            }
        } else if self.progress && !self.interactive && self.is_tty {
            let pct = if self.total_utf16 > 0 {
                ((utf16_pos + utf16_len) as f64 / self.total_utf16 as f64).min(1.0)
            } else {
                1.0
            };
            let mut out = io::stdout();
            write!(out, "\r\x1b[2K").ok();
            Self::draw_bar(&mut out, pct);
            out.flush().ok();
        }
    }

    pub fn finish(&mut self) {
        let mut out = io::stdout();

        if self.interactive {
            if self.progress && self.pad_active && self.is_tty {
                write!(out, "\x1b[{}A", GAP + 1).ok();
            }
            writeln!(out).ok();

            if self.progress && self.is_tty {
                for _ in 0..GAP {
                    writeln!(out).ok();
                }
                Self::draw_bar(&mut out, 1.0);
                writeln!(out).ok();
            }
        } else if self.progress && self.is_tty {
            write!(out, "\r\x1b[2K").ok();
            Self::draw_bar(&mut out, 1.0);
            writeln!(out).ok();
        }

        out.flush().ok();
    }
}
