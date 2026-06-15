use std::io::{self, Write};

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
    lines: usize,
    started: bool,
}

impl Display {
    pub fn new(text: &str, interactive: bool, progress: bool) -> Self {
        let mut out = io::stdout();
        if interactive {
            write!(out, "\x1b[?25l").ok();
            out.flush().ok();
        }
        let lines = interactive as usize + progress as usize;
        Self {
            text: text.to_string(),
            map: Utf16Map::new(text),
            interactive,
            progress,
            total_utf16: text.encode_utf16().count(),
            lines,
            started: false,
        }
    }

    pub fn on_word(&mut self, utf16_pos: usize, utf16_len: usize) {
        let mut out = io::stdout();

        if self.started && self.lines > 0 {
            write!(out, "\x1b[{}A\r", self.lines).ok();
        }

        if self.interactive {
            let (byte_start, byte_end) = self.map.to_byte_range(utf16_pos, utf16_len);
            let before = &self.text[..byte_start];
            let word = &self.text[byte_start..byte_end];
            let after = &self.text[byte_end..];
            write!(
                out,
                "\x1b[2K{}\x1b[7m{}\x1b[0m{}\n",
                before, word, after
            )
            .ok();
        }

        if self.progress {
            let chars_done = utf16_pos + utf16_len;
            let pct = if self.total_utf16 > 0 {
                (chars_done as f64 / self.total_utf16 as f64).min(1.0)
            } else {
                1.0
            };
            let width = 30;
            let filled = (pct * width as f64) as usize;
            write!(
                out,
                "\x1b[2K  [\x1b[32m{}{}\x1b[0m] {:3.0}%\n",
                "\u{2588}".repeat(filled),
                "\u{2591}".repeat(width - filled),
                pct * 100.0,
            )
            .ok();
        }

        out.flush().ok();
        self.started = true;
    }

    pub fn finish(&mut self) {
        let mut out = io::stdout();

        if self.started && self.lines > 0 {
            write!(out, "\x1b[{}A\r", self.lines).ok();
        }

        if self.interactive {
            write!(out, "\x1b[2K{}\n", self.text).ok();
        }

        if self.progress {
            let width = 30;
            write!(
                out,
                "\x1b[2K  [\x1b[32m{}\x1b[0m] 100%\n",
                "\u{2588}".repeat(width),
            )
            .ok();
        }

        if self.interactive {
            write!(out, "\x1b[?25h").ok();
        }

        out.flush().ok();
    }
}

impl Drop for Display {
    fn drop(&mut self) {
        if self.interactive {
            let mut out = io::stdout();
            write!(out, "\x1b[?25h").ok();
            out.flush().ok();
        }
    }
}
