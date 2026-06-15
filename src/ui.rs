use std::io::{self, IsTerminal, Write};
use std::time::Duration;

const GAP: usize = 2;

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

fn terminal_width() -> usize {
    unsafe {
        let mut ws: libc::winsize = std::mem::zeroed();
        if libc::ioctl(libc::STDOUT_FILENO, libc::TIOCGWINSZ, &mut ws) == 0 && ws.ws_col > 0 {
            ws.ws_col as usize
        } else {
            80
        }
    }
}

pub struct Display {
    text: String,
    map: Utf16Map,
    interactive: bool,
    progress: bool,
    total_utf16: usize,
    emitted_up_to: usize,
    col: usize,
    visual_rows_since_pad: usize,
    term_width: usize,
    pad_drawn: bool,
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
            col: 0,
            visual_rows_since_pad: 0,
            term_width: terminal_width(),
            pad_drawn: false,
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

    fn pct(&self, utf16_done: usize) -> f64 {
        if self.total_utf16 > 0 {
            (utf16_done as f64 / self.total_utf16 as f64).min(1.0)
        } else {
            1.0
        }
    }

    fn create_pad(&mut self, out: &mut io::Stdout, pct: f64) {
        for _ in 0..GAP {
            writeln!(out).ok();
        }
        writeln!(out).ok();
        write!(out, "\x1b[2K").ok();
        Self::draw_bar(out, pct);
        out.flush().ok();
        write!(out, "\x1b[{}A\x1b[{}G", GAP + 1, self.col + 1).ok();
        self.pad_drawn = true;
        self.visual_rows_since_pad = 0;
    }

    fn update_bar_inplace(&mut self, out: &mut io::Stdout, pct: f64) {
        let dist = GAP + 1 + self.visual_rows_since_pad;
        write!(out, "\x1b[{}B\r\x1b[2K", dist).ok();
        Self::draw_bar(out, pct);
        write!(out, "\x1b[{}A\x1b[{}G", dist, self.col + 1).ok();
        out.flush().ok();
        self.visual_rows_since_pad = 0;
    }

    fn erase_pad(&mut self, out: &mut io::Stdout) {
        if self.pad_drawn {
            write!(out, "\x1b[J").ok();
            self.pad_drawn = false;
            self.visual_rows_since_pad = 0;
        }
    }

    fn emit_char(&mut self, ch: char) {
        if ch == '\n' {
            self.col = 0;
            self.visual_rows_since_pad += 1;
        } else {
            self.col += 1;
            if self.term_width > 0 && self.col >= self.term_width {
                self.col = 0;
                self.visual_rows_since_pad += 1;
            }
        }
    }

    pub fn on_word(&mut self, utf16_pos: usize, utf16_len: usize) {
        let (byte_start, byte_end) = self.map.to_byte_range(utf16_pos, utf16_len);

        if self.interactive && byte_start >= self.emitted_up_to {
            let mut out = io::stdout();
            let show_progress = self.progress && self.is_tty;

            let chunk: String = self.text[self.emitted_up_to..byte_end].to_string();

            for ch in chunk.chars() {
                write!(out, "{}", ch).ok();
                out.flush().ok();
                self.emit_char(ch);
                if !ch.is_whitespace() {
                    std::thread::sleep(Duration::from_millis(2));
                }
            }
            self.emitted_up_to = byte_end;

            if show_progress {
                let pct = self.pct(utf16_pos + utf16_len);
                if !self.pad_drawn {
                    self.create_pad(&mut out, pct);
                } else if self.visual_rows_since_pad > 0 {
                    self.erase_pad(&mut out);
                    self.create_pad(&mut out, pct);
                } else {
                    self.update_bar_inplace(&mut out, pct);
                }
            }
        } else if self.progress && !self.interactive && self.is_tty {
            let mut out = io::stdout();
            write!(out, "\r\x1b[2K").ok();
            Self::draw_bar(&mut out, self.pct(utf16_pos + utf16_len));
            out.flush().ok();
        }
    }

    pub fn finish(&mut self) {
        let mut out = io::stdout();

        if self.interactive {
            if self.pad_drawn {
                self.erase_pad(&mut out);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vterm::VTerm;

    #[test]
    fn same_line_no_overwrite() {
        let gap = GAP;
        let mut vt = VTerm::new();

        vt.feed(b"Hello");
        vt.feed(b"\n\n\n\x1b[2K  [bar10]");
        let up = format!("\x1b[{}A\x1b[{}G", gap + 1, 6);
        vt.feed(up.as_bytes());

        vt.feed(b" world");
        let down = format!("\x1b[{}B\r\x1b[2K", gap + 1);
        vt.feed(down.as_bytes());
        vt.feed(b"  [bar50]");
        let up = format!("\x1b[{}A\x1b[{}G", gap + 1, 12);
        vt.feed(up.as_bytes());

        assert!(vt.overwrites.is_empty());
        assert_eq!(vt.screen_text().lines().filter(|l| l.contains("[bar")).count(), 1);
    }

    #[test]
    fn cross_line_no_overwrite() {
        let gap = GAP;
        let mut vt = VTerm::new();

        vt.feed(b"Hello");
        vt.feed(b"\n\n\n\x1b[2K  [bar10]");
        let up = format!("\x1b[{}A\x1b[{}G", gap + 1, 6);
        vt.feed(up.as_bytes());

        vt.feed(b" world\nLine two");
        vt.feed(b"\x1b[J");
        vt.feed(b"\n\n\n\x1b[2K  [bar80]");
        let up = format!("\x1b[{}A\x1b[{}G", gap + 1, 9);
        vt.feed(up.as_bytes());

        assert!(vt.overwrites.is_empty());
        let s = vt.screen_text();
        assert!(s.contains("Hello world"));
        assert!(s.contains("Line two"));
        assert!(!s.contains("[bar10]"));
    }
}
