mod synth;
mod ui;
pub mod vterm;

use clap::Parser;
use std::io::{self, IsTerminal, Read};
use std::process;
use std::time::Instant;

#[derive(Parser)]
#[command(name = "say", about = "Convert text to audible speech")]
struct Cli {
    /// Text to speak
    text: Vec<String>,

    /// Voice to use (use '?' to list voices)
    #[arg(short = 'v', long = "voice")]
    voice: Option<String>,

    /// Speech rate in words per minute
    #[arg(short = 'r', long = "rate")]
    rate: Option<f64>,

    /// Read text from file (use '-' for stdin)
    #[arg(short = 'f', long = "input-file")]
    file: Option<String>,

    /// Stream words as they are spoken (default: on)
    #[arg(short = 'i', long = "interactive", default_value_t = true, action = clap::ArgAction::SetTrue)]
    interactive: bool,

    /// Show progress bar below text
    #[arg(long = "progress")]
    progress: bool,

    /// Suppress all output (just speak)
    #[arg(short = 'q', long = "quiet")]
    quiet: bool,

    /// Print timing debug info to stderr
    #[arg(short = 'd', long = "debug")]
    debug: bool,
}

fn get_text(cli: &Cli) -> Result<String, Box<dyn std::error::Error>> {
    if let Some(ref path) = cli.file {
        if path == "-" {
            let mut buf = String::new();
            std::io::stdin().read_to_string(&mut buf)?;
            return Ok(buf.trim_end().to_string());
        }
        return Ok(std::fs::read_to_string(path)?.trim_end().to_string());
    }

    if !cli.text.is_empty() {
        return Ok(cli.text.join(" "));
    }

    if io::stdin().is_terminal() {
        return Err("no text specified".into());
    }

    let mut buf = String::new();
    std::io::stdin().read_to_string(&mut buf)?;
    Ok(buf.trim_end().to_string())
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    if cli.voice.as_deref() == Some("?") {
        let voices = synth::list_voices()?;
        for v in &voices {
            println!("{}", v.name);
        }
        return Ok(());
    }

    let text = get_text(&cli)?;
    if text.is_empty() {
        return Ok(());
    }

    let voice_spec = match cli.voice {
        Some(ref name) => {
            Some(synth::find_voice(name)?.ok_or_else(|| format!("voice '{}' not found", name))?)
        }
        None => None,
    };

    let s = synth::Synthesizer::new(voice_spec)?;
    speak(&cli, &s, &text)
}

// --- Sentence-level chunking ---

#[derive(Debug, Clone, Copy)]
struct TextChunk<'a> {
    text: &'a str,
    start: usize,
    end: usize,
}

