use core_foundation::base::TCFType;
use core_foundation::number::CFNumber;
use core_foundation::string::{CFString, CFStringRef};
use core_foundation_sys::base::{CFRange, CFTypeRef};
use core_foundation_sys::runloop::{
    kCFRunLoopDefaultMode, CFRunLoopGetCurrent, CFRunLoopRunInMode, CFRunLoopStop,
};
use std::ffi::c_void;
use std::fmt;
use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};

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
    fn SpeakCFString(chan: SpeechChannelPtr, text: CFStringRef, options: *const c_void) -> i16;
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
    fn SpeechBusy() -> i16;

    static kSpeechWordCFCallBack: CFStringRef;
    static kSpeechSpeechDoneCallBack: CFStringRef;
    static kSpeechRefConProperty: CFStringRef;
    static kSpeechRateProperty: CFStringRef;
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

struct CallbackContext {
    word_caller: unsafe fn(*mut c_void, usize, usize),
    word_data: *mut c_void,
    done: AtomicBool,
}

extern "C" fn word_trampoline(
    _ch: SpeechChannelPtr,
    refcon: *mut c_void,
    _text: CFStringRef,
    range: CFRange,
) {
    let ctx = unsafe { &*(refcon as *const CallbackContext) };
    unsafe { (ctx.word_caller)(ctx.word_data, range.location as usize, range.length as usize) };
}

extern "C" fn done_trampoline(_ch: SpeechChannelPtr, refcon: *mut c_void) {
    let ctx = unsafe { &*(refcon as *const CallbackContext) };
    ctx.done.store(true, Ordering::Release);
    unsafe { CFRunLoopStop(CFRunLoopGetCurrent()) };
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

    pub fn speak<F: FnMut(usize, usize)>(
        &self,
        text: &str,
        mut on_word: F,
    ) -> Result<(), SpeechError> {
        unsafe fn call_on_word<F: FnMut(usize, usize)>(data: *mut c_void, pos: usize, len: usize) {
            unsafe { (*(data as *mut F))(pos, len) };
        }

        let ctx = CallbackContext {
            word_caller: call_on_word::<F>,
            word_data: &mut on_word as *mut F as *mut c_void,
            done: AtomicBool::new(false),
        };

        let refcon_num = CFNumber::from(&ctx as *const CallbackContext as *const c_void as i64);
        check(
            unsafe {
                SetSpeechProperty(self.channel, kSpeechRefConProperty, refcon_num.as_CFTypeRef())
            },
            "failed to set refcon",
        )?;

        let word_num = CFNumber::from(word_trampoline as *const c_void as i64);
        check(
            unsafe {
                SetSpeechProperty(self.channel, kSpeechWordCFCallBack, word_num.as_CFTypeRef())
            },
            "failed to set word callback",
        )?;

        let done_num = CFNumber::from(done_trampoline as *const c_void as i64);
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

        let cf_text = CFString::new(text);
        check(
            unsafe { SpeakCFString(self.channel, cf_text.as_concrete_TypeRef(), ptr::null()) },
            "SpeakCFString failed",
        )?;

        // Block until speech completes. Use CFRunLoop to pump callbacks
        // on this thread. The done callback sets the atomic flag and
        // stops the run loop. We also poll SpeechBusy as a fallback
        // in case the done callback fires before the loop starts.
        while !ctx.done.load(Ordering::Acquire) {
            unsafe {
                // returnAfterSourceHandled=1: process ONE callback then return,
                // so word callbacks aren't delivered in bursts
                CFRunLoopRunInMode(kCFRunLoopDefaultMode, 5.0, 1);
            }
            if unsafe { SpeechBusy() } == 0 {
                break;
            }
        }

        // Keep cf_text alive through the entire speech
        drop(cf_text);

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
