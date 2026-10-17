//! Desktop audio engine for LAN call links (Igris native).
//!
//! `attach` opens a duplex session to a handset over `eco::audio_net`:
//! the desktop's default mic streams up, peer frames play on the default
//! output. Realtime-only semantics throughout — congestion drops frames
//! instead of blocking.

use crate::eco::audio_net::{self, ActiveLink, AudioParams};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::net::IpAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex, OnceLock};

static SENDING: AtomicBool = AtomicBool::new(false);
static LINK: OnceLock<Arc<ActiveLink>> = OnceLock::new();
/// Playback pump: frames → a rodio sink fed by a dedicated thread that
/// owns the default output for the session.
static SINK_TX: OnceLock<mpsc::Sender<Vec<u8>>> = OnceLock::new();
/// Capture: cpal input stream opened on first attach; its callback lossy
/// pushes frames into the active link (frames drop when no link).
static CAPTURE: OnceLock<cpal::Stream> = OnceLock::new();

fn link() -> &'static Arc<ActiveLink> {
    LINK.get_or_init(|| Arc::new(ActiveLink::new()))
}

pub fn link_up() -> bool {
    link().params().is_some()
}

/// Attach (initiator role): connect to the handset's audio port and open
/// the duplex session. Returns immediately; the session runs until the
/// peer closes.
pub fn attach(ip: IpAddr) -> Result<(), String> {
    if SENDING
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err("audio session already active".into());
    }
    ensure_capture();
    let sink_tx = ensure_playback();
    let addr = std::net::SocketAddr::new(ip, audio_net::AUDIO_PORT);
    let l = link().clone();
    std::thread::spawn(move || {
        let rt = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(rt) => rt,
            Err(_) => {
                SENDING.store(false, Ordering::SeqCst);
                return;
            }
        };
        let result = rt.block_on(async {
            audio_net::connect(
                l,
                addr,
                AudioParams::default_params(),
                Arc::new(move |frame| {
                    let _ = sink_tx.send(frame);
                }),
                Arc::new(move |active| {
                    if !active {
                        SENDING.store(false, Ordering::SeqCst);
                    }
                }),
            )
            .await
        });
        if let Err(e) = result {
            eprintln!("[ECO] audio link {addr}: {e}");
            SENDING.store(false, Ordering::SeqCst);
        }
        let _ = rt; // keep the runtime alive as long as the task pool needs it
    });
    Ok(())
}

/// Tear down our side: future frames drop and the session ends when the
/// peer notices the silent close (or it closes itself).
pub fn detach() {
    link().clear();
    SENDING.store(false, Ordering::SeqCst);
}

/// Test hook: push a short sine burst to the handset (verifies the
/// PC→phone leg without needing a live mic).
pub fn test_tone_to_phone() -> bool {
    let params = match link().params() {
        Some(p) => p,
        None => return false,
    };
    const BURST_SECS: f32 = 2.0;
    let total = (f32::from(params.sample_rate) * BURST_SECS) as usize;
    let bytes = params.frame_bytes();
    if bytes == 0 {
        return false;
    }
    let mut samples = Vec::with_capacity(total);
    for i in 0..total {
        let t = i as f32 / f32::from(params.sample_rate);
        let env = (i as f32 / (0.01 * f32::from(params.sample_rate)).max(1.0))
            .min(1.0)
            .min(((total - i) as f32 / (0.03 * f32::from(params.sample_rate)).max(1.0)).min(1.0));
        let v = (t * 520.0 * std::f32::consts::TAU).sin() * 0.30 * env;
        samples.push((v * i16::MAX as f32) as i16);
    }
    let le: Vec<u8> = samples.into_iter().flat_map(|s| s.to_le_bytes()).collect();
    for chunk in le.chunks(bytes) {
        let mut one = chunk.to_vec();
        while one.len() < bytes {
            one.push(0);
        }
        if !link().send(one) {
            return false;
        }
    }
    true
}