fn chunk_text(text: &str) -> Vec<TextChunk<'_>> {
    let mut chunks: Vec<TextChunk<'_>> = Vec::new();
    let bytes = text.as_bytes();
    let mut seg_start: usize = 0;

    let mut i = 0;
    while i < bytes.len() {
        // Paragraph break: \n\n+
        if bytes[i] == b'\n' && i + 1 < bytes.len() && bytes[i + 1] == b'\n' {
            let mut end = i;
            while end < bytes.len() && bytes[end] == b'\n' {
                end += 1;
            }
            if i > seg_start {
                chunks.push(TextChunk {
                    text: &text[seg_start..i],
                    start: seg_start,
                    end: i,
                });
            }
            seg_start = end;
            i = end;
            continue;
        }

        // Sentence end: .!? followed by space or newline, segment >= 30 bytes
        if i - seg_start >= 30 && is_sentence_boundary(bytes, i) {
            let end = i + 1;
            chunks.push(TextChunk {
                text: &text[seg_start..end],
                start: seg_start,
                end,
            });
            seg_start = end;
            // Skip whitespace after sentence
            while seg_start < bytes.len() && (bytes[seg_start] == b' ' || bytes[seg_start] == b'\n')
            {
                seg_start += 1;
            }
            i = seg_start;
            continue;
        }

        // Forced break at 500 bytes
        if i - seg_start >= 500 {
            let mut safe_i = i;
            while safe_i > seg_start && !text.is_char_boundary(safe_i) {
                safe_i -= 1;
            }
            let search = &text[seg_start..safe_i];
            let bp = last_sentence_boundary(search)
                .or_else(|| search.rfind('\n').map(|p| p + 1))
                .or_else(|| last_space_boundary(search))
                .unwrap_or(search.len());
            let end = seg_start + bp;
            chunks.push(TextChunk {
                text: &text[seg_start..end],
                start: seg_start,
                end,
            });
            seg_start = end;
            i = end;
            continue;
        }

        i += 1;
    }

    if seg_start < text.len() {
        let remainder = text[seg_start..].trim();
        if !remainder.is_empty() {
            chunks.push(TextChunk {
                text: &text[seg_start..],
                start: seg_start,
                end: text.len(),
            });
        }
    }

    // Merge adjacent tiny chunks (< 20 bytes) with next
    let mut merged: Vec<TextChunk<'_>> = Vec::new();
    let mut j = 0;
    while j < chunks.len() {
        let can_merge_gap = j + 1 < chunks.len()
            && text[chunks[j].end..chunks[j + 1].start]
                .bytes()
                .all(|b| b == b' ');
        if chunks[j].text.trim().len() < 8 && can_merge_gap {
            let start = chunks[j].start;
            let end = chunks[j + 1].end;
            merged.push(TextChunk {
                text: &text[start..end],
                start,
                end,
            });
            j += 2;
        } else {
            merged.push(chunks[j]);
            j += 1;
        }
    }

    if merged.is_empty() && !text.trim().is_empty() {
        merged.push(TextChunk {
            text,
            start: 0,
            end: text.len(),
        });
    }

    merged
}

fn is_abbreviation_period(bytes: &[u8], period: usize) -> bool {
    bytes[period] == b'.'
        && period > 0
        && bytes[period - 1].is_ascii_uppercase()
        && (period < 2 || !bytes[period - 2].is_ascii_alphanumeric())
}

fn is_sentence_boundary(bytes: &[u8], i: usize) -> bool {
    (bytes[i] == b'.' || bytes[i] == b'!' || bytes[i] == b'?')
        && i + 1 < bytes.len()
        && (bytes[i + 1] == b' ' || bytes[i + 1] == b'\n')
        && !is_abbreviation_period(bytes, i)
}

fn last_sentence_boundary(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut i = bytes.len().saturating_sub(1);
    while i > 0 {
        if is_sentence_boundary(bytes, i) {
            let mut end = i + 1;
            while end < bytes.len() && (bytes[end] == b' ' || bytes[end] == b'\n') {
                end += 1;
            }
            return Some(end);
        }
        i -= 1;
    }
    None
}

fn last_space_boundary(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut i = bytes.len().saturating_sub(1);
    while i > 0 {
        if bytes[i] == b' ' && (i == 0 || !is_abbreviation_period(bytes, i - 1)) {
            return Some(i + 1);
        }
        i -= 1;
    }
    None
}

// --- UTF-16 callback mapping ---

struct Utf16Map {
    utf16_to_byte: Vec<usize>,
}

impl Utf16Map {
    fn new(text: &str) -> Self {
        let mut utf16_to_byte = Vec::new();
        for (byte_idx, ch) in text.char_indices() {
            for _ in 0..ch.len_utf16() {
                utf16_to_byte.push(byte_idx);
            }
        }
        utf16_to_byte.push(text.len());
        Self { utf16_to_byte }
    }

    fn to_byte(&self, utf16_pos: usize) -> usize {
        self.utf16_to_byte
            .get(utf16_pos)
            .copied()
            .unwrap_or_else(|| self.utf16_to_byte.last().copied().unwrap_or(0))
    }
}

// --- Word helpers ---

fn word_end_bytes(text: &str) -> Vec<usize> {
    let mut ends = Vec::new();
    let mut in_word = false;
    for (i, ch) in text.char_indices() {
        if ch.is_whitespace() {
            if in_word {
                ends.push(i);
                in_word = false;
            }
        } else {
            in_word = true;
        }
    }
    if in_word {
        ends.push(text.len());
    }
    ends
}

