use std::io::{self, IsTerminal, Write};
use std::time::Duration;

const GAP: usize = 2;

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

fn char_display_width(ch: char, col: usize) -> usize {
    if ch == '\t' {
        let tab_width = 8;
        return tab_width - (col % tab_width);
    }

    let code = ch as u32;
    if is_zero_width(code) {
        0
    } else if is_wide(code) {
        2
    } else {
        1
    }
}

fn is_zero_width(code: u32) -> bool {
    matches!(
        code,
        0x0300..=0x036f
            | 0x0483..=0x0489
            | 0x0591..=0x05bd
            | 0x05bf
            | 0x05c1..=0x05c2
            | 0x05c4..=0x05c5
            | 0x05c7
            | 0x0610..=0x061a
            | 0x064b..=0x065f
            | 0x0670
            | 0x06d6..=0x06dc
            | 0x06df..=0x06e4
            | 0x06e7..=0x06e8
            | 0x06ea..=0x06ed
            | 0x0711
            | 0x0730..=0x074a
            | 0x07a6..=0x07b0
            | 0x07eb..=0x07f3
            | 0x0816..=0x0819
            | 0x081b..=0x0823
            | 0x0825..=0x0827
            | 0x0829..=0x082d
            | 0x0859..=0x085b
            | 0x08d3..=0x08e1
            | 0x08e3..=0x0902
            | 0x093a
            | 0x093c
            | 0x0941..=0x0948
            | 0x094d
            | 0x0951..=0x0957
            | 0x0962..=0x0963
            | 0x0981
            | 0x09bc
            | 0x09c1..=0x09c4
            | 0x09cd
            | 0x09e2..=0x09e3
            | 0x0a01..=0x0a02
            | 0x0a3c
            | 0x0a41..=0x0a42
            | 0x0a47..=0x0a48
            | 0x0a4b..=0x0a4d
            | 0x0a51
            | 0x0a70..=0x0a71
            | 0x0a75
            | 0x0a81..=0x0a82
            | 0x0abc
            | 0x0ac1..=0x0ac5
            | 0x0ac7..=0x0ac8
            | 0x0acd
            | 0x0ae2..=0x0ae3
            | 0x0b01
            | 0x0b3c
            | 0x0b3f
            | 0x0b41..=0x0b44
            | 0x0b4d
            | 0x0b56
            | 0x0b62..=0x0b63
            | 0x0b82
            | 0x0bc0
            | 0x0bcd
            | 0x0c00
            | 0x0c04
            | 0x0c3e..=0x0c40
            | 0x0c46..=0x0c48
            | 0x0c4a..=0x0c4d
            | 0x0c55..=0x0c56
            | 0x0c62..=0x0c63
            | 0x0c81
            | 0x0cbc
            | 0x0cbf
            | 0x0cc6
            | 0x0ccc..=0x0ccd
            | 0x0ce2..=0x0ce3
            | 0x0d00..=0x0d01
            | 0x0d3b..=0x0d3c
            | 0x0d41..=0x0d44
            | 0x0d4d
            | 0x0d62..=0x0d63
            | 0x0dca
            | 0x0dd2..=0x0dd4
            | 0x0dd6
            | 0x0e31
            | 0x0e34..=0x0e3a
            | 0x0e47..=0x0e4e
            | 0x0eb1
            | 0x0eb4..=0x0ebc
            | 0x0ec8..=0x0ecd
            | 0x0f18..=0x0f19
            | 0x0f35
            | 0x0f37
            | 0x0f39
            | 0x0f71..=0x0f7e
            | 0x0f80..=0x0f84
            | 0x0f86..=0x0f87
            | 0x0f8d..=0x0f97
            | 0x0f99..=0x0fbc
            | 0x0fc6
            | 0x102d..=0x1030
            | 0x1032..=0x1037
            | 0x1039..=0x103a
            | 0x103d..=0x103e
            | 0x1058..=0x1059
            | 0x105e..=0x1060
            | 0x1071..=0x1074
            | 0x1082
            | 0x1085..=0x1086
            | 0x108d
            | 0x109d
            | 0x135d..=0x135f
            | 0x1712..=0x1714
            | 0x1732..=0x1734
            | 0x1752..=0x1753
            | 0x1772..=0x1773
            | 0x17b4..=0x17b5
            | 0x17b7..=0x17bd
            | 0x17c6
            | 0x17c9..=0x17d3
            | 0x17dd
            | 0x180b..=0x180f
            | 0x1885..=0x1886
            | 0x18a9
            | 0x1920..=0x1922
            | 0x1927..=0x1928
            | 0x1932
            | 0x1939..=0x193b
            | 0x1a17..=0x1a18
            | 0x1a1b
            | 0x1a56
            | 0x1a58..=0x1a5e
            | 0x1a60
            | 0x1a62
            | 0x1a65..=0x1a6c
            | 0x1a73..=0x1a7c
            | 0x1a7f
            | 0x1ab0..=0x1aff
            | 0x1b00..=0x1b03
            | 0x1b34
            | 0x1b36..=0x1b3a
            | 0x1b3c
            | 0x1b42
            | 0x1b6b..=0x1b73
            | 0x1b80..=0x1b81
            | 0x1ba2..=0x1ba5
            | 0x1ba8..=0x1ba9
            | 0x1bab..=0x1bad
            | 0x1be6
            | 0x1be8..=0x1be9
            | 0x1bed
            | 0x1bef..=0x1bf1
            | 0x1c2c..=0x1c33
            | 0x1c36..=0x1c37
            | 0x1cd0..=0x1cd2
            | 0x1cd4..=0x1ce0
            | 0x1ce2..=0x1ce8
            | 0x1ced
            | 0x1cf4
            | 0x1cf8..=0x1cf9
            | 0x1dc0..=0x1dff
            | 0x200b..=0x200f
            | 0x202a..=0x202e
            | 0x2060..=0x2064
            | 0x2066..=0x206f
            | 0x20d0..=0x20ff
            | 0xfe00..=0xfe0f
            | 0xfe20..=0xfe2f
            | 0xfeff
    )
}

