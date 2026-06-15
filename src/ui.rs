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

fn terminal_height() -> u16 {
    unsafe {
        let mut ws: libc::winsize = std::mem::zeroed();
        if libc::ioctl(libc::STDOUT_FILENO, libc::TIOCGWINSZ, &mut ws) == 0 && ws.ws_row > 0 {
            ws.ws_row
        } else {
            0
        }
    }
}

pub struct Display {
    text: String,
    map: Utf16Map,
    interactive: bool,
    #[allow(dead_code)]
    progress: bool,
    total_utf16: usize,
    emitted_up_to: usize,
    term_height: u16,
    use_scroll_region: bool,
}

impl Display {
    pub fn new(text: &str, interactive: bool, progress: bool) -> Self {
        let is_tty = io::stdout().is_terminal();
        let term_height = if is_tty { terminal_height() } else { 0 };
        let use_scroll_region = progress && term_height > 2 && is_tty;

        if use_scroll_region {
            let mut out = io::stdout();
            // Set scroll region to all rows except the last
            write!(out, "\x1b[1;{}r", term_height - 1).ok();
            // Draw empty progress bar on bottom line
            write!(
                out,
                "\x1b7\x1b[{};1H\x1b[2K  [\x1b[32m{}\x1b[0m]   0%\x1b8",
                term_height,
                "\u{2591}".repeat(30),
            )
            .ok();
            out.flush().ok();
        }

        Self {
            text: text.to_string(),
            map: Utf16Map::new(text),
            interactive,
            progress,
            total_utf16: text.encode_utf16().count(),
            emitted_up_to: 0,
            term_height,
            use_scroll_region,
        }
    }

    pub fn on_word(&mut self, utf16_pos: usize, utf16_len: usize) {
        let (byte_start, byte_end) = self.map.to_byte_range(utf16_pos, utf16_len);

        if self.interactive && byte_start >= self.emitted_up_to {
            let chunk = &self.text[self.emitted_up_to..byte_end];
            let mut out = io::stdout();
            for ch in chunk.chars() {
                write!(out, "{}", ch).ok();
                out.flush().ok();
                if !ch.is_whitespace() {
                    std::thread::sleep(Duration::from_millis(8));
                }
            }
            self.emitted_up_to = byte_end;
        }

        if self.use_scroll_region {
            let chars_done = utf16_pos + utf16_len;
            let pct = if self.total_utf16 > 0 {
                (chars_done as f64 / self.total_utf16 as f64).min(1.0)
            } else {
                1.0
            };
            let width = 30;
            let filled = (pct * width as f64) as usize;
            let mut out = io::stdout();
            write!(
                out,
                "\x1b7\x1b[{};1H\x1b[2K  [\x1b[32m{}{}\x1b[0m] {:3.0}%\x1b8",
                self.term_height,
                "\u{2588}".repeat(filled),
                "\u{2591}".repeat(width - filled),
                pct * 100.0,
            )
            .ok();
            out.flush().ok();
        }
    }

    pub fn finish(&mut self) {
        let mut out = io::stdout();

        if self.use_scroll_region {
            // Clear the progress bar line and reset scroll region
            write!(
                out,
                "\x1b7\x1b[{};1H\x1b[2K\x1b8\x1b[r",
                self.term_height,
            )
            .ok();
        }

        if self.interactive {
            writeln!(out).ok();
        }

        out.flush().ok();
    }
}

impl Drop for Display {
    fn drop(&mut self) {
        if self.use_scroll_region {
            let mut out = io::stdout();
            // Reset scroll region in case of early exit
            write!(out, "\x1b[r").ok();
            out.flush().ok();
        }
    }
}