// --- Speaking with sync ---

const DEFAULT_RATE: f64 = 175.0;

fn speak(cli: &Cli, s: &synth::Synthesizer, text: &str) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(rate) = cli.rate {
        s.set_rate(rate)?;
    }

    let stdout_tty = io::stdout().is_terminal();
    let interactive = cli.interactive && !cli.quiet && stdout_tty;
    let progress = cli.progress && !cli.quiet && stdout_tty;
    let debug = cli.debug;

    let chunks = chunk_text(text);

    if debug {
        let rate_wpm = s.get_rate().unwrap_or(DEFAULT_RATE);
        eprintln!("[debug] rate={:.0} wpm, chunks={}", rate_wpm, chunks.len());
        for (i, c) in chunks.iter().enumerate() {
            let words = word_end_bytes(c.text).len();
            let preview: String = c.text.chars().take(60).collect();
            let preview = preview.replace('\n', "\\n");
            eprintln!(
                "[debug] chunk[{}]: {} words, {} bytes @{}..{}: {:?}",
                i,
                words,
                c.text.len(),
                c.start,
                c.end,
                preview
            );
        }
    }

    if interactive || progress {
        let mut display = ui::Display::new(text, interactive, progress);

        for (ci, chunk) in chunks.iter().enumerate() {
            let utf16_map = Utf16Map::new(chunk.text);
            let expected_words = word_end_bytes(chunk.text).len();
            let mut session = s.start_speaking(chunk.text)?;
            let t0 = Instant::now();
            let mut observed_words: usize = 0;

            loop {
                let finished = session.pump(0.01);
                for ev in session.drain_words() {
                    observed_words += 1;
                    let utf16_end = ev.utf16_pos.saturating_add(ev.utf16_len);
                    let byte_end = utf16_map.to_byte(utf16_end);
                    display.emit_up_to(chunk.start + byte_end);
                }

                if finished {
                    break;
                }
            }

            display.emit_up_to(chunk.end);

            if debug {
                eprintln!(
                    "[debug] chunk[{}]: callbacks={}/{} actual={:.2}s",
                    ci,
                    observed_words,
                    expected_words,
                    t0.elapsed().as_secs_f64(),
                );
            }
        }

        display.finish();
    } else {
        for chunk in &chunks {
            s.speak(chunk.text, |_, _| {})?;
        }
    }

    Ok(())
}

extern "C" fn handle_sigint(_: libc::c_int) {
    const RESET: &[u8] = b"\x1b[0m\x1b[J\n";
    unsafe {
        libc::write(
            libc::STDOUT_FILENO,
            RESET.as_ptr().cast::<libc::c_void>(),
            RESET.len(),
        );
        libc::_exit(130);
    }
}