fn is_wide(code: u32) -> bool {
    matches!(
        code,
        0x1100..=0x115f
            | 0x231a..=0x231b
            | 0x2329..=0x232a
            | 0x23e9..=0x23ec
            | 0x23f0
            | 0x23f3
            | 0x25fd..=0x25fe
            | 0x2614..=0x2615
            | 0x2648..=0x2653
            | 0x267f
            | 0x2693
            | 0x26a1
            | 0x26aa..=0x26ab
            | 0x26bd..=0x26be
            | 0x26c4..=0x26c5
            | 0x26ce
            | 0x26d4
            | 0x26ea
            | 0x26f2..=0x26f3
            | 0x26f5
            | 0x26fa
            | 0x26fd
            | 0x2705
            | 0x270a..=0x270b
            | 0x2728
            | 0x274c
            | 0x274e
            | 0x2753..=0x2755
            | 0x2757
            | 0x2795..=0x2797
            | 0x27b0
            | 0x27bf
            | 0x2b1b..=0x2b1c
            | 0x2b50
            | 0x2b55
            | 0x2e80..=0xa4cf
            | 0xac00..=0xd7a3
            | 0xf900..=0xfaff
            | 0xfe10..=0xfe19
            | 0xfe30..=0xfe6f
            | 0xff00..=0xff60
            | 0xffe0..=0xffe6
            | 0x1f004
            | 0x1f0cf
            | 0x1f18e
            | 0x1f191..=0x1f19a
            | 0x1f200..=0x1f202
            | 0x1f210..=0x1f23b
            | 0x1f240..=0x1f248
            | 0x1f250..=0x1f251
            | 0x1f300..=0x1f64f
            | 0x1f680..=0x1f6ff
            | 0x1f900..=0x1f9ff
            | 0x20000..=0x3fffd
    )
}

