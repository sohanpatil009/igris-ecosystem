// Piper FFI bindings - direct library calls, no process spawning
// Based on piper1-gpl/libpiper/include/piper.h

use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::ptr;

const PIPER_OK: i32 = 0;
const PIPER_DONE: i32 = 1;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct PiperSynthesizeOptions {
    pub speaker_id: i32,
    pub length_scale: f32,
    pub noise_scale: f32,
    pub noise_w_scale: f32,
}

#[repr(C)]
pub struct PiperAudioChunk {
    pub samples: *const f32,
    pub num_samples: usize,
    pub sample_rate: i32,
    pub is_last: bool,
    pub phonemes: *const u32,
    pub num_phonemes: usize,
    pub phoneme_ids: *const i32,
    pub num_phoneme_ids: usize,
    pub alignments: *const i32,
    pub num_alignments: usize,
}

extern "C" {
    fn piper_create(
        model_path: *const c_char,
        config_path: *const c_char,
        espeak_data_path: *const c_char,
    ) -> *mut std::ffi::c_void;

    fn piper_free(synth: *mut std::ffi::c_void);

    fn piper_default_synthesize_options(
        synth: *mut std::ffi::c_void,
    ) -> PiperSynthesizeOptions;

    fn piper_synthesize_start(
        synth: *mut std::ffi::c_void,
        text: *const c_char,
        options: *const PiperSynthesizeOptions,
    ) -> i32;

    fn piper_synthesize_next(
        synth: *mut std::ffi::c_void,
        chunk: *mut PiperAudioChunk,
    ) -> i32;
}

pub struct PiperVoice {
    synth: *mut std::ffi::c_void,
}

unsafe impl Send for PiperVoice {}
unsafe impl Sync for PiperVoice {}

impl PiperVoice {
    pub fn new(model_path: &str, config_path: &str, espeak_data_path: &str) -> Result<Self, String> {
        // Validate paths exist before calling FFI (prevents segfault)
        if !std::path::Path::new(model_path).exists() {
            return Err(format!("Model file not found: {}", model_path));
        }
        if !std::path::Path::new(config_path).exists() {
            return Err(format!("Config file not found: {}", config_path));
        }
        if !std::path::Path::new(espeak_data_path).exists() {
            return Err(format!("espeak-ng-data dir not found: {}", espeak_data_path));
        }

        let model = CString::new(model_path).map_err(|e| format!("Invalid model path: {}", e))?;
        let config = CString::new(config_path).map_err(|e| format!("Invalid config path: {}", e))?;
        let espeak = CString::new(espeak_data_path).map_err(|e| format!("Invalid espeak path: {}", e))?;

        let msg = format!("[TTS-FFI] piper_create({}, {}, {})", model_path, config_path, espeak_data_path);
        eprintln!("{}", msg);
        log_to_file(&msg);

        let synth = unsafe { piper_create(model.as_ptr(), config.as_ptr(), espeak.as_ptr()) };

        if synth.is_null() {
            let err = "piper_create returned NULL".to_string();
            eprintln!("[TTS-FFI] {}", err);
            log_to_file(&err);
            return Err(err);
        }

        let ok_msg = "[TTS-FFI] piper_create succeeded".to_string();
        eprintln!("{}", ok_msg);
        log_to_file(&ok_msg);

        Ok(Self { synth })
    }

    pub fn default_options(&self) -> PiperSynthesizeOptions {
        unsafe { piper_default_synthesize_options(self.synth) }
    }

    pub fn synthesize_wav(&self, text: &str, speaker_id: Option<i32>) -> Result<Vec<u8>, String> {
        let c_text = CString::new(text).map_err(|e| format!("Invalid text: {}", e))?;

        let mut options = self.default_options();
        if let Some(sid) = speaker_id {
            options.speaker_id = sid;
        }

        let ret = unsafe { piper_synthesize_start(self.synth, c_text.as_ptr(), &options) };
        if ret != PIPER_OK {
            return Err(format!("piper_synthesize_start failed with code {}", ret));
        }

        let mut all_samples: Vec<f32> = Vec::new();
        let mut sample_rate: i32 = 22050;

        loop {
            let mut chunk: PiperAudioChunk = unsafe { std::mem::zeroed() };
            let ret = unsafe { piper_synthesize_next(self.synth, &mut chunk) };

            if ret == PIPER_DONE {
                break;
            }
            if ret != PIPER_OK {
                return Err(format!("piper_synthesize_next failed with code {}", ret));
            }

            sample_rate = chunk.sample_rate;
            if !chunk.samples.is_null() && chunk.num_samples > 0 {
                let samples = unsafe {
                    std::slice::from_raw_parts(chunk.samples, chunk.num_samples)
                };
                all_samples.extend_from_slice(samples);
            }
        }

        if all_samples.is_empty() {
            return Err("No audio samples produced".to_string());
        }

        let wav = encode_wav(&all_samples, sample_rate as u32)?;
        Ok(wav)
    }
}

impl Drop for PiperVoice {
    fn drop(&mut self) {
        if !self.synth.is_null() {
            unsafe { piper_free(self.synth) };
            self.synth = ptr::null_mut();
        }
    }
}

fn encode_wav(samples: &[f32], sample_rate: u32) -> Result<Vec<u8>, String> {
    let num_samples = samples.len() as u32;
    let num_channels: u16 = 1;
    let bits_per_sample: u16 = 16;
    let byte_rate = sample_rate * num_channels as u32 * bits_per_sample as u32 / 8;
    let block_align = num_channels * bits_per_sample / 8;
    let data_size = num_samples * (bits_per_sample as u32 / 8);

    let mut wav = Vec::with_capacity(44 + data_size as usize);

    // RIFF header
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_size).to_le_bytes());
    wav.extend_from_slice(b"WAVE");

    // fmt chunk
    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes()); // PCM
    wav.extend_from_slice(&num_channels.to_le_bytes());
    wav.extend_from_slice(&sample_rate.to_le_bytes());
    wav.extend_from_slice(&byte_rate.to_le_bytes());
    wav.extend_from_slice(&block_align.to_le_bytes());
    wav.extend_from_slice(&bits_per_sample.to_le_bytes());

    // data chunk
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_size.to_le_bytes());

    for &s in samples {
        let clamped = s.max(-1.0).min(1.0);
        let i16_sample = (clamped * 32767.0) as i16;
        wav.extend_from_slice(&i16_sample.to_le_bytes());
    }

    Ok(wav)
}

#[cfg(target_os = "windows")]
fn log_to_file(msg: &str) {
    let _ = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("startup.log")
        .and_then(|mut f| {
            use std::io::Write;
            writeln!(f, "{}", msg)
        });
}

#[cfg(not(target_os = "windows"))]
fn log_to_file(_msg: &str) {}
