use std::fmt;

pub struct VTerm {
    pub rows: Vec<Vec<char>>,
    pub row: usize,
    pub col: usize,
    pub overwrites: Vec<Overwrite>,
}

pub struct Overwrite {
    pub row: usize,
    pub col: usize,
    pub old: char,
    pub new: char,
}

impl fmt::Display for Overwrite {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "  row={} col={}: '{}' overwritten by '{}'",
            self.row, self.col, self.old, self.new
        )
    }
}

impl VTerm {
    pub fn new() -> Self {
        Self {
            rows: vec![Vec::new()],
            row: 0,
            col: 0,
            overwrites: Vec::new(),
        }
    }

    fn ensure_row(&mut self, r: usize) {
        while self.rows.len() <= r {
            self.rows.push(Vec::new());
        }
    }

    fn put_char(&mut self, ch: char) {
        self.ensure_row(self.row);
        let row = &mut self.rows[self.row];
        while row.len() <= self.col {
            row.push(' ');
        }
        let old = row[self.col];
        if old != ' ' && old != ch {
            self.overwrites.push(Overwrite {
                row: self.row,
                col: self.col,
                old,
                new: ch,
            });
        }
        row[self.col] = ch;
        self.col += 1;
    }

    pub fn feed(&mut self, bytes: &[u8]) {
        let s = String::from_utf8_lossy(bytes);
        let mut chars = s.chars().peekable();

        while let Some(ch) = chars.next() {
            if ch == '\x1b' {
                if chars.peek() == Some(&'[') {
                    chars.next();
                    let mut params = String::new();
                    while let Some(&c) = chars.peek() {
                        if c.is_ascii_digit() || c == ';' {
                            params.push(c);
                            chars.next();
                        } else {
                            break;
                        }
                    }
                    if let Some(cmd) = chars.next() {
                        self.handle_csi(&params, cmd);
                    }
                }
            } else if ch == '\n' {
                self.row += 1;
                self.col = 0;
                self.ensure_row(self.row);
            } else if ch == '\r' {
                self.col = 0;
            } else if !ch.is_control() {
                self.put_char(ch);
            }
        }
    }

    fn handle_csi(&mut self, params: &str, cmd: char) {
        let n: usize = params.parse().unwrap_or(1);
        match cmd {
            'A' => self.row = self.row.saturating_sub(n),
            'B' => self.row += n,
            'G' => self.col = n.saturating_sub(1),
            'K' => {
                self.ensure_row(self.row);
                let row = &mut self.rows[self.row];
                // erase line (mode 2 = entire line, default/0 = cursor to end)
                let mode: usize = params.parse().unwrap_or(0);
                match mode {
                    2 => {
                        row.clear();
                        self.col = 0;
                    }
                    0 => row.truncate(self.col),
                    _ => {}
                }
            }
            'm' => {} // SGR (colors/attributes) - ignore
            _ => {}
        }
    }

