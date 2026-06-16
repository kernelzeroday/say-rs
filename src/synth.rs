use core_foundation::base::TCFType;
use core_foundation::boolean::CFBoolean;
use core_foundation::dictionary::CFDictionary;
use core_foundation::number::CFNumber;
use core_foundation::string::{CFString, CFStringRef};
use core_foundation_sys::base::{CFRange, CFTypeRef};
use core_foundation_sys::dictionary::CFDictionaryRef;
use core_foundation_sys::runloop::{
    CFRunLoopGetCurrent, CFRunLoopRunInMode, CFRunLoopStop, kCFRunLoopDefaultMode,
};
use std::ffi::c_void;
use std::fmt;
use std::marker::PhantomData;
use std::ptr;
use std::sync::{
    Mutex,
    atomic::{AtomicBool, Ordering},
};

type SpeechChannelPtr = *mut c_void;

#[repr(C, packed(2))]
#[derive(Clone, Copy)]
pub struct VoiceSpec {
    pub creator: u32,
    pub id: u32,
}

#[repr(C, packed(2))]
pub struct VoiceDescription {
    pub length: i32,
    pub voice: VoiceSpec,
    pub version: i32,
    pub name: [u8; 64],
    pub comment: [u8; 256],
    pub gender: i16,
    pub age: i16,
    pub script: i16,
    pub language: i16,
    pub region: i16,
    pub reserved: [i32; 4],
}

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn NewSpeechChannel(voice: *const VoiceSpec, chan: *mut SpeechChannelPtr) -> i16;
    fn DisposeSpeechChannel(chan: SpeechChannelPtr) -> i16;
    fn SpeakCFString(chan: SpeechChannelPtr, text: CFStringRef, options: CFDictionaryRef) -> i16;
    #[allow(dead_code)]
    fn StopSpeech(chan: SpeechChannelPtr) -> i16;
    fn SetSpeechProperty(chan: SpeechChannelPtr, property: CFStringRef, object: CFTypeRef) -> i16;
    fn CountVoices(num_voices: *mut i16) -> i16;
    fn GetIndVoice(index: i16, voice: *mut VoiceSpec) -> i16;
    fn GetVoiceDescription(
        voice: *const VoiceSpec,
        info: *mut VoiceDescription,
        info_length: i64,
    ) -> i16;
    fn CopySpeechProperty(
        chan: SpeechChannelPtr,
        property: CFStringRef,
        object: *mut CFTypeRef,
    ) -> i16;
    #[allow(dead_code)]
    fn SpeechBusy() -> i16;

    static kSpeechWordCFCallBack: CFStringRef;
    static kSpeechSpeechDoneCallBack: CFStringRef;
    static kSpeechRefConProperty: CFStringRef;
    static kSpeechRateProperty: CFStringRef;
    static kSpeechNoEndingProsody: CFStringRef;
    static kSpeechNoSpeechInterrupt: CFStringRef;
}

#[derive(Debug)]
pub struct SpeechError {
    pub code: i16,
    pub context: String,
}

impl fmt::Display for SpeechError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} (OSErr {})", self.context, self.code)
    }
}

impl std::error::Error for SpeechError {}

fn check(err: i16, context: &str) -> Result<(), SpeechError> {
    if err == 0 {
        Ok(())
    } else {
        Err(SpeechError {
            code: err,
            context: context.to_string(),
        })
    }
}

pub struct VoiceInfo {
    pub name: String,
    pub spec: VoiceSpec,
}

fn pascal_to_string(data: &[u8]) -> String {
    let len = data[0] as usize;
    if len == 0 || len >= data.len() {
        return String::new();
    }
    String::from_utf8_lossy(&data[1..1 + len]).to_string()
}

pub fn list_voices() -> Result<Vec<VoiceInfo>, SpeechError> {
    let mut count: i16 = 0;
    check(unsafe { CountVoices(&mut count) }, "CountVoices")?;

    let mut voices = Vec::with_capacity(count as usize);
    for i in 1..=count {
        let mut spec = VoiceSpec { creator: 0, id: 0 };
        check(unsafe { GetIndVoice(i, &mut spec) }, "GetIndVoice")?;

        let mut desc: VoiceDescription = unsafe { std::mem::zeroed() };
        check(
            unsafe {
                GetVoiceDescription(
                    &spec,
                    &mut desc,
                    std::mem::size_of::<VoiceDescription>() as i64,
                )
            },
            "GetVoiceDescription",
        )?;

        voices.push(VoiceInfo {
            name: pascal_to_string(&desc.name),
            spec,
        });
    }

    Ok(voices)
}

