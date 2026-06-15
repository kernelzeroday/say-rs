mod synth;
mod ui;
pub mod vterm;

use clap::Parser;
use std::io::{self, IsTerminal, Read, Write};
use std::process;

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
        Some(ref name) => Some(
            synth::find_voice(name)?.ok_or_else(|| format!("voice '{}' not found", name))?,
        ),
        None => None,
    };

    let s = synth::Synthesizer::new(voice_spec)?;
    speak(&cli, &s, &text)
}

const MAX_CHUNK: usize = 2000;

fn chunk_text(text: &str) -> Vec<&str> {
    let mut chunks = Vec::new();
    let mut start = 0;
    while start < text.len() {
        let remaining = &text[start..];
        if remaining.len() <= MAX_CHUNK {
            chunks.push(remaining);
            break;
        }
        let end = (start + MAX_CHUNK).min(text.len());
        let search = &text[start..end];
        let break_at = search
            .rfind("\n\n")
            .map(|i| i + 2)
            .or_else(|| search.rfind('\n').map(|i| i + 1))
            .or_else(|| search.rfind(". ").map(|i| i + 2))
            .unwrap_or(search.len());
        chunks.push(&text[start..start + break_at]);
        start += break_at;
    }
    chunks
}

fn speak(
    cli: &Cli,
    s: &synth::Synthesizer,
    text: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(rate) = cli.rate {
        s.set_rate(rate)?;
    }

    let interactive = cli.interactive && !cli.quiet;
    let progress = cli.progress && !cli.quiet;

    let chunks = chunk_text(text);

    if interactive || progress {
        let mut display = ui::Display::new(text, interactive, progress);
        let mut utf16_offset: usize = 0;

        for chunk in &chunks {
            let offset = utf16_offset;
            s.speak(chunk, |pos, len| {
                display.on_word(offset + pos, len);
            })?;
            utf16_offset += chunk.encode_utf16().count();
        }

        display.finish();
    } else {
        for chunk in &chunks {
            s.speak(chunk, |_, _| {})?;
        }
    }

    Ok(())
}

extern "C" fn handle_sigint(_: libc::c_int) {
    let _ = io::stdout().write_all(b"\x1b[0m\n");
    unsafe { libc::_exit(130) };
}

fn main() {
    unsafe { libc::signal(libc::SIGINT, handle_sigint as *const () as libc::sighandler_t) };

    if let Err(e) = run() {
        eprintln!("say: {}", e);
        process::exit(1);
    }
}