pub struct Display {
    text: String,
    interactive: bool,
    progress: bool,
    emitted_up_to: usize,
    col: usize,
    visual_rows_since_pad: usize,
    term_width: usize,
    pad_drawn: bool,
    is_tty: bool,
    char_delay_us: u64,
}

impl Display {
    pub fn new(text: &str, interactive: bool, progress: bool) -> Self {
        let is_tty = io::stdout().is_terminal();
        Self {
            text: text.to_string(),
            interactive: interactive && is_tty,
            progress: progress && is_tty,
            emitted_up_to: 0,
            col: 0,
            visual_rows_since_pad: 0,
            term_width: terminal_width(),
            pad_drawn: false,
            is_tty,
            char_delay_us: 8_000,
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

    fn pct_bytes(&self, byte_pos: usize) -> f64 {
        if !self.text.is_empty() {
            (byte_pos as f64 / self.text.len() as f64).min(1.0)
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
            self.col += char_display_width(ch, self.col);
            while self.term_width > 0 && self.col >= self.term_width {
                self.col -= self.term_width;
                self.visual_rows_since_pad += 1;
            }
        }
    }

    pub fn emit_up_to(&mut self, byte_end: usize) {
        if byte_end <= self.emitted_up_to {
            return;
        }
        let mut byte_end = byte_end.min(self.text.len());
        while byte_end > self.emitted_up_to && !self.text.is_char_boundary(byte_end) {
            byte_end -= 1;
        }
        if byte_end <= self.emitted_up_to {
            return;
        }

        if self.interactive {
            let mut out = io::stdout();
            let show_progress = self.progress && self.is_tty;
            let chunk: String = self.text[self.emitted_up_to..byte_end].to_string();

            for ch in chunk.chars() {
                write!(out, "{}", ch).ok();
                out.flush().ok();
                self.emit_char(ch);
                if !ch.is_whitespace() && self.char_delay_us > 0 {
                    std::thread::sleep(Duration::from_micros(self.char_delay_us));
                }
            }
            if show_progress {
                let pct = self.pct_bytes(byte_end);
                if !self.pad_drawn {
                    self.create_pad(&mut out, pct);
                } else if self.visual_rows_since_pad > 0 {
                    self.erase_pad(&mut out);
                    self.create_pad(&mut out, pct);
                } else {
                    self.update_bar_inplace(&mut out, pct);
                }
            }
        } else if self.progress && self.is_tty {
            let mut out = io::stdout();
            write!(out, "\r\x1b[2K").ok();
            Self::draw_bar(&mut out, self.pct_bytes(byte_end));
            out.flush().ok();
        }

        self.emitted_up_to = byte_end;
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
        assert_eq!(
            vt.screen_text()
                .lines()
                .filter(|l| l.contains("[bar"))
                .count(),
            1
        );
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

    #[test]
    fn emit_up_to_advances() {
        let mut d = Display::new("Hello world", true, false);
        d.char_delay_us = 0; // skip delay in tests
        d.emit_up_to(5);
        assert_eq!(d.emitted_up_to, 5);
        d.emit_up_to(11);
        assert_eq!(d.emitted_up_to, 11);
    }

    #[test]
    fn emit_up_to_no_backward() {
        let mut d = Display::new("Hello world", true, false);
        d.char_delay_us = 0;
        d.emit_up_to(8);
        d.emit_up_to(5); // should not go backward
        assert_eq!(d.emitted_up_to, 8);
    }

    #[test]
    fn emit_up_to_snaps_to_char_boundary() {
        let mut d = Display::new("caf\u{e9}", true, false);
        d.char_delay_us = 0;
        d.emit_up_to(4);
        assert_eq!(d.emitted_up_to, 3);
    }

    #[test]
    fn pct_bytes_correct() {
        let d = Display::new("0123456789", true, false);
        assert!((d.pct_bytes(5) - 0.5).abs() < 0.01);
        assert!((d.pct_bytes(10) - 1.0).abs() < 0.01);
    }
}