pub fn find_voice(name: &str) -> Result<Option<VoiceSpec>, SpeechError> {
    let voices = list_voices()?;
    let name_lower = name.to_lowercase();
    Ok(voices
        .into_iter()
        .find(|v| v.name.to_lowercase() == name_lower)
        .map(|v| v.spec))
}

#[derive(Debug, Clone)]
pub struct WordEvent {
    pub utf16_pos: usize,
    pub utf16_len: usize,
}

struct CallbackCollector {
    words: Mutex<Vec<WordEvent>>,
    done: AtomicBool,
}

extern "C" fn collect_word(
    _ch: SpeechChannelPtr,
    refcon: *mut c_void,
    _text: CFStringRef,
    range: CFRange,
) {
    if refcon.is_null() {
        return;
    }
    let ctx = unsafe { &*(refcon as *const CallbackCollector) };
    if let Ok(mut words) = ctx.words.lock() {
        words.push(WordEvent {
            utf16_pos: range.location.max(0) as usize,
            utf16_len: range.length.max(0) as usize,
        });
    }
}

extern "C" fn collect_done(_ch: SpeechChannelPtr, refcon: *mut c_void) {
    if refcon.is_null() {
        return;
    }
    let ctx = unsafe { &*(refcon as *const CallbackCollector) };
    ctx.done.store(true, Ordering::Release);
    unsafe { CFRunLoopStop(CFRunLoopGetCurrent()) };
}

pub struct SpeechSession<'a> {
    channel: SpeechChannelPtr,
    collector: Box<CallbackCollector>,
    _cf_text: CFString,
    done: bool,
    _owner: PhantomData<&'a Synthesizer>,
}

impl SpeechSession<'_> {
    pub fn pump(&mut self, timeout_secs: f64) -> bool {
        if self.done {
            return true;
        }
        unsafe {
            CFRunLoopRunInMode(kCFRunLoopDefaultMode, timeout_secs, 1);
        }
        if self.collector.done.load(Ordering::Acquire) {
            self.done = true;
        }
        self.done
    }

    pub fn drain_words(&mut self) -> Vec<WordEvent> {
        self.collector
            .words
            .lock()
            .map(|mut words| std::mem::take(&mut *words))
            .unwrap_or_default()
    }

    #[allow(dead_code)]
    pub fn is_done(&self) -> bool {
        self.done
    }
}

fn clear_speech_property(channel: SpeechChannelPtr, property: CFStringRef) {
    let zero = CFNumber::from(0_i64);
    unsafe {
        SetSpeechProperty(channel, property, zero.as_CFTypeRef());
    }
}

fn clear_session_callbacks(channel: SpeechChannelPtr) {
    unsafe {
        clear_speech_property(channel, kSpeechWordCFCallBack);
        clear_speech_property(channel, kSpeechSpeechDoneCallBack);
        clear_speech_property(channel, kSpeechRefConProperty);
    }
}

fn speech_options() -> CFDictionary<CFString, CFBoolean> {
    let no_ending_prosody = unsafe { CFString::wrap_under_get_rule(kSpeechNoEndingProsody) };
    let no_speech_interrupt = unsafe { CFString::wrap_under_get_rule(kSpeechNoSpeechInterrupt) };

    CFDictionary::from_CFType_pairs(&[
        (no_ending_prosody, CFBoolean::true_value()),
        (no_speech_interrupt, CFBoolean::true_value()),
    ])
}

impl Drop for SpeechSession<'_> {
    fn drop(&mut self) {
        unsafe {
            if !self.done {
                StopSpeech(self.channel);
            }
        }
        clear_session_callbacks(self.channel);
    }
}

pub struct Synthesizer {
    channel: SpeechChannelPtr,
}