fn ensure_playback() -> mpsc::Sender<Vec<u8>> {
    SINK_TX
        .get_or_init(|| {
            let (tx, rx) = mpsc::channel::<Vec<u8>>();
            std::thread::spawn(move || {
                let (stream, handle) = match rodio::OutputStream::try_default() {
                    Ok(pair) => pair,
                    Err(_) => {
                        eprintln!("[ECO] audio: no default output device");
                        return;
                    }
                };
                if let Ok(sink) = rodio::Sink::try_new(&handle) {
                    sink.set_volume(1.0);
                    for frame in rx {
                        let pcm = bytes_to_i16(&frame);
                        sink.append(rodio::buffer::SamplesBuffer::new(
                            1,
                            audio_net::SAMPLE_RATE,
                            pcm,
                        ));
                    }
                    sink.sleep_until_end();
                }
                drop(stream);
            });
            tx
        })
        .clone()
}

fn bytes_to_i16(bytes: &[u8]) -> Vec<i16> {
    let mut out = Vec::with_capacity(bytes.len() / 2);
    for c in bytes.chunks_exact(2) {
        out.push(i16::from_le_bytes([c[0], c[1]]));
    }
    out
}

/// Open the desktop mic once: cpal default input → mono i16 at the
/// device rate (WASAPI shared stereo→mono downmix) → lossy push into the
/// active link. Frames drop silently while no link is up.
fn ensure_capture() {
    if CAPTURE.get().is_some() {
        return;
    }
    let device = match cpal::default_host().default_input_device() {
        Some(d) => d,
        None => {
            eprintln!("[ECO] audio: no default input device");
            return;
        }
    };
    let config = match device.default_input_config() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[ECO] audio: input config failed: {e}");
            return;
        }
    };
    if config.sample_rate().0 != audio_net::SAMPLE_RATE {
        eprintln!(
            "[ECO] audio: input rate {} != {} (pitch artifacts possible)",
            config.sample_rate().0,
            audio_net::SAMPLE_RATE
        );
    }
    let mut flusher = SamplePusher {
        acc: Arc::new(Mutex::new(Vec::new())),
        link: link().clone(),
        rate: config.sample_rate().0,
    };
    let stream_config: cpal::StreamConfig = config.clone().into();
    let channels = stream_config.channels as usize;
    let err_fn = move |e| eprintln!("[ECO] audio input stream error: {e}");
    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => device.build_input_stream(
            &stream_config,
            move |data: &[f32], _: &cpal::InputCallbackInfo| flusher.push_f32(data, channels),
            err_fn,
            None,
        ),
        cpal::SampleFormat::I16 => device.build_input_stream(
            &stream_config,
            move |data: &[i16], _: &cpal::InputCallbackInfo| flusher.push_i16(data, channels),
            err_fn,
            None,
        ),
        other => {
            eprintln!("[ECO] audio: unsupported input format {other:?}");
            return;
        }
    };
    if let Err(e) = stream {
        eprintln!("[ECO] audio: capture stream failed: {e}");
        return;
    }
    let stream = stream.unwrap();
    stream.play().ok();
    let _ = CAPTURE.set(stream);
}

struct SamplePusher {
    acc: Arc<Mutex<Vec<i16>>>,
    link: Arc<ActiveLink>,
    rate: u32,
}

impl SamplePusher {
    fn push_i16(&mut self, data: &[i16], channels: usize) {
        let mono: Vec<i16> = if channels <= 1 {
            data.to_vec()
        } else {
            data.chunks(channels)
                .map(|c| c.iter().map(|s| *s as i32).sum::<i32>() / channels as i32)
                .map(|v| v.clamp(-32768, 32767) as i16)
                .collect()
        };
        self.enqueue(mono);
    }

    fn push_f32(&mut self, data: &[f32], channels: usize) {
        let mono: Vec<i16> = if channels <= 1 {
            data.iter()
                .map(|s| (*s * i16::MAX as f32).clamp(-32768.0, 32767.0) as i16)
                .collect()
        } else {
            data.chunks(channels)
                .map(|c| {
                    let sum: f32 = c.iter().sum();
                    (sum / channels as f32 * i16::MAX as f32)
                        .clamp(-32768.0, 32767.0) as i16
                })
                .collect()
        };
        self.enqueue(mono);
    }

    fn enqueue(&mut self, mono: Vec<i16>) {
        let mut acc = self.acc.lock().unwrap_or_else(|e| e.into_inner());
        acc.extend_from_slice(&mono);
        let frame_samples = audio_net::FRAME_SAMPLES;
        while acc.len() >= frame_samples {
            let frame: Vec<i16> = acc.drain(..frame_samples).collect();
            let bytes: Vec<u8> = frame.iter().flat_map(|s| s.to_le_bytes()).collect();
            self.link.send(bytes);
        }
    }
}
