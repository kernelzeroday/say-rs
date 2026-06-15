# say-rs

A Rust reimplementation of the macOS `say` command with streaming word highlighting and a progress meter.

Uses direct FFI bindings to the macOS `SpeechSynthesis` framework — the same Carbon API that `/usr/bin/say` uses under the hood. No wrapping of the upstream binary, no Objective-C runtime.

## Install

```
cargo install --path .
```

## Usage

```
say "Hello world"                          # speak text
say -v Samantha "Hello"                    # choose a voice
say -v '?'                                 # list available voices
say -r 250 "Fast speech"                   # set rate (words per minute)
say -f speech.txt                          # read from file
echo "piped text" | say                    # read from stdin
say -i "Watch the words"                   # interactive: highlight words as spoken
say --progress "Long text..."              # show a progress bar
say -i --progress "Both at once"           # combine them
say -q "Silent mode"                       # suppress all visual output
```

## Features

- **Interactive mode** (`-i`): real-time word-by-word highlighting via reverse video, driven by the framework's word callback
- **Progress bar** (`--progress`): character-level progress with a colored bar
- **Combined display**: interactive + progress render as a clean two-line block
- **Voice selection**: full access to all system voices
- **Rate control**: words-per-minute speech rate
- **Stdin/file input**: pipe text in or read from files
- **UTF-16 aware**: correct word highlighting for non-ASCII text

## How it works

The binary links against `ApplicationServices.framework` and calls:

- `NewSpeechChannel` / `DisposeSpeechChannel` — channel lifecycle
- `SpeakCFString` — initiate speech
- `SetSpeechProperty` with `kSpeechWordCFCallBack` — register a word-boundary callback that fires for each word with its `CFRange`
- `SetSpeechProperty` with `kSpeechSpeechDoneCallBack` — completion notification
- `CFRunLoop` — event pump for callbacks

The word callback provides UTF-16 ranges which are mapped to UTF-8 byte offsets for terminal rendering.

## Requirements

- macOS (uses macOS-only frameworks)
- Rust 1.85+