fn main() {
    unsafe {
        libc::signal(
            libc::SIGINT,
            handle_sigint as *const () as libc::sighandler_t,
        )
    };

    if let Err(e) = run() {
        eprintln!("say: {}", e);
        process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn word_ends_basic() {
        assert_eq!(word_end_bytes("Hello world"), vec![5, 11]);
    }

    #[test]
    fn word_ends_multiline() {
        assert_eq!(word_end_bytes("Hello\nworld"), vec![5, 11]);
    }

    #[test]
    fn word_ends_empty() {
        assert_eq!(word_end_bytes(""), Vec::<usize>::new());
    }

    #[test]
    fn chunk_splits_sentences() {
        let text = "Hello world. This is a second sentence. And a third one.";
        let chunks = chunk_text(text);
        assert!(
            chunks.len() >= 2,
            "should split at sentence boundaries: {:?}",
            chunks
        );
        assert_eq!(chunks[0].text, "Hello world. This is a second sentence.");
        assert_eq!(chunks[1].start, text.find("And").unwrap());
    }

    #[test]
    fn chunk_splits_paragraphs() {
        let text = "First paragraph.\n\nSecond paragraph.";
        let chunks = chunk_text(text);
        assert_eq!(chunks.len(), 2, "chunks: {:?}", chunks);
    }

    #[test]
    fn chunk_skips_abbreviations() {
        let text = "Dr. Smith went to Washington D.C. and met Mr. Jones for lunch.";
        let chunks = chunk_text(text);
        // "D.C." and "Dr." and "Mr." should not cause splits (single uppercase before period)
        // Only "lunch." at end
        assert!(
            chunks.len() <= 2,
            "too many splits on abbreviations: {:?}",
            chunks
        );
    }

    #[test]
    fn chunk_merges_tiny() {
        let text = "Hi.\n\nBye world and more text here please.";
        let chunks = chunk_text(text);
        assert!(chunks.len() <= 2, "tiny chunk not merged: {:?}", chunks);
    }

    #[test]
    fn chunk_tiny_merge_does_not_cross_paragraph_break() {
        let text = "OK.\n\nThis is a longer second paragraph.";
        let chunks = chunk_text(text);
        assert_eq!(
            chunks.len(),
            2,
            "paragraph chunks should stay split: {:?}",
            chunks
        );
        assert_eq!(chunks[0].text, "OK.");
        assert_eq!(chunks[1].text, "This is a longer second paragraph.");
    }

    #[test]
    fn chunk_handles_long() {
        let text = "a ".repeat(300);
        let chunks = chunk_text(&text);
        assert!(chunks.len() >= 2);
        for c in &chunks {
            assert!(
                c.text.len() <= 510,
                "chunk too large: {} bytes",
                c.text.len()
            );
        }
    }

    #[test]
    fn chunk_handles_multibyte() {
        // 3-byte UTF-8 chars (smart quotes, em dashes)
        let text = "\u{201c}Hello\u{201d} world \u{2014} this is a test. Second sentence here. Third one too. Fourth also. Fifth as well.";
        let chunks = chunk_text(text);
        for c in &chunks {
            // Verify all chunks are valid UTF-8 (no panics on slice)
            let _ = c.text.len();
            assert!(!c.text.is_empty());
        }
    }

    #[test]
    fn chunk_long_multibyte() {
        // Force a 500-byte break with multibyte chars
        let word = "\u{201c}test\u{201d} ";
        let text = word.repeat(100); // ~800 bytes
        let chunks = chunk_text(&text);
        assert!(chunks.len() >= 2, "should split long multibyte text");
        for c in &chunks {
            let _ = c.text.len();
        }
    }

    #[test]
    fn forced_break_does_not_prefer_abbreviation_period() {
        let text = format!("{}U.S. {}", "a ".repeat(247), "b ".repeat(80));
        let chunks = chunk_text(&text);
        assert!(chunks.len() >= 2, "long input should be forced into chunks");
        assert!(
            chunks[1].text.starts_with("U.S."),
            "forced break should move abbreviation to next chunk: {:?}",
            chunks
        );
    }

    #[test]
    fn chunk_offsets_include_skipped_space_before_multibyte_word() {
        let text = "This first sentence is longer than thirty bytes. caf\u{e9} wins.";
        let chunks = chunk_text(text);
        let second = chunks
            .iter()
            .find(|chunk| chunk.text.starts_with("caf\u{e9}"))
            .expect("second chunk should start at the multibyte word");

        assert_eq!(second.start, text.find("caf\u{e9}").unwrap());

        let map = Utf16Map::new(second.text);
        let utf16_word_end = "caf\u{e9}".encode_utf16().count();
        let absolute_end = second.start + map.to_byte(utf16_word_end);
        assert_eq!(
            &text[..absolute_end],
            "This first sentence is longer than thirty bytes. caf\u{e9}"
        );
    }

    #[test]
    fn utf16_map_handles_surrogate_pairs() {
        let text = "a \u{1f9ea} b";
        let map = Utf16Map::new(text);
        let through_emoji = "a \u{1f9ea}".encode_utf16().count();
        assert_eq!(map.to_byte(through_emoji), "a \u{1f9ea}".len());
    }
}