impl Synthesizer {
    pub fn new(voice: Option<VoiceSpec>) -> Result<Self, SpeechError> {
        let mut channel: SpeechChannelPtr = ptr::null_mut();
        let voice_ptr = match voice {
            Some(ref v) => v as *const VoiceSpec,
            None => ptr::null(),
        };
        check(
            unsafe { NewSpeechChannel(voice_ptr, &mut channel) },
            "failed to create speech channel",
        )?;
        Ok(Self { channel })
    }

    pub fn set_rate(&self, wpm: f64) -> Result<(), SpeechError> {
        let num = CFNumber::from(wpm);
        check(
            unsafe { SetSpeechProperty(self.channel, kSpeechRateProperty, num.as_CFTypeRef()) },
            "failed to set speech rate",
        )
    }

    pub fn get_rate(&self) -> Result<f64, SpeechError> {
        let mut obj: CFTypeRef = ptr::null();
        check(
            unsafe { CopySpeechProperty(self.channel, kSpeechRateProperty, &mut obj) },
            "failed to get speech rate",
        )?;
        if obj.is_null() {
            return Ok(175.0);
        }
        let num = unsafe { CFNumber::wrap_under_create_rule(obj as _) };
        Ok(num.to_f64().unwrap_or(175.0))
    }

    pub fn start_speaking(&self, text: &str) -> Result<SpeechSession<'_>, SpeechError> {
        let collector = Box::new(CallbackCollector {
            words: Mutex::new(Vec::new()),
            done: AtomicBool::new(false),
        });

        let refcon_num =
            CFNumber::from(&*collector as *const CallbackCollector as *const c_void as i64);
        let word_num = CFNumber::from(collect_word as *const c_void as i64);
        let done_num = CFNumber::from(collect_done as *const c_void as i64);

        let cf_text = CFString::new(text);
        let options = speech_options();
        let setup_result = (|| {
            check(
                unsafe {
                    SetSpeechProperty(
                        self.channel,
                        kSpeechRefConProperty,
                        refcon_num.as_CFTypeRef(),
                    )
                },
                "failed to set refcon",
            )?;
            check(
                unsafe {
                    SetSpeechProperty(self.channel, kSpeechWordCFCallBack, word_num.as_CFTypeRef())
                },
                "failed to set word callback",
            )?;
            check(
                unsafe {
                    SetSpeechProperty(
                        self.channel,
                        kSpeechSpeechDoneCallBack,
                        done_num.as_CFTypeRef(),
                    )
                },
                "failed to set done callback",
            )?;
            check(
                unsafe {
                    SpeakCFString(
                        self.channel,
                        cf_text.as_concrete_TypeRef(),
                        options.as_concrete_TypeRef(),
                    )
                },
                "SpeakCFString failed",
            )
        })();

        if let Err(err) = setup_result {
            clear_session_callbacks(self.channel);
            return Err(err);
        }

        Ok(SpeechSession {
            channel: self.channel,
            collector,
            _cf_text: cf_text,
            done: false,
            _owner: PhantomData,
        })
    }

    pub fn speak<F: FnMut(usize, usize)>(
        &self,
        text: &str,
        mut on_word: F,
    ) -> Result<(), SpeechError> {
        let mut session = self.start_speaking(text)?;
        loop {
            let finished = session.pump(5.0);
            for ev in session.drain_words() {
                on_word(ev.utf16_pos, ev.utf16_len);
            }
            if finished {
                break;
            }
        }
        Ok(())
    }

    #[allow(dead_code)]
    pub fn stop(&self) {
        unsafe {
            StopSpeech(self.channel);
        }
    }
}

impl Drop for Synthesizer {
    fn drop(&mut self) {
        unsafe {
            DisposeSpeechChannel(self.channel);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn word_event_drain() {
        let collector = CallbackCollector {
            words: Mutex::new(Vec::new()),
            done: AtomicBool::new(false),
        };
        {
            let mut words = collector.words.lock().unwrap();
            words.push(WordEvent {
                utf16_pos: 0,
                utf16_len: 5,
            });
            words.push(WordEvent {
                utf16_pos: 6,
                utf16_len: 5,
            });
        }
        let drained: Vec<WordEvent> = std::mem::take(&mut *collector.words.lock().unwrap());
        assert_eq!(drained.len(), 2);
        assert_eq!(drained[0].utf16_pos, 0);
        assert_eq!(drained[1].utf16_pos, 6);
        assert!(collector.words.lock().unwrap().is_empty());
    }
}
