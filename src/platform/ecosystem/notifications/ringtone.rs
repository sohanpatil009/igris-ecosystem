//! Incoming-call alert chirp while the ring toast is up (desktop
//! speakers). Procedural double-beep via rodio — no bundled audio asset —
//! stopped when the call leaves the ringing state (toast hidden).

use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

const SAMPLE_RATE: u32 = 44_100;

static ACTIVE: AtomicBool = AtomicBool::new(false);

/// Begin the chirp loop (no-op when already ringing).
pub fn start() {
    if ACTIVE.swap(true, Ordering::SeqCst) {
        return;
    }
    thread::spawn(move || {
        while ACTIVE.load(Ordering::SeqCst) {
            chirp(740.0, 0.11);
            if !ACTIVE.load(Ordering::SeqCst) {
                break;
            }
            thread::sleep(Duration::from_millis(70));
            chirp(960.0, 0.16);
            // short slices so stop() takes effect quickly
            for _ in 0..20 {
                if !ACTIVE.load(Ordering::SeqCst) {
                    break;
                }
                thread::sleep(Duration::from_millis(100));
            }
        }
        ACTIVE.store(false, Ordering::SeqCst);
    });
}

/// Stop the chirp loop (idempotent; the current chirp finishes).
pub fn stop() {
    ACTIVE.store(false, Ordering::SeqCst);
}

/// Play one short sine chirp with a click-free envelope on the default
/// output. Errors (no audio device) are swallowed — the toast carries the
/// UX already.
fn chirp(freq: f32, secs: f32) {
    let len = (SAMPLE_RATE as f32 * secs) as usize;
    let fade_in = (0.008 * SAMPLE_RATE as f32) as usize;
    let fade_out = (0.014 * SAMPLE_RATE as f32) as usize;
    let mut samples = Vec::with_capacity(len);
    for i in 0..len {
        let t = i as f32 / SAMPLE_RATE as f32;
        let env_in = if i < fade_in { i as f32 / fade_in as f32 } else { 1.0 };
        let env_out = if len - i < fade_out {
            (len - i) as f32 / fade_out as f32
        } else {
            1.0
        };
        let v = (t * freq * std::f32::consts::TAU).sin() * 0.30 * env_in.min(env_out);
        samples.push((v * i16::MAX as f32) as i16);
    }
    let (_stream, handle) = match rodio::OutputStream::try_default() {
        Ok(pair) => pair,
        Err(_) => return,
    };
    if let Ok(sink) = rodio::Sink::try_new(&handle) {
        sink.append(rodio::buffer::SamplesBuffer::new(1, SAMPLE_RATE, samples));
        sink.sleep_until_end();
    }
}