    pub fn screen_text(&self) -> String {
        self.rows
            .iter()
            .map(|r| r.iter().collect::<String>().trim_end().to_string())
            .collect::<Vec<_>>()
            .join("\n")
            .trim_end_matches('\n')
            .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_text() {
        let mut v = VTerm::new();
        v.feed(b"Hello world");
        assert_eq!(v.screen_text(), "Hello world");
        assert!(v.overwrites.is_empty());
    }

    #[test]
    fn newline() {
        let mut v = VTerm::new();
        v.feed(b"line one\nline two");
        assert_eq!(v.screen_text(), "line one\nline two");
        assert!(v.overwrites.is_empty());
    }

    #[test]
    fn detects_overwrite() {
        let mut v = VTerm::new();
        v.feed(b"ABCD");
        v.feed(b"\rXX");
        assert_eq!(v.overwrites.len(), 2);
        assert_eq!(v.overwrites[0].old, 'A');
        assert_eq!(v.overwrites[0].new, 'X');
    }

    #[test]
    fn cursor_up_preserves_col_from_bar() {
        let mut v = VTerm::new();
        v.feed(b"Hello");
        v.feed(b"\n\n\nbar");
        // cursor-up 3 keeps col=3 (from "bar"), so writing overwrites "lo"
        v.feed(b"\x1b[3A");
        v.feed(b" world");
        assert!(!v.overwrites.is_empty(), "cursor-up without col fix causes overwrite");
    }

    #[test]
    fn cursor_up_with_explicit_col_ok() {
        let mut v = VTerm::new();
        v.feed(b"Hello");               // col=5
        v.feed(b"\n\n\nbar");           // col=3 after "bar"
        v.feed(b"\x1b[3A\x1b[6G");     // up 3, col 6 (1-indexed → col 5)
        v.feed(b" world");
        assert_eq!(v.screen_text(), "Hello world\n\n\nbar");
        assert!(v.overwrites.is_empty());
    }

    #[test]
    fn cursor_up_with_cr_causes_overwrite() {
        let mut v = VTerm::new();
        v.feed(b"Hello");
        v.feed(b"\n\n\nbar");
        v.feed(b"\x1b[3A\r"); // cursor up 3, then CR — col 0!
        v.feed(b"XX");
        assert!(!v.overwrites.is_empty(), "\\r after cursor-up should cause overwrite");
    }

    #[test]
    fn column_restore_no_overwrite() {
        let mut v = VTerm::new();
        v.feed(b"Hello");           // col=5
        v.feed(b"\n\n\nbar");       // 3 lines down, write bar
        v.feed(b"\x1b[3A\x1b[6G"); // up 3, column 6 (1-indexed = col 5)
        v.feed(b" world");
        assert_eq!(v.screen_text(), "Hello world\n\n\nbar");
        assert!(v.overwrites.is_empty());
    }

    #[test]
    fn erase_line_on_bar_ok() {
        let mut v = VTerm::new();
        v.feed(b"Hello");
        v.feed(b"\n\n\n\x1b[2Kprogress bar");
        v.feed(b"\x1b[3A\x1b[6G");
        v.feed(b" world");
        v.feed(b"\n\n\n\x1b[2Kprogress bar v2");
        assert!(v.overwrites.is_empty());
    }

    /// Simulates the full pad cycle: text → pad → undo → more text → pad
    #[test]
    fn full_pad_cycle_no_overwrites() {
        let gap = 2;
        let mut v = VTerm::new();
        let mut col: usize = 0;

        // Word 1: "Hello"
        v.feed(b"Hello");
        col = 5;
        // draw pad: GAP blank lines + bar line
        v.feed(b"\n\n\n\x1b[2Kbar10%");

        // Word 2: " world"
        // undo pad: up GAP+1, restore col
        let seq = format!("\x1b[{}A\x1b[{}G", gap + 1, col + 1);
        v.feed(seq.as_bytes());
        v.feed(b" world");
        col = 11;
        // draw pad again
        v.feed(b"\n\n\n\x1b[2Kbar50%");

        // Word 3: newline then "Second"
        let seq = format!("\x1b[{}A\x1b[{}G", gap + 1, col + 1);
        v.feed(seq.as_bytes());
        v.feed(b"\nSecond");
        col = 6;
        v.feed(b"\n\n\n\x1b[2Kbar80%");

        // Word 4: " line"
        let seq = format!("\x1b[{}A\x1b[{}G", gap + 1, col + 1);
        v.feed(seq.as_bytes());
        v.feed(b" line");
        col = 11;

        assert!(
            v.overwrites.is_empty(),
            "overwrites detected:\n{}",
            v.overwrites.iter().map(|o| o.to_string()).collect::<Vec<_>>().join("\n")
        );
        let text = v.screen_text();
        assert!(text.starts_with("Hello world\nSecond line"), "got: {text}");
    }

    /// Proves the OLD undo_pad with \\r causes overwrites
    #[test]
    fn old_undo_pad_with_cr_fails() {
        let gap = 2;
        let mut v = VTerm::new();

        v.feed(b"Hello");
        v.feed(b"\n\n\n\x1b[2Kbar10%");

        // OLD behavior: cursor up + \r (resets to col 0)
        let seq = format!("\x1b[{}A\r", gap + 1);
        v.feed(seq.as_bytes());
        v.feed(b" world");

        assert!(
            !v.overwrites.is_empty(),
            "old \\r approach should overwrite text"
        );
    }
}
