mod synth;
mod ui;
pub mod vterm;

use clap::Parser;
use std::io::{self, IsTerminal, Read};
use std::process;
use std::time::{Duration, Instant};

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

// --- Size-based chunking with smart break points ---

const MAX_CHUNK: usize = 2000;

fn split_chunks(text: &str) -> Vec<(usize, &str)> {
    if text.trim().is_empty() {
        return Vec::new();
    }
    if text.len() <= MAX_CHUNK {
        return vec![(0, text)];
    }

    let mut chunks = Vec::new();
    let mut start = 0;

    while start < text.len() {
        while start < text.len() && text.as_bytes()[start].is_ascii_whitespace() {
            start += 1;
        }
        if start >= text.len() {
            break;
        }
        if text.len() - start <= MAX_CHUNK {
            chunks.push((start, &text[start..]));
            break;
        }

        let bp = snap_to_break(&text[start..], MAX_CHUNK);
        chunks.push((start, &text[start..start + bp]));
        start += bp;
    }

    chunks
}

fn snap_to_break(text: &str, max: usize) -> usize {
    let mut limit = max.min(text.len());
    while limit > 0 && !text.is_char_boundary(limit) {
        limit -= 1;
    }

    let bytes = &text.as_bytes()[..limit];
    let floor = limit / 4;

    for i in (floor..bytes.len().saturating_sub(1)).rev() {
        if bytes[i] == b'\n' && bytes[i + 1] == b'\n' {
            return i;
        }
    }
    for i in (floor..bytes.len()).rev() {
        if bytes[i] == b'\n' {
            return i;
        }
    }
    for i in (floor..bytes.len().saturating_sub(1)).rev() {
        if matches!(bytes[i], b'.' | b'!' | b'?') && bytes[i + 1] == b' ' {
            return i + 1;
        }
    }
    for i in (floor..bytes.len()).rev() {
        if bytes[i] == b' ' {
            return i;
        }
    }

    limit
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

    let paras = split_chunks(text);

    if debug {
        let rate_wpm = s.get_rate().unwrap_or(DEFAULT_RATE);
        eprintln!(
            "[debug] rate={:.0} wpm, {} bytes, {} paragraphs",
            rate_wpm,
            text.len(),
            paras.len()
        );
    }

    if interactive || progress {
        let mut display = ui::Display::new(text, interactive, progress);
        let rate_wpm = s.get_rate().unwrap_or(DEFAULT_RATE);
        let word_interval = Duration::from_secs_f64(60.0 / rate_wpm);

        for &(offset, para) in &paras {
            let utf16_map = Utf16Map::new(para);
            let mut session = s.start_speaking(para)?;
            let t0 = Instant::now();
            let mut words: u32 = 0;

            loop {
                let finished = session.pump(0.01);
                for ev in session.drain_words() {
                    words += 1;
                    let target = word_interval * words;
                    let elapsed = t0.elapsed();
                    if target > elapsed {
                        std::thread::sleep(target - elapsed);
                    }
                    let utf16_end = ev.utf16_pos.saturating_add(ev.utf16_len);
                    let byte_end = utf16_map.to_byte(utf16_end);
                    display.emit_up_to(offset + byte_end);
                }
                if finished {
                    break;
                }
            }

            display.emit_up_to(offset + para.len());
        }

        display.emit_up_to(text.len());
        display.finish();
    } else {
        for &(_, para) in &paras {
            s.speak(para, |_, _| {})?;
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
    fn chunks_short_text_single() {
        let text = "Hello world, this is short.";
        let c = split_chunks(text);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0], (0, text));
    }

    #[test]
    fn chunks_long_splits_at_sentence() {
        let a = "x".repeat(1800);
        let text = format!("{}. {}", a, "y".repeat(500));
        let c = split_chunks(&text);
        assert!(c.len() >= 2, "should split: {:?}", c.len());
        assert!(c[0].1.ends_with('.'));
    }

    #[test]
    fn chunks_prefers_paragraph_break() {
        let a = "word ".repeat(350);
        let b = "more ".repeat(200);
        let text = format!("{}\n\n{}", a.trim(), b.trim());
        let c = split_chunks(&text);
        assert!(c.len() >= 2);
    }

    #[test]
    fn chunks_no_empty() {
        let c = split_chunks("   \n\n  ");
        assert!(c.is_empty());
    }

    #[test]
    fn utf16_map_handles_surrogate_pairs() {
        let text = "a \u{1f9ea} b";
        let map = Utf16Map::new(text);
        let through_emoji = "a \u{1f9ea}".encode_utf16().count();
        assert_eq!(map.to_byte(through_emoji), "a \u{1f9ea}".len());
    }
}
